use std::io::{self, Write};

use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::{
    Attribute as CAttribute, Color as CColor, Colors, Print, SetAttribute, SetBackgroundColor,
    SetColors, SetForegroundColor, SetUnderlineColor,
};
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use ratatui::backend::{Backend, ClearType, CrosstermBackend, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use ratatui::style::{Color, Modifier};

/// A `ratatui` backend wrapping [`CrosstermBackend`] that:
/// 1. Brackets every `draw()` call in Synchronized Output (`\x1b[?2026h` / `\x1b[?2026l`)
///    to prevent mid-frame tearing during rapid resize or drag updates.
/// 2. Forces explicit `MoveTo(x, y)` immediately after any non-ASCII cell so width
///    differences between `unicode-width` and physical terminal emulators cannot drift
///    the cursor across adjacent cells or pane borders.
pub struct SyncCrosstermBackend<W: Write> {
    inner: CrosstermBackend<W>,
}

impl<W: Write> SyncCrosstermBackend<W> {
    pub fn new(writer: W) -> Self {
        Self {
            inner: CrosstermBackend::new(writer),
        }
    }
}

impl<W: Write> Write for SyncCrosstermBackend<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        io::Write::flush(&mut self.inner)
    }
}

impl<W: Write> Backend for SyncCrosstermBackend<W> {
    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        queue!(self.inner, BeginSynchronizedUpdate)?;

        let mut fg = Color::Reset;
        let mut bg = Color::Reset;
        let mut underline_color = Color::Reset;
        let mut modifier = Modifier::empty();
        let mut last_pos: Option<Position> = None;
        let mut prev_non_ascii = false;

        for (x, y, cell) in content {
            if prev_non_ascii || !matches!(last_pos, Some(p) if x == p.x + 1 && y == p.y) {
                queue!(self.inner, MoveTo(x, y))?;
            }
            last_pos = Some(Position { x, y });
            prev_non_ascii = !cell.symbol().is_ascii();

            if cell.modifier != modifier {
                queue_modifier_diff(&mut self.inner, modifier, cell.modifier)?;
                modifier = cell.modifier;
            }
            if cell.fg != fg || cell.bg != bg {
                queue!(
                    self.inner,
                    SetColors(Colors::new(cell.fg.into(), cell.bg.into()))
                )?;
                fg = cell.fg;
                bg = cell.bg;
            }
            if cell.underline_color != underline_color {
                queue!(
                    self.inner,
                    SetUnderlineColor(CColor::from(cell.underline_color))
                )?;
                underline_color = cell.underline_color;
            }

            queue!(self.inner, Print(cell.symbol()))?;
        }

        queue!(
            self.inner,
            SetForegroundColor(CColor::Reset),
            SetBackgroundColor(CColor::Reset),
            SetUnderlineColor(CColor::Reset),
            SetAttribute(CAttribute::Reset),
            EndSynchronizedUpdate,
        )
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.inner.clear_region(clear_type)
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        self.inner.append_lines(n)
    }

    fn size(&self) -> io::Result<Size> {
        self.inner.size()
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.inner)
    }
}

const REMOVE_ATTRS: &[(Modifier, CAttribute)] = &[
    (Modifier::REVERSED, CAttribute::NoReverse),
    (Modifier::ITALIC, CAttribute::NoItalic),
    (Modifier::UNDERLINED, CAttribute::NoUnderline),
    (Modifier::CROSSED_OUT, CAttribute::NotCrossedOut),
];

const ADD_ATTRS: &[(Modifier, CAttribute)] = &[
    (Modifier::REVERSED, CAttribute::Reverse),
    (Modifier::BOLD, CAttribute::Bold),
    (Modifier::ITALIC, CAttribute::Italic),
    (Modifier::UNDERLINED, CAttribute::Underlined),
    (Modifier::DIM, CAttribute::Dim),
    (Modifier::CROSSED_OUT, CAttribute::CrossedOut),
    (Modifier::SLOW_BLINK, CAttribute::SlowBlink),
    (Modifier::RAPID_BLINK, CAttribute::RapidBlink),
];

