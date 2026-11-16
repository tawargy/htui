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

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopEventListener;

impl alacritty_terminal::event::EventListener for NoopEventListener {
    fn send_event(&self, _event: alacritty_terminal::event::Event) {}
}

pub struct AlacrittyScreen {
    term: alacritty_terminal::Term<NoopEventListener>,
    parser: alacritty_terminal::vte::ansi::Processor,
    logical_cols: u16,
    logical_rows: u16,
}

struct ScreenDims {
    cols: usize,
    rows: usize,
}

impl ScreenDims {
    fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols: (cols as usize).max(alacritty_terminal::term::MIN_COLUMNS),
            rows: (rows as usize).max(alacritty_terminal::term::MIN_SCREEN_LINES),
        }
    }
}

impl alacritty_terminal::grid::Dimensions for ScreenDims {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

impl AlacrittyScreen {
    pub fn new(cols: u16, rows: u16, scrollback_len: usize) -> Self {
        let logical_cols = cols.max(1);
        let logical_rows = rows.max(1);
        let dims = ScreenDims::new(logical_cols, logical_rows);
        let config = alacritty_terminal::term::Config {
            scrolling_history: scrollback_len,
            ..alacritty_terminal::term::Config::default()
        };
        Self {
            term: alacritty_terminal::Term::new(config, &dims, NoopEventListener),
            parser: alacritty_terminal::vte::ansi::Processor::new(),
            logical_cols,
            logical_rows,
        }
    }
}

impl ScreenModel for AlacrittyScreen {
    fn process(&mut self, bytes: &[u8]) {
        if let Some(deadline) = self.parser.sync_timeout().sync_timeout() {
            if std::time::Instant::now() >= deadline {
                self.parser.stop_sync(&mut self.term);
            }
        }
        self.parser.advance(&mut self.term, bytes);
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        self.logical_cols = cols.max(1);
        self.logical_rows = rows.max(1);
        let dims = ScreenDims::new(self.logical_cols, self.logical_rows);
        self.term.resize(dims);
    }

    fn size(&self) -> (u16, u16) {
        (self.logical_cols, self.logical_rows)
    }

    fn cursor(&self) -> (u16, u16, bool) {
        let grid = self.term.grid();
        let display_offset = grid.display_offset();
        let visible = self
            .term
            .mode()
            .contains(alacritty_terminal::term::TermMode::SHOW_CURSOR)
            && display_offset == 0;
        let point = grid.cursor.point;
        let col = (point.column.0.max(0) as u16).min(self.logical_cols.saturating_sub(1));
        let row = (point.line.0.max(0) as u16).min(self.logical_rows.saturating_sub(1));
        (col, row, visible)
    }

    fn modes(&self) -> ScreenModes {
        map_alacritty_modes(*self.term.mode())
    }

    fn scrollback(&self) -> usize {
        self.term.grid().display_offset()
    }

    fn set_scrollback(&mut self, offset: usize) {
        use alacritty_terminal::grid::Dimensions;
        let grid = self.term.grid();
        let target = offset.min(grid.history_size());
        let current = grid.display_offset();
        let delta =
            (target as isize - current as isize).clamp(i32::MIN as isize, i32::MAX as isize) as i32;
        if delta != 0 {
            self.term
                .scroll_display(alacritty_terminal::grid::Scroll::Delta(delta));
        }
    }

    fn cell(&self, row: u16, col: u16) -> Option<ScreenCell> {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Column, Line};
        use alacritty_terminal::term::cell::Flags;

        if row >= self.logical_rows || col >= self.logical_cols {
            return None;
        }
        let grid = self.term.grid();
        let line = Line(row as i32) - grid.display_offset();
        if line < grid.topmost_line() || line > grid.bottommost_line() {
            return None;
        }
        let grid_cell = &grid[line][Column(col as usize)];

        let mut raw = String::new();
        if grid_cell.c != '\0' {
            raw.push(grid_cell.c);
        }
        if let Some(zw) = grid_cell.zerowidth() {
            for &ch in zw {
                raw.push(ch);
            }
        }
        let symbol = sanitize_cell_symbol(&raw);

