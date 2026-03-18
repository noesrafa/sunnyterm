use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// Translates winit key events into byte sequences to send to the PTY.
pub fn key_to_pty_bytes(event: &KeyEvent, modifiers: ModifiersState) -> Option<Vec<u8>> {
    if event.state != ElementState::Pressed {
        return None;
    }

    let ctrl = modifiers.control_key();
    let alt = modifiers.alt_key();
    let super_key = modifiers.super_key();

    // Ignore Cmd+key combos (handled as app shortcuts) except specific ones
    if super_key {
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                // Cmd+Backspace: delete entire line (Ctrl+U)
                return Some(vec![0x15]);
            }
            _ => return None,
        }
    }

    match &event.logical_key {
        Key::Named(named) => {
            let bytes = match named {
                NamedKey::Space => b" ".to_vec(),
                NamedKey::Enter => b"\r".to_vec(),
                NamedKey::Backspace => {
                    if ctrl {
                        // Ctrl+Backspace: delete word backward (send Ctrl+W)
                        vec![0x17]
                    } else if alt {
                        // Alt+Backspace: delete word backward
                        vec![0x1b, 0x7f]
                    } else {
                        vec![0x7f]
                    }
                }
                NamedKey::Tab => b"\t".to_vec(),
                NamedKey::Escape => vec![0x1b],
                NamedKey::ArrowUp => b"\x1b[A".to_vec(),
                NamedKey::ArrowDown => b"\x1b[B".to_vec(),
                NamedKey::ArrowRight => {
                    if alt {
                        // Alt+Right: move word forward
                        b"\x1bf".to_vec()
                    } else if ctrl {
                        b"\x1b[1;5C".to_vec()
                    } else {
                        b"\x1b[C".to_vec()
                    }
                }
                NamedKey::ArrowLeft => {
                    if alt {
                        // Alt+Left: move word backward
                        b"\x1bb".to_vec()
                    } else if ctrl {
                        b"\x1b[1;5D".to_vec()
                    } else {
                        b"\x1b[D".to_vec()
                    }
                }
                NamedKey::Home => b"\x1b[H".to_vec(),
                NamedKey::End => b"\x1b[F".to_vec(),
                NamedKey::PageUp => b"\x1b[5~".to_vec(),
                NamedKey::PageDown => b"\x1b[6~".to_vec(),
                NamedKey::Insert => b"\x1b[2~".to_vec(),
                NamedKey::Delete => {
                    if ctrl {
                        // Ctrl+Delete: delete word forward
                        b"\x1b[3;5~".to_vec()
                    } else {
                        b"\x1b[3~".to_vec()
                    }
                }
                NamedKey::F1 => b"\x1bOP".to_vec(),
                NamedKey::F2 => b"\x1bOQ".to_vec(),
                NamedKey::F3 => b"\x1bOR".to_vec(),
                NamedKey::F4 => b"\x1bOS".to_vec(),
                NamedKey::F5 => b"\x1b[15~".to_vec(),
                NamedKey::F6 => b"\x1b[17~".to_vec(),
                NamedKey::F7 => b"\x1b[18~".to_vec(),
                NamedKey::F8 => b"\x1b[19~".to_vec(),
                NamedKey::F9 => b"\x1b[20~".to_vec(),
                NamedKey::F10 => b"\x1b[21~".to_vec(),
                NamedKey::F11 => b"\x1b[23~".to_vec(),
                NamedKey::F12 => b"\x1b[24~".to_vec(),
                _ => return None,
            };
            Some(bytes)
        }
        Key::Character(s) => {
            if ctrl {
                if let Some(c) = s.chars().next() {
                    let code = match c {
                        'a'..='z' => Some((c as u8 - b'a' + 1) as u8),
                        '@' => Some(0),
                        '[' => Some(0x1b),
                        '\\' => Some(0x1c),
                        ']' => Some(0x1d),
                        '^' => Some(0x1e),
                        '_' => Some(0x1f),
                        _ => None,
                    };
                    code.map(|c| vec![c])
                } else {
                    None
                }
            } else if alt {
                // Alt+key: send ESC prefix
                let mut bytes = vec![0x1b];
                bytes.extend(s.as_bytes());
                Some(bytes)
            } else {
                Some(s.as_bytes().to_vec())
            }
        }
        _ => None,
    }
}