fn queue_modifier_diff<W: Write>(w: &mut W, from: Modifier, to: Modifier) -> io::Result<()> {
    let removed = from - to;
    let added = to - from;

    for &(flag, attr) in REMOVE_ATTRS {
        if removed.contains(flag) {
            queue!(w, SetAttribute(attr))?;
        }
    }

    // SGR 22 (NormalIntensity) resets both Bold and Dim, so re-enable whichever remains active.
    if removed.intersects(Modifier::BOLD | Modifier::DIM) {
        queue!(w, SetAttribute(CAttribute::NormalIntensity))?;
        if to.contains(Modifier::BOLD) && !added.contains(Modifier::BOLD) {
            queue!(w, SetAttribute(CAttribute::Bold))?;
        }
        if to.contains(Modifier::DIM) && !added.contains(Modifier::DIM) {
            queue!(w, SetAttribute(CAttribute::Dim))?;
        }
    }

    if removed.intersects(Modifier::SLOW_BLINK | Modifier::RAPID_BLINK) {
        queue!(w, SetAttribute(CAttribute::NoBlink))?;
    }

    for &(flag, attr) in ADD_ATTRS {
        if added.contains(flag) {
            queue!(w, SetAttribute(attr))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Style;

    fn cell_with(symbol: &str, style: Style) -> Cell {
        let mut cell = Cell::default();
        cell.set_symbol(symbol);
        cell.set_style(style);
        cell
    }

    #[test]
    fn draw_wraps_frame_in_synchronized_update_sequences() {
        let mut buf = Vec::new();
        let mut backend = SyncCrosstermBackend::new(&mut buf);
        let c = cell_with("A", Style::default());

        backend.draw([(0, 0, &c)].into_iter()).unwrap();
        let out = String::from_utf8(buf).unwrap();

        assert!(
            out.starts_with("\x1b[?2026h"),
            "expected BeginSynchronizedUpdate prefix, got {out:?}"
        );
        assert!(
            out.ends_with("\x1b[?2026l"),
            "expected EndSynchronizedUpdate suffix, got {out:?}"
        );
    }

    #[test]
    fn draw_handles_empty_diff_with_balanced_sync_update() {
        let mut buf = Vec::new();
        let mut backend = SyncCrosstermBackend::new(&mut buf);

        backend.draw(std::iter::empty()).unwrap();
        let out = String::from_utf8(buf).unwrap();

        assert!(out.starts_with("\x1b[?2026h"), "got {out:?}");
        assert!(out.ends_with("\x1b[?2026l"), "got {out:?}");
    }

    #[test]
    fn draw_emits_moveto_after_non_ascii_and_elides_for_consecutive_ascii() {
        let mut buf = Vec::new();
        let mut backend = SyncCrosstermBackend::new(&mut buf);
        let s = Style::default();

        // Consecutive row 0 cells: 'A' (0,0), 'B' (1,0), '❯' (2,0), 'C' (3,0), '│' (4,0), 'D' (5,0)
        let cells = [
            cell_with("A", s),
            cell_with("B", s),
            cell_with("❯", s),
            cell_with("C", s),
            cell_with("│", s),
            cell_with("D", s),
        ];
        let items = cells
            .iter()
            .enumerate()
            .map(|(x, c)| (x as u16, 0u16, c));

        backend.draw(items).unwrap();
        let out = String::from_utf8(buf).unwrap();

        // First cell (0,0) -> MoveTo(0,0) = \x1b[1;1H
        assert!(out.contains("\x1b[1;1HA"), "got {out:?}");
        // Consecutive ASCII 'B' at (1,0) and '❯' at (2,0) follow immediately without MoveTo
        assert!(out.contains("AB❯"), "expected ASCII run elision, got {out:?}");
        // Immediately after non-ASCII '❯', cell (3,0) MUST receive explicit MoveTo(3,0) = \x1b[1;4H
        assert!(
            out.contains("❯\x1b[1;4HC"),
            "expected MoveTo after '❯' before 'C', got {out:?}"
        );
        // Immediately after non-ASCII '│' at (4,0), cell (5,0) MUST receive explicit MoveTo(5,0) = \x1b[1;6H
        assert!(
            out.contains("│\x1b[1;6HD"),
            "expected MoveTo after '│' before 'D', got {out:?}"
        );
    }

    #[test]
    fn draw_handles_modifier_diff_including_bold_dim_and_underline_color() {
        let mut buf = Vec::new();
        let mut backend = SyncCrosstermBackend::new(&mut buf);

        let c0 = cell_with(
            "a",
            Style::default()
                .add_modifier(Modifier::BOLD | Modifier::DIM | Modifier::ITALIC)
                .underline_color(Color::Red),
        );
        // Remove BOLD while keeping DIM -> must emit NormalIntensity (22m) and re-emit Dim (2m)
        let c1 = cell_with(
            "b",
            Style::default()
                .add_modifier(Modifier::DIM)
                .underline_color(Color::Reset),
        );
        // Remove DIM while keeping nothing, add UNDERLINED
        let c2 = cell_with("c", Style::default().add_modifier(Modifier::UNDERLINED));

        backend
            .draw([(0, 0, &c0), (1, 0, &c1), (2, 0, &c2)].into_iter())
            .unwrap();
        let out = String::from_utf8(buf).unwrap();

        // Bold (1m), Dim (2m), Italic (3m) present before 'a', plus underline color
        assert!(out.contains("\x1b[1m"), "missing Bold in {out:?}");
        assert!(out.contains("\x1b[2m"), "missing Dim in {out:?}");
        assert!(out.contains("\x1b[3m"), "missing Italic in {out:?}");
        assert!(out.contains("\x1b[58;5;1m"), "missing Red underline color in {out:?}");
        // Transition c0 -> c1: NoItalic (23m), NormalIntensity (22m), re-emit Dim (2m), Reset underline color (59m)
        let after_a = out.split('a').nth(1).unwrap();
        let before_b = after_a.split('b').next().unwrap();
        assert!(before_b.contains("\x1b[23m"), "missing NoItalic in {before_b:?}");
        assert!(before_b.contains("\x1b[22m"), "missing NormalIntensity in {before_b:?}");
        assert!(before_b.contains("\x1b[2m"), "missing re-emitted Dim in {before_b:?}");
        assert!(before_b.contains("\x1b[59m"), "missing underline color reset in {before_b:?}");
        // Transition c1 -> c2: NormalIntensity (22m) and Underlined (4m)
        let after_b = out.split('b').nth(1).unwrap();
        let before_c = after_b.split('c').next().unwrap();
        assert!(before_c.contains("\x1b[22m"), "missing NormalIntensity in {before_c:?}");
        assert!(before_c.contains("\x1b[4m"), "missing Underlined in {before_c:?}");
    }
}