        let colors = self.term.colors();
        let (fg, fg_dim) = map_alacritty_color(grid_cell.fg, colors);
        let (bg, bg_dim) = map_alacritty_color(grid_cell.bg, colors);
        let mut modifiers = map_alacritty_modifiers(grid_cell.flags);
        if fg_dim || bg_dim {
            modifiers |= Modifier::DIM;
        }

        Some(ScreenCell {
            symbol,
            fg,
            bg,
            modifiers,
            wide_continuation: grid_cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER),
        })
    }

    fn row_text(&self, row: u16) -> String {
        screen_row_text(self, row)
    }

    fn render_to_buffer(&self, area: Rect, buf: &mut Buffer) {
        screen_render_to_buffer(self, area, buf);
    }
}

/// Strips control, format (`Cf`/soft-hyphen), and variation-selector codepoints
/// before extracting the first grapheme so ZWJ/VS16 clusters cannot merge
/// multiple wide glyphs or skew width between the terminal grid and ratatui.
fn sanitize_cell_symbol(raw: &str) -> String {
    if raw.is_empty() {
        return " ".to_string();
    }
    let cleaned: String = raw
        .chars()
        .filter(|&c| !c.is_control() && !is_invisible_format_or_vs(c))
        .collect();
    if cleaned.is_empty() {
        return " ".to_string();
    }
    let candidate = preserve_first_grapheme(&cleaned);
    if UnicodeWidthStr::width(candidate.as_str()) == 0 {
        " ".to_string()
    } else {
        candidate
    }
}

fn screen_row_text(screen: &dyn ScreenModel, row: u16) -> String {
    let (cols, rows) = screen.size();
    if row >= rows {
        return String::new();
    }
    let mut out = String::new();
    for col in 0..cols {
        if let Some(c) = screen.cell(row, col) {
            if !c.wide_continuation {
                out.push_str(&c.symbol);
            }
        }
    }
    out.trim_end().to_string()
}

