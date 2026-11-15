use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::term::screen::ScreenModes;

pub fn encode_key(event: KeyEvent, modes: ScreenModes) -> Option<Vec<u8>> {
    if event.kind == KeyEventKind::Release {
        return None;
    }

    let alt = event.modifiers.contains(KeyModifiers::ALT);
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    let shift = event.modifiers.contains(KeyModifiers::SHIFT);

    let xterm_mod = xterm_modifier(shift, alt, ctrl);

    let bytes = match event.code {
        KeyCode::Char(c) => {
            if ctrl {
                let b = match c {
                    ' ' | '@' => 0x00,
                    'a'..='z' => (c as u8) - b'a' + 1,
                    'A'..='Z' => (c as u8) - b'A' + 1,
                    '[' => 0x1b,
                    '\\' => 0x1c,
                    ']' => 0x1d,
                    '^' => 0x1e,
                    '_' => 0x1f,
                    '?' => 0x7f,
                    _ => return None,
                };
                if alt {
                    vec![0x1b, b]
                } else {
                    vec![b]
                }
            } else {
                let mut out = Vec::new();
                if alt {
                    out.push(0x1b);
                }
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                out
            }
        }
        KeyCode::Enter => {
            if alt {
                vec![0x1b, b'\r']
            } else {
                vec![b'\r']
            }
        }
        KeyCode::Backspace => {
            if alt {
                vec![0x1b, 0x7f]
            } else {
                vec![0x7f]
            }
        }
        KeyCode::Tab => {
            if shift {
                b"\x1b[Z".to_vec()
            } else if alt {
                vec![0x1b, b'\t']
            } else {
                vec![b'\t']
            }
        }
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => encode_cursor_key('A', modes.app_cursor, xterm_mod),
        KeyCode::Down => encode_cursor_key('B', modes.app_cursor, xterm_mod),
        KeyCode::Right => encode_cursor_key('C', modes.app_cursor, xterm_mod),
        KeyCode::Left => encode_cursor_key('D', modes.app_cursor, xterm_mod),
        KeyCode::Home => encode_cursor_key('H', modes.app_cursor, xterm_mod),
        KeyCode::End => encode_cursor_key('F', modes.app_cursor, xterm_mod),
        KeyCode::Insert => encode_tilde_key(2, xterm_mod),
        KeyCode::Delete => encode_tilde_key(3, xterm_mod),
        KeyCode::PageUp => encode_tilde_key(5, xterm_mod),
        KeyCode::PageDown => encode_tilde_key(6, xterm_mod),
        KeyCode::F(n) => match n {
            1 => encode_ss3_or_mod('P', xterm_mod),
            2 => encode_ss3_or_mod('Q', xterm_mod),
            3 => encode_ss3_or_mod('R', xterm_mod),
            4 => encode_ss3_or_mod('S', xterm_mod),
            5 => encode_tilde_key(15, xterm_mod),
            6 => encode_tilde_key(17, xterm_mod),
            7 => encode_tilde_key(18, xterm_mod),
            8 => encode_tilde_key(19, xterm_mod),
            9 => encode_tilde_key(20, xterm_mod),
            10 => encode_tilde_key(21, xterm_mod),
            11 => encode_tilde_key(23, xterm_mod),
            12 => encode_tilde_key(24, xterm_mod),
            _ => return None,
        },
        _ => return None,
    };

    Some(bytes)
}

pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let mut out = Vec::with_capacity(text.len() + 12);
        out.extend_from_slice(b"\x1b[200~");
        out.extend_from_slice(text.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out
    } else {
        text.as_bytes().to_vec()
    }
}

fn xterm_modifier(shift: bool, alt: bool, ctrl: bool) -> Option<u8> {
    let bits = (shift as u8) | ((alt as u8) << 1) | ((ctrl as u8) << 2);
    if bits == 0 {
        None
    } else {
        Some(bits + 1)
    }
}

fn encode_cursor_key(letter: char, app_cursor: bool, xterm_mod: Option<u8>) -> Vec<u8> {
    match xterm_mod {
        Some(m) => format!("\x1b[1;{m}{letter}").into_bytes(),
        None if app_cursor => format!("\x1bO{letter}").into_bytes(),
        None => format!("\x1b[{letter}").into_bytes(),
    }
}

fn encode_ss3_or_mod(letter: char, xterm_mod: Option<u8>) -> Vec<u8> {
    match xterm_mod {
        Some(m) => format!("\x1b[1;{m}{letter}").into_bytes(),
        None => format!("\x1bO{letter}").into_bytes(),
    }
}

fn encode_tilde_key(code: u8, xterm_mod: Option<u8>) -> Vec<u8> {
    match xterm_mod {
        Some(m) => format!("\x1b[{code};{m}~").into_bytes(),
        None => format!("\x1b[{code}~").into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_cursor_and_normal_arrows() {
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(
            encode_key(up, ScreenModes::default()),
            Some(b"\x1b[A".to_vec())
        );
        let app = ScreenModes {
            app_cursor: true,
            ..Default::default()
        };
        assert_eq!(encode_key(up, app), Some(b"\x1bOA".to_vec()));
    }

    #[test]
    fn utf8_arabic_and_ctrl_keys() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(encode_key(ctrl_c, ScreenModes::default()), Some(vec![0x03]));

        let arabic_beh = KeyEvent::new(KeyCode::Char('ب'), KeyModifiers::NONE);
        assert_eq!(
            encode_key(arabic_beh, ScreenModes::default()),
            Some("ب".as_bytes().to_vec())
        );
    }

    #[test]
    fn bracketed_paste_wrapping() {
        assert_eq!(encode_paste("hi", false), b"hi");
        assert_eq!(encode_paste("hi", true), b"\x1b[200~hi\x1b[201~");
    }
}
