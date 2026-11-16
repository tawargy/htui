use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseProtocolMode {
    #[default]
    None,
    Press,
    PressRelease,
    ButtonMotion,
    AnyMotion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseProtocolEncoding {
    #[default]
    Default,
    Utf8,
    Sgr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScreenModes {
    pub app_cursor: bool,
    pub bracketed_paste: bool,
    pub alternate_screen: bool,
    pub mouse_mode: MouseProtocolMode,
    pub mouse_encoding: MouseProtocolEncoding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenCell {
    pub symbol: String,
    pub fg: Color,
    pub bg: Color,
    pub modifiers: Modifier,
    pub wide_continuation: bool,
}

pub trait ScreenModel: Send {
    fn process(&mut self, bytes: &[u8]);
    fn resize(&mut self, cols: u16, rows: u16);
    fn size(&self) -> (u16, u16);
    fn cursor(&self) -> (u16, u16, bool);
    fn modes(&self) -> ScreenModes;
    fn scrollback(&self) -> usize;
    fn set_scrollback(&mut self, offset: usize);
    fn cell(&self, row: u16, col: u16) -> Option<ScreenCell>;
    fn row_text(&self, row: u16) -> String;
    fn render_to_buffer(&self, area: Rect, buf: &mut Buffer);
}

pub struct Vt100Screen {
    parser: vt100::Parser,
}

impl Vt100Screen {
    pub fn new(cols: u16, rows: u16, scrollback_len: usize) -> Self {
        Self {
            parser: vt100::Parser::new(rows.max(1), cols.max(1), scrollback_len),
        }
    }
}

impl ScreenModel for Vt100Screen {
    fn process(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        self.parser.set_size(rows.max(1), cols.max(1));
    }

    fn size(&self) -> (u16, u16) {
        let (rows, cols) = self.parser.screen().size();
        (cols, rows)
    }

    fn cursor(&self) -> (u16, u16, bool) {
        let screen = self.parser.screen();
        let (row, col) = screen.cursor_position();
        (col, row, !screen.hide_cursor())
    }

    fn modes(&self) -> ScreenModes {
        let screen = self.parser.screen();
        ScreenModes {
            app_cursor: screen.application_cursor(),
            bracketed_paste: screen.bracketed_paste(),
            alternate_screen: screen.alternate_screen(),
            mouse_mode: map_vt100_mouse_mode(screen.mouse_protocol_mode()),
            mouse_encoding: map_vt100_mouse_encoding(screen.mouse_protocol_encoding()),
        }
    }

    fn scrollback(&self) -> usize {
        self.parser.screen().scrollback()
    }

    fn set_scrollback(&mut self, offset: usize) {
        self.parser.set_scrollback(offset);
    }

    fn cell(&self, row: u16, col: u16) -> Option<ScreenCell> {
        let cell = self.parser.screen().cell(row, col)?;
        let raw = cell.contents();
        let symbol = if raw.is_empty() {
            " ".to_string()
        } else {
            let candidate = preserve_first_grapheme(&raw);
            if candidate.chars().any(char::is_control)
                || UnicodeWidthStr::width(candidate.as_str()) == 0
            {
                " ".to_string()
            } else {
                candidate
            }
        };

        let mut modifiers = Modifier::empty();
        if cell.bold() {
            modifiers |= Modifier::BOLD;
        }
        if cell.italic() {
            modifiers |= Modifier::ITALIC;
        }
        if cell.underline() {
            modifiers |= Modifier::UNDERLINED;
        }
        if cell.inverse() {
            modifiers |= Modifier::REVERSED;
        }

        Some(ScreenCell {
            symbol,
            fg: map_vt100_color(cell.fgcolor()),
            bg: map_vt100_color(cell.bgcolor()),
            modifiers,
            wide_continuation: cell.is_wide_continuation(),
        })
    }

    fn row_text(&self, row: u16) -> String {
        let (cols, rows) = self.size();
        if row >= rows {
            return String::new();
        }
        let mut out = String::new();
        for col in 0..cols {
            if let Some(c) = self.cell(row, col) {
                if !c.wide_continuation {
                    out.push_str(&c.symbol);
                }
            }
        }
        out.trim_end().to_string()
    }

    fn render_to_buffer(&self, area: Rect, buf: &mut Buffer) {
        let (screen_cols, screen_rows) = self.size();
        let max_rows = area.height.min(screen_rows);
        let max_cols = area.width.min(screen_cols);

        for r in 0..area.height {
            let mut skip_cols: u16 = 0;
            for c in 0..area.width {
                let bx = area.x + c;
                let by = area.y + r;
                let Some(buf_cell) = buf.cell_mut((bx, by)) else {
                    continue;
                };
                buf_cell.reset();

                if r >= max_rows || c >= max_cols {
                    continue;
                }
                if skip_cols > 0 {
                    skip_cols -= 1;
                    continue;
                }

                let Some(sc) = self.cell(r, c) else {
                    continue;
                };
                let style = Style::default()
                    .fg(sc.fg)
                    .bg(sc.bg)
                    .add_modifier(sc.modifiers);

                if sc.wide_continuation {
                    buf_cell.set_symbol(" ");
                    buf_cell.set_style(style);
                    continue;
                }

                let sym_width = UnicodeWidthStr::width(sc.symbol.as_str());
                if sym_width == 0
                    || sym_width > 2
                    || (sym_width > 1 && c as usize + sym_width > area.width as usize)
                {
                    buf_cell.set_symbol(" ");
                    buf_cell.set_style(style);
                } else {
                    buf_cell.set_symbol(&sc.symbol);
                    buf_cell.set_style(style);
                    if sym_width > 1 {
                        skip_cols = (sym_width - 1) as u16;
                    }
                }
            }
        }
    }
}

/// Preserves the complete first UTF-8 grapheme cluster (base character + all
/// combining marks such as Arabic harakat/diacritics, ZWJ, ZWNJ) without
/// byte-slicing or dropping combining codepoints.
pub fn preserve_first_grapheme(s: &str) -> String {
    s.graphemes(true)
        .next()
        .map(String::from)
        .unwrap_or_else(|| " ".to_string())
}

fn map_vt100_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn map_vt100_mouse_mode(m: vt100::MouseProtocolMode) -> MouseProtocolMode {
    match m {
        vt100::MouseProtocolMode::None => MouseProtocolMode::None,
        vt100::MouseProtocolMode::Press => MouseProtocolMode::Press,
        vt100::MouseProtocolMode::PressRelease => MouseProtocolMode::PressRelease,
        vt100::MouseProtocolMode::ButtonMotion => MouseProtocolMode::ButtonMotion,
        vt100::MouseProtocolMode::AnyMotion => MouseProtocolMode::AnyMotion,
    }
}

fn map_vt100_mouse_encoding(e: vt100::MouseProtocolEncoding) -> MouseProtocolEncoding {
    match e {
        vt100::MouseProtocolEncoding::Default => MouseProtocolEncoding::Default,
        vt100::MouseProtocolEncoding::Utf8 => MouseProtocolEncoding::Utf8,
        vt100::MouseProtocolEncoding::Sgr => MouseProtocolEncoding::Sgr,
    }
}