fn screen_render_to_buffer(screen: &dyn ScreenModel, area: Rect, buf: &mut Buffer) {
    let (screen_cols, screen_rows) = screen.size();
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

            let Some(sc) = screen.cell(r, c) else {
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

fn map_alacritty_modes(mode: alacritty_terminal::term::TermMode) -> ScreenModes {
    use alacritty_terminal::term::TermMode;
    let mouse_mode = if mode.contains(TermMode::MOUSE_MOTION) {
        MouseProtocolMode::AnyMotion
    } else if mode.contains(TermMode::MOUSE_DRAG) {
        MouseProtocolMode::ButtonMotion
    } else if mode.contains(TermMode::MOUSE_REPORT_CLICK) {
        MouseProtocolMode::PressRelease
    } else {
        MouseProtocolMode::None
    };
    let mouse_encoding = if mode.contains(TermMode::SGR_MOUSE) {
        MouseProtocolEncoding::Sgr
    } else if mode.contains(TermMode::UTF8_MOUSE) {
        MouseProtocolEncoding::Utf8
    } else {
        MouseProtocolEncoding::Default
    };
    ScreenModes {
        app_cursor: mode.contains(TermMode::APP_CURSOR),
        bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
        alternate_screen: mode.contains(TermMode::ALT_SCREEN),
        mouse_mode,
        mouse_encoding,
    }
}

fn map_alacritty_modifiers(flags: alacritty_terminal::term::cell::Flags) -> Modifier {
    use alacritty_terminal::term::cell::Flags;
    let mut modifiers = Modifier::empty();
    if flags.contains(Flags::BOLD) {
        modifiers |= Modifier::BOLD;
    }
    if flags.contains(Flags::DIM) {
        modifiers |= Modifier::DIM;
    }
    if flags.contains(Flags::ITALIC) {
        modifiers |= Modifier::ITALIC;
    }
    if flags.intersects(Flags::ALL_UNDERLINES) {
        modifiers |= Modifier::UNDERLINED;
    }
    if flags.contains(Flags::INVERSE) {
        modifiers |= Modifier::REVERSED;
    }
    if flags.contains(Flags::STRIKEOUT) {
        modifiers |= Modifier::CROSSED_OUT;
    }
    modifiers
}

fn map_alacritty_color(
    color: alacritty_terminal::vte::ansi::Color,
    colors: &alacritty_terminal::term::color::Colors,
) -> (Color, bool) {
    use alacritty_terminal::vte::ansi::{Color as VteColor, NamedColor};
    match color {
        VteColor::Spec(rgb) => (Color::Rgb(rgb.r, rgb.g, rgb.b), false),
        VteColor::Indexed(idx) => {
            if let Some(rgb) = colors[idx as usize] {
                (Color::Rgb(rgb.r, rgb.g, rgb.b), false)
            } else {
                (Color::Indexed(idx), false)
            }
        }
        VteColor::Named(named) => {
            if let Some(rgb) = colors[named] {
                return (Color::Rgb(rgb.r, rgb.g, rgb.b), false);
            }
            match named {
                NamedColor::Black
                | NamedColor::Red
                | NamedColor::Green
                | NamedColor::Yellow
                | NamedColor::Blue
                | NamedColor::Magenta
                | NamedColor::Cyan
                | NamedColor::White
                | NamedColor::BrightBlack
                | NamedColor::BrightRed
                | NamedColor::BrightGreen
                | NamedColor::BrightYellow
                | NamedColor::BrightBlue
                | NamedColor::BrightMagenta
                | NamedColor::BrightCyan
                | NamedColor::BrightWhite => (Color::Indexed(named as u8), false),
                NamedColor::DimBlack
                | NamedColor::DimRed
                | NamedColor::DimGreen
                | NamedColor::DimYellow
                | NamedColor::DimBlue
                | NamedColor::DimMagenta
                | NamedColor::DimCyan
                | NamedColor::DimWhite => {
                    let idx = (named as usize - NamedColor::DimBlack as usize) as u8;
                    (Color::Indexed(idx), true)
                }
                NamedColor::Foreground
                | NamedColor::Background
                | NamedColor::Cursor
                | NamedColor::BrightForeground
                | NamedColor::DimForeground => (Color::Reset, false),
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

/// Returns true for soft hyphen, Unicode `Cf` format characters (BIDI overrides/isolates,
/// zero-width joiners/spaces, BOM), and variation selectors (`VS1..=VS16`, `VS17..=VS256`).
fn is_invisible_format_or_vs(ch: char) -> bool {
    matches!(
        ch,
        '\u{00AD}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
            | '\u{E0100}'..='\u{E01EF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rep_csi_b_places_right_aligned_text_at_expected_columns() {
        let mut screen = AlacrittyScreen::new(30, 3, 100);
        // "1%" (2 cols) + 23 spaces via CSI 23 b (`REP`) + "20:56" (5 cols) = 30 cols
        screen.process(b"1% \x1b[22b20:56");
        let row = screen.row_text(0);
        assert_eq!(row, "1%                       20:56");
        assert_eq!(screen.cell(0, 25).unwrap().symbol, "2");
        assert_eq!(screen.cell(0, 29).unwrap().symbol, "6");
    }

    #[test]
    fn hvp_cnl_cpl_hpa_and_ech_sequences_work() {
        let mut screen = AlacrittyScreen::new(20, 6, 100);
        // HVP (row 2, col 3) -> "AB", CNL 2 -> row 4 col 1 "CD", CPL 1 -> row 3 col 1 "EF"
        screen.process(b"\x1b[2;3fAB\x1b[2ECD\x1b[1FEF");
        assert_eq!(screen.row_text(1), "  AB");
        assert_eq!(screen.row_text(2), "EF");
        assert_eq!(screen.row_text(3), "CD");

        // HPA (`) to col 5 on row 1, write "12345", then move to col 2 and ECH 2 chars
        screen.process(b"\x1b[1;1H\x1b[5`12345\x1b[6`X\x1b[6`\x1b[2X");
        let row0 = screen.row_text(0);
        assert_eq!(row0, "    1  45");
        assert_eq!(screen.cell(0, 4).unwrap().symbol, "1");
        assert_eq!(screen.cell(0, 5).unwrap().symbol, " ");
        assert_eq!(screen.cell(0, 6).unwrap().symbol, " ");
        assert_eq!(screen.cell(0, 7).unwrap().symbol, "4");
    }

    #[test]
    fn colon_sgr_truecolor_and_undercurl_map_correctly() {
        let mut screen = AlacrittyScreen::new(20, 2, 100);
        // Colon-separated 24-bit fg + undercurl (4:3) + dim + strikethrough
        screen.process(b"\x1b[38:2::12:34:56;4:3;2;9mX\x1b[0m");
        let cell = screen.cell(0, 0).unwrap();
        assert_eq!(cell.symbol, "X");
        assert_eq!(cell.fg, Color::Rgb(12, 34, 56));
        assert!(cell.modifiers.contains(Modifier::UNDERLINED));
        assert!(cell.modifiers.contains(Modifier::DIM));
        assert!(cell.modifiers.contains(Modifier::CROSSED_OUT));
    }

    #[test]
    fn cols_one_wide_char_safety_and_logical_size() {
        let mut screen = AlacrittyScreen::new(1, 3, 100);
        assert_eq!(screen.size(), (1, 3));
        // Writing a width-2 CJK char into a logical 1-col screen must not panic
        screen.process("漢a".as_bytes());
        assert_eq!(screen.size(), (1, 3));
        assert!(screen.cell(0, 0).is_some());
        assert!(screen.cell(0, 1).is_none());
        let (_, _, _) = screen.cursor();

        // Resizing down to 1 col must also preserve logical size (1, 3)
        let mut s2 = AlacrittyScreen::new(10, 3, 100);
        s2.process("漢".as_bytes());
        s2.resize(1, 3);
        assert_eq!(s2.size(), (1, 3));
        assert!(s2.cell(0, 1).is_none());
    }

    #[test]
    fn arabic_harakat_and_cf_vs_zwj_sanitization() {
        let mut screen = AlacrittyScreen::new(30, 4, 100);
        screen.process("\u{26A0}\u{FE0F}X \u{1F468}\u{200D}\u{1F469}Y\r\n".as_bytes());
        screen.process("a\u{00AD}b\u{202E}c\r\n".as_bytes());
        screen.process("بِ لَّ شُ".as_bytes());

        assert_eq!(screen.cell(0, 0).unwrap().symbol, "\u{26A0}");
        assert_eq!(screen.cell(0, 1).unwrap().symbol, "X");
        assert_eq!(screen.cell(0, 3).unwrap().symbol, "\u{1F468}");
        assert!(screen.cell(0, 4).unwrap().wide_continuation);
        assert_eq!(screen.row_text(1), "abc");
        let row2 = screen.row_text(2);
        assert!(row2.contains("بِ"));
        assert!(row2.contains("لَّ"));
        assert!(row2.contains("شُ"));
    }

    #[test]
    fn scrollback_and_cursor_visibility() {
        let mut screen = AlacrittyScreen::new(20, 3, 50);
        for i in 0..6 {
            screen.process(format!("line{i}\r\n").as_bytes());
        }
        let (_, _, visible_at_bottom) = screen.cursor();
        assert!(visible_at_bottom);
        assert_eq!(screen.scrollback(), 0);

        screen.set_scrollback(3);
        assert_eq!(screen.scrollback(), 3);
        let (_, _, visible_in_history) = screen.cursor();
        assert!(!visible_in_history);
        assert_eq!(screen.row_text(0), "line1");

        screen.set_scrollback(0);
        assert_eq!(screen.scrollback(), 0);
        assert!(screen.cursor().2);
    }

    #[test]
    fn event_pty_write_and_title_do_not_panic_or_leak() {
        let mut screen = AlacrittyScreen::new(20, 4, 100);
        // DSR (CSI 6 n -> Event::PtyWrite), DA1 (CSI c -> Event::PtyWrite), OSC 0 title, BEL
        screen.process(b"\x1b[6n\x1b[c\x1b]0;my-title\x07\x07hello");
        assert_eq!(screen.row_text(0), "hello");
    }
}
