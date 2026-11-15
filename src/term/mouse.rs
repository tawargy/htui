use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::term::screen::{MouseProtocolEncoding, MouseProtocolMode, ScreenModes};

/// Encodes a pane-relative `MouseEvent` into xterm mouse protocol bytes
/// according to the child screen's active mouse protocol mode and encoding.
pub fn encode_mouse(
    event: MouseEvent,
    rel_col: u16,
    rel_row: u16,
    modes: ScreenModes,
) -> Option<Vec<u8>> {
    if !mode_allows_event(modes.mouse_mode, event.kind) {
        return None;
    }

    let (base_cb, is_release) = match event.kind {
        MouseEventKind::Down(btn) => (button_code(btn), false),
        MouseEventKind::Up(btn) => (button_code(btn), true),
        MouseEventKind::Drag(btn) => (button_code(btn) + 32, false),
        MouseEventKind::Moved => (3 + 32, false),
        MouseEventKind::ScrollUp => (64, false),
        MouseEventKind::ScrollDown => (65, false),
        MouseEventKind::ScrollLeft => (66, false),
        MouseEventKind::ScrollRight => (67, false),
    };

    let mod_bits = modifier_bits(event.modifiers);
    let cb = base_cb + mod_bits;
    let cx = u32::from(rel_col) + 1;
    let cy = u32::from(rel_row) + 1;

    match modes.mouse_encoding {
        MouseProtocolEncoding::Sgr => {
            let suffix = if is_release { 'm' } else { 'M' };
            Some(format!("\x1b[<{cb};{cx};{cy}{suffix}").into_bytes())
        }
        MouseProtocolEncoding::Default => {
            // X10/Normal encoding reports button 3 for all releases and caps coords at 223.
            let x10_cb = if is_release { 3 + mod_bits } else { cb };
            if cx > 223 || cy > 223 || x10_cb > 223 {
                return None;
            }
            Some(vec![
                0x1b,
                b'[',
                b'M',
                (x10_cb + 32) as u8,
                (cx + 32) as u8,
                (cy + 32) as u8,
            ])
        }
        MouseProtocolEncoding::Utf8 => {
            // UTF-8 (?1005h) encoding also reports button 3 for releases.
            let utf8_cb = if is_release { 3 + mod_bits } else { cb };
            let c_cb = char::from_u32(utf8_cb + 32)?;
            let c_cx = char::from_u32(cx + 32)?;
            let c_cy = char::from_u32(cy + 32)?;
            let mut out = Vec::with_capacity(9);
            out.extend_from_slice(b"\x1b[M");
            let mut buf = [0u8; 4];
            out.extend_from_slice(c_cb.encode_utf8(&mut buf).as_bytes());
            out.extend_from_slice(c_cx.encode_utf8(&mut buf).as_bytes());
            out.extend_from_slice(c_cy.encode_utf8(&mut buf).as_bytes());
            Some(out)
        }
    }
}

fn mode_allows_event(mode: MouseProtocolMode, kind: MouseEventKind) -> bool {
    match mode {
        MouseProtocolMode::None => false,
        MouseProtocolMode::Press => matches!(kind, MouseEventKind::Down(_)),
        MouseProtocolMode::PressRelease => matches!(
            kind,
            MouseEventKind::Down(_)
                | MouseEventKind::Up(_)
                | MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
                | MouseEventKind::ScrollLeft
                | MouseEventKind::ScrollRight
        ),
        MouseProtocolMode::ButtonMotion => !matches!(kind, MouseEventKind::Moved),
        MouseProtocolMode::AnyMotion => true,
    }
}

