use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

extern "C" {
    fn proc_listchildpids(ppid: libc::pid_t, buffer: *mut libc::pid_t, buffersize: libc::c_int) -> libc::c_int;
    fn proc_pidinfo(
        pid: libc::c_int,
        flavor: libc::c_int,
        arg: u64,
        buffer: *mut libc::c_void,
        buffersize: libc::c_int,
    ) -> libc::c_int;
}

const PROC_PIDVNODEPATHINFO: libc::c_int = 9;
// proc_vnodepathinfo: pvi_cdir (vnode_info 152 + path 1024) + pvi_rdir (same)
const VNODEPATHINFO_SIZE: usize = (152 + 1024) * 2;
// cdir path starts at offset 152 (after vnode_info in pvi_cdir)
const CDIR_PATH_OFFSET: usize = 152;


pub struct Pty {
    pair: portable_pty::PtyPair,
    writer: Box<dyn Write + Send>,
    _child: Box<dyn portable_pty::Child + Send + Sync>,
    rx: mpsc::Receiver<Vec<u8>>,
    _reader_thread: thread::JoinHandle<()>,
    child_pid: u32,
}

impl Pty {
    pub fn spawn(shell: &str, cols: u16, rows: u16) -> anyhow::Result<Self> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(shell);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let child = pair.slave.spawn_command(cmd)?;
        let child_pid = child.process_id().unwrap_or(0);
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        // Read PTY output in a background thread to avoid blocking the event loop
        let (tx, rx) = mpsc::channel();
        let reader_thread = thread::spawn(move || {
            let mut buf = [0u8; 65536];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            pair,
            writer,
            _child: child,
            rx,
            _reader_thread: reader_thread,
            child_pid,
        })
    }

    /// Non-blocking: returns all available PTY output, or empty vec if none.
    pub fn try_read(&mut self) -> Vec<u8> {
        let mut data = Vec::new();
        while let Ok(chunk) = self.rx.try_recv() {
            data.extend(chunk);
        }
        data
    }

    pub fn write(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()
    }

    /// Check if the shell has a foreground child process running.
    /// When true, input should bypass the input buffer and go directly to the PTY.
    pub fn has_foreground_child(&self) -> bool {
        if self.child_pid == 0 { return false; }
        let mut pids = [0i32; 64];
        let result = unsafe {
            proc_listchildpids(
                self.child_pid as i32,
                pids.as_mut_ptr(),
                (pids.len() * std::mem::size_of::<i32>()) as libc::c_int,
            )
        };
        result > 0
    }

    /// Get the current working directory of the shell process.
    pub fn cwd(&self) -> Option<PathBuf> {
        if self.child_pid == 0 { return None; }
        let mut buf = vec![0u8; VNODEPATHINFO_SIZE];
        let ret = unsafe {
            proc_pidinfo(
                self.child_pid as i32,
                PROC_PIDVNODEPATHINFO,
                0,
                buf.as_mut_ptr() as *mut libc::c_void,
                VNODEPATHINFO_SIZE as libc::c_int,
            )
        };
        if ret <= 0 { return None; }
        // Extract null-terminated path string from the cdir offset
        let path_bytes = &buf[CDIR_PATH_OFFSET..CDIR_PATH_OFFSET + 1024];
        let end = path_bytes.iter().position(|&b| b == 0).unwrap_or(1024);
        let path_str = std::str::from_utf8(&path_bytes[..end]).ok()?;
        if path_str.is_empty() { return None; }
        Some(PathBuf::from(path_str))
    }

    pub fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        self.pair.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }
}
