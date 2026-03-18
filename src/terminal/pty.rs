use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;

pub struct Pty {
    pair: portable_pty::PtyPair,
    writer: Box<dyn Write + Send>,
    _child: Box<dyn portable_pty::Child + Send + Sync>,
    rx: mpsc::Receiver<Vec<u8>>,
    _reader_thread: thread::JoinHandle<()>,
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