fn button_code(btn: MouseButton) -> u32 {
    match btn {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
}

fn modifier_bits(mods: KeyModifiers) -> u32 {
    let mut bits = 0;
    if mods.contains(KeyModifiers::SHIFT) {
        bits += 4;
    }
    if mods.contains(KeyModifiers::ALT) {
        bits += 8;
    }
    if mods.contains(KeyModifiers::CONTROL) {
        bits += 16;
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mouse_event(kind: MouseEventKind, modifiers: KeyModifiers) -> MouseEvent {
        MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers,
        }
    }

    fn sgr_modes(mode: MouseProtocolMode) -> ScreenModes {
        ScreenModes {
            mouse_mode: mode,
            mouse_encoding: MouseProtocolEncoding::Sgr,
            ..Default::default()
        }
    }

    fn default_modes(mode: MouseProtocolMode) -> ScreenModes {
        ScreenModes {
            mouse_mode: mode,
            mouse_encoding: MouseProtocolEncoding::Default,
            ..Default::default()
        }
    }

    fn utf8_modes(mode: MouseProtocolMode) -> ScreenModes {
        ScreenModes {
            mouse_mode: mode,
            mouse_encoding: MouseProtocolEncoding::Utf8,
            ..Default::default()
        }
    }

    #[test]
    fn mode_none_drops_all_events() {
        let modes = ScreenModes::default();
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE),
                0,
                0,
                modes
            ),
            None
        );
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::ScrollUp, KeyModifiers::NONE),
                0,
                0,
                modes
            ),
            None
        );
    }

    #[test]
    fn mode_press_allows_down_only() {
        let modes = sgr_modes(MouseProtocolMode::Press);
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE),
                0,
                0,
                modes
            ),
            Some(b"\x1b[<0;1;1M".to_vec())
        );
        for kind in [
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Moved,
            MouseEventKind::ScrollUp,
            MouseEventKind::ScrollDown,
            MouseEventKind::ScrollLeft,
            MouseEventKind::ScrollRight,
        ] {
            assert_eq!(
                encode_mouse(mouse_event(kind, KeyModifiers::NONE), 0, 0, modes),
                None
            );
        }
    }

    #[test]
    fn mode_press_release_allows_press_release_and_scroll() {
        let modes = sgr_modes(MouseProtocolMode::PressRelease);
        assert_eq!(
            encode_mouse(
                mouse_event(
                    MouseEventKind::Down(MouseButton::Middle),
                    KeyModifiers::NONE
                ),
                4,
                9,
                modes
            ),
            Some(b"\x1b[<1;5;10M".to_vec())
        );
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Up(MouseButton::Right), KeyModifiers::NONE),
                4,
                9,
                modes
            ),
            Some(b"\x1b[<2;5;10m".to_vec())
        );
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::ScrollUp, KeyModifiers::NONE),
                4,
                9,
                modes
            ),
            Some(b"\x1b[<64;5;10M".to_vec())
        );
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::ScrollDown, KeyModifiers::NONE),
                4,
                9,
                modes
            ),
            Some(b"\x1b[<65;5;10M".to_vec())
        );
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::ScrollLeft, KeyModifiers::NONE),
                4,
                9,
                modes
            ),
            Some(b"\x1b[<66;5;10M".to_vec())
        );
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::ScrollRight, KeyModifiers::NONE),
                4,
                9,
                modes
            ),
            Some(b"\x1b[<67;5;10M".to_vec())
        );
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Drag(MouseButton::Left), KeyModifiers::NONE),
                4,
                9,
                modes
            ),
            None
        );
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Moved, KeyModifiers::NONE),
                4,
                9,
                modes
            ),
            None
        );
    }

    #[test]
    fn mode_button_motion_allows_drag_but_drops_moved() {
        let modes = sgr_modes(MouseProtocolMode::ButtonMotion);
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Drag(MouseButton::Left), KeyModifiers::NONE),
                1,
                2,
                modes
            ),
            Some(b"\x1b[<32;2;3M".to_vec())
        );
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Drag(MouseButton::Right), KeyModifiers::NONE),
                1,
                2,
                modes
            ),
            Some(b"\x1b[<34;2;3M".to_vec())
        );
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Moved, KeyModifiers::NONE),
                1,
                2,
                modes
            ),
            None
        );
    }

    #[test]
    fn mode_any_motion_allows_moved() {
        let modes = sgr_modes(MouseProtocolMode::AnyMotion);
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Moved, KeyModifiers::NONE),
                10,
                20,
                modes
            ),
            Some(b"\x1b[<35;11;21M".to_vec())
        );
    }

    #[test]
    fn modifiers_and_large_sgr_coordinates() {
        let modes = sgr_modes(MouseProtocolMode::AnyMotion);
        let mods = KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL;
        // Left (0) + Shift (4) + Alt (8) + Ctrl (16) = 28
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Down(MouseButton::Left), mods),
                299,
                249,
                modes
            ),
            Some(b"\x1b[<28;300;250M".to_vec())
        );
        // Up preserves button number in SGR
        assert_eq!(
            encode_mouse(
                mouse_event(
                    MouseEventKind::Up(MouseButton::Middle),
                    KeyModifiers::CONTROL
                ),
                250,
                10,
                modes
            ),
            Some(b"\x1b[<17;251;11m".to_vec())
        );
    }

    #[test]
    fn default_x10_encoding_and_bounds() {
        let modes = default_modes(MouseProtocolMode::PressRelease);
        // Down Left at (0, 0) -> cx=1, cy=1 -> cb=0 -> [0x1b, '[', 'M', 32, 33, 33]
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE),
                0,
                0,
                modes
            ),
            Some(vec![0x1b, b'[', b'M', 32, 33, 33])
        );
        // Up uses button 3 in X10 encoding -> cb = 3 + modifier
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Up(MouseButton::Left), KeyModifiers::ALT),
                0,
                0,
                modes
            ),
            Some(vec![0x1b, b'[', b'M', 3 + 8 + 32, 33, 33])
        );
        // Max valid coordinate: rel_col = 222 -> cx = 223 -> 223 + 32 = 255
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE),
                222,
                222,
                modes
            ),
            Some(vec![0x1b, b'[', b'M', 32, 255, 255])
        );
        // Out-of-range coordinate: rel_col = 223 -> cx = 224 > 223 -> None
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE),
                223,
                0,
                modes
            ),
            None
        );
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE),
                0,
                223,
                modes
            ),
            None
        );
    }

    #[test]
    fn utf8_encoding_supports_coords_above_95_and_up_button_3() {
        let modes = utf8_modes(MouseProtocolMode::PressRelease);
        // rel_col = 199 -> cx = 200 -> 200 + 32 = 232 ('è' in UTF-8: [0xc3, 0xa8])
        let mut expected = vec![0x1b, b'[', b'M', 32];
        let mut buf = [0u8; 4];
        expected.extend_from_slice(
            char::from_u32(200 + 32)
                .unwrap()
                .encode_utf8(&mut buf)
                .as_bytes(),
        );
        expected.extend_from_slice(
            char::from_u32(1 + 32)
                .unwrap()
                .encode_utf8(&mut buf)
                .as_bytes(),
        );
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE),
                199,
                0,
                modes
            ),
            Some(expected)
        );

        // Up uses button 3 in Utf8 encoding
        assert_eq!(
            encode_mouse(
                mouse_event(MouseEventKind::Up(MouseButton::Right), KeyModifiers::NONE),
                0,
                0,
                modes
            ),
            Some(vec![0x1b, b'[', b'M', 35, 33, 33])
        );
    }
}
