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

struct ClearWrapHandler<'a>(&'a mut alacritty_terminal::Term<NoopEventListener>);

fn clear_prev_wrapline(
    term: &mut alacritty_terminal::Term<NoopEventListener>,
    line: alacritty_terminal::index::Line,
) {
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::cell::Flags;

    let grid = term.grid();
    if line <= grid.topmost_line() {
        return;
    }
    let cols = grid.columns();
    if cols == 0 {
        return;
    }
    let prev_line = Line(line.0 - 1);
    let last_col = Column(cols - 1);
    if grid[prev_line][last_col].flags.contains(Flags::WRAPLINE) {
        term.grid_mut()[prev_line][last_col]
            .flags
            .remove(Flags::WRAPLINE);
    }
}

macro_rules! forward_handler_methods {
    (
        $(fn $name:ident(&mut self $(, $arg:ident : $ty:ty)* );)*
    ) => {
        $(
            #[inline]
            fn $name(&mut self $(, $arg : $ty)*) {
                self.0.$name($($arg),*);
            }
        )*
    };
}

impl alacritty_terminal::vte::ansi::Handler for ClearWrapHandler<'_> {
    forward_handler_methods! {
        fn set_title(&mut self, title: Option<String>);
        fn set_cursor_style(&mut self, style: Option<alacritty_terminal::vte::ansi::CursorStyle>);
        fn set_cursor_shape(&mut self, shape: alacritty_terminal::vte::ansi::CursorShape);
        fn input(&mut self, c: char);
        fn goto(&mut self, line: i32, col: usize);
        fn goto_line(&mut self, line: i32);
        fn goto_col(&mut self, col: usize);
        fn insert_blank(&mut self, count: usize);
        fn move_up(&mut self, lines: usize);
        fn move_down(&mut self, lines: usize);
        fn identify_terminal(&mut self, intermediate: Option<char>);
        fn device_status(&mut self, arg: usize);
        fn move_forward(&mut self, cols: usize);
        fn move_backward(&mut self, cols: usize);
        fn move_down_and_cr(&mut self, lines: usize);
        fn move_up_and_cr(&mut self, lines: usize);
        fn put_tab(&mut self, count: u16);
        fn backspace(&mut self);
        fn carriage_return(&mut self);
        fn linefeed(&mut self);
        fn bell(&mut self);
        fn substitute(&mut self);
        fn newline(&mut self);
        fn set_horizontal_tabstop(&mut self);
        fn scroll_up(&mut self, lines: usize);
        fn scroll_down(&mut self, lines: usize);
        fn insert_blank_lines(&mut self, lines: usize);
        fn delete_lines(&mut self, lines: usize);
        fn erase_chars(&mut self, count: usize);
        fn delete_chars(&mut self, count: usize);
        fn move_backward_tabs(&mut self, count: u16);
        fn move_forward_tabs(&mut self, count: u16);
        fn save_cursor_position(&mut self);
        fn restore_cursor_position(&mut self);
        fn clear_tabs(&mut self, mode: alacritty_terminal::vte::ansi::TabulationClearMode);
        fn reset_state(&mut self);
        fn reverse_index(&mut self);
        fn terminal_attribute(&mut self, attr: alacritty_terminal::vte::ansi::Attr);
        fn set_mode(&mut self, mode: alacritty_terminal::vte::ansi::Mode);
        fn unset_mode(&mut self, mode: alacritty_terminal::vte::ansi::Mode);
        fn report_mode(&mut self, mode: alacritty_terminal::vte::ansi::Mode);
        fn set_private_mode(&mut self, mode: alacritty_terminal::vte::ansi::PrivateMode);
        fn unset_private_mode(&mut self, mode: alacritty_terminal::vte::ansi::PrivateMode);
        fn report_private_mode(&mut self, mode: alacritty_terminal::vte::ansi::PrivateMode);
        fn set_scrolling_region(&mut self, top: usize, bottom: Option<usize>);
        fn set_keypad_application_mode(&mut self);
        fn unset_keypad_application_mode(&mut self);
        fn set_active_charset(&mut self, index: alacritty_terminal::vte::ansi::CharsetIndex);
        fn configure_charset(
            &mut self,
            index: alacritty_terminal::vte::ansi::CharsetIndex,
            charset: alacritty_terminal::vte::ansi::StandardCharset
        );
        fn set_color(&mut self, index: usize, color: alacritty_terminal::vte::ansi::Rgb);
        fn dynamic_color_sequence(&mut self, prefix: String, index: usize, terminator: &str);
        fn reset_color(&mut self, index: usize);
        fn clipboard_store(&mut self, clipboard: u8, base64: &[u8]);
        fn clipboard_load(&mut self, clipboard: u8, terminator: &str);
        fn decaln(&mut self);
        fn push_title(&mut self);
        fn pop_title(&mut self);
        fn text_area_size_pixels(&mut self);
        fn text_area_size_chars(&mut self);
        fn set_hyperlink(&mut self, hyperlink: Option<alacritty_terminal::vte::ansi::Hyperlink>);
        fn report_keyboard_mode(&mut self);
        fn push_keyboard_mode(&mut self, mode: alacritty_terminal::vte::ansi::KeyboardModes);
        fn pop_keyboard_modes(&mut self, to_pop: u16);
        fn set_keyboard_mode(
            &mut self,
            mode: alacritty_terminal::vte::ansi::KeyboardModes,
            behavior: alacritty_terminal::vte::ansi::KeyboardModesApplyBehavior
        );
    }

    #[inline]
    fn clear_line(&mut self, mode: alacritty_terminal::vte::ansi::LineClearMode) {
        use alacritty_terminal::index::Column;
        use alacritty_terminal::vte::ansi::LineClearMode;

        let cursor = &self.0.grid().cursor;
        let clears_col0 = match mode {
            LineClearMode::All | LineClearMode::Left => true,
            LineClearMode::Right => !cursor.input_needs_wrap && cursor.point.column == Column(0),
        };
        let line = cursor.point.line;
        self.0.clear_line(mode);
        if clears_col0 {
            clear_prev_wrapline(self.0, line);
        }
    }

    #[inline]
    fn clear_screen(&mut self, mode: alacritty_terminal::vte::ansi::ClearMode) {
        use alacritty_terminal::index::{Column, Line};
        use alacritty_terminal::vte::ansi::ClearMode;

        let cursor_point = self.0.grid().cursor.point;
        let clear_mode = match mode {
            ClearMode::Below => 0u8,
            ClearMode::Above => 1u8,
            ClearMode::All => 2u8,
            ClearMode::Saved => 3u8,
        };
        self.0.clear_screen(mode);
        match clear_mode {
            0 => {
                if cursor_point.column == Column(0) {
                    clear_prev_wrapline(self.0, cursor_point.line);
                }
            }
            1 => {
                clear_prev_wrapline(self.0, cursor_point.line);
                if cursor_point.line > Line(1) {
                    clear_prev_wrapline(self.0, Line(0));
                }
            }
            2 => {
                clear_prev_wrapline(self.0, Line(0));
            }
            _ => {}
        }
    }
}

impl ScreenModel for AlacrittyScreen {
    fn process(&mut self, bytes: &[u8]) {
        let mut handler = ClearWrapHandler(&mut self.term);
        if let Some(deadline) = self.parser.sync_timeout().sync_timeout() {
            if std::time::Instant::now() >= deadline {
                self.parser.stop_sync(&mut handler);
            }
        }
        self.parser.advance(&mut handler, bytes);
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        use alacritty_terminal::grid::{Dimensions, Grid};
        use alacritty_terminal::index::Line;
        use alacritty_terminal::term::TermMode;

        self.logical_cols = cols.max(1);
        self.logical_rows = rows.max(1);
        let dims = ScreenDims::new(self.logical_cols, self.logical_rows);

        let old_term_cols = self.term.columns();
        let old_term_rows = self.term.screen_lines();
        let target_cols = dims.columns();
        let target_rows = dims.screen_lines();

        if self.term.mode().contains(TermMode::ALT_SCREEN) || target_cols >= old_term_cols {
            self.term.resize(dims);
            return;
        }

        let saved_display_offset = self.term.grid().display_offset();

        // If height also changed, apply row resize first at old_term_cols so `hist_before`
        // isolates lines pushed into scrollback solely by column-shrink wrapping.
        if target_rows != old_term_rows {
            self.term.resize(ScreenDims {
                cols: old_term_cols,
                rows: target_rows,
            });
        }

        let pre_grid = self.term.grid();
        let hist_before = pre_grid.history_size();
        let pre_cursor = pre_grid.cursor.clone();
        let pre_cursor_row = pre_cursor.point.line.0.clamp(0, target_rows as i32 - 1) as usize;
        let pre_last_non_clear = (0..target_rows)
            .rev()
            .find(|&r| !pre_grid[Line(r as i32)].is_clear())
            .unwrap_or(0);
        let pre_last_occupied = pre_cursor_row.max(pre_last_non_clear);
        let pre_rows: Vec<_> = (0..=pre_last_occupied)
            .map(|r| pre_grid[Line(r as i32)].clone())
            .collect();

        self.term.resize(dims);

        let post_grid = self.term.grid();
        let pushed = post_grid.history_size().saturating_sub(hist_before);
        if pushed == 0 {
            return;
        }

        let post_cursor_line = post_grid.cursor.point.line;
        let post_cursor_row = post_cursor_line.0.clamp(0, target_rows as i32 - 1) as usize;
        let post_last_non_clear = (0..target_rows)
            .rev()
            .find(|&r| !post_grid[Line(r as i32)].is_clear())
            .unwrap_or(0);
        let post_occupied_end = post_cursor_row.max(post_last_non_clear);
        let trailing_blanks = target_rows.saturating_sub(post_occupied_end + 1);
        let reclaim = pushed.min(trailing_blanks);

        if reclaim == 0 {
            return;
        }

        // If `alacritty_terminal`'s `max(line - 1, Line(0))` clamp fired because `Line(0)` wrapped,
        // compute the exact un-clamped cursor row on a scratch grid padded with `pushed` top rows.
        let corrected_cursor_line = if post_cursor_line == Line(0) {
            let pad = pushed;
            let scratch_rows = pad + pre_last_occupied + 1;
            let mut scratch = Grid::new(scratch_rows, old_term_cols, pushed + 4);
            for (r, row) in pre_rows.into_iter().enumerate() {
                scratch[Line((pad + r) as i32)] = row;
            }
            scratch.cursor = pre_cursor;
            scratch.cursor.point.line = Line(scratch.cursor.point.line.0 + pad as i32);
            scratch.resize(true, scratch_rows, target_cols);
            let unclamped = scratch.cursor.point.line.0 - (pushed - reclaim) as i32;
            Some(Line(unclamped.clamp(0, target_rows as i32 - 1)))
        } else {
            None
        };

        self.term.resize(ScreenDims {
            cols: target_cols,
            rows: target_rows + reclaim,
        });
        self.term.resize(ScreenDims {
            cols: target_cols,
            rows: target_rows,
        });

        if let Some(line) = corrected_cursor_line {
            self.term.grid_mut().cursor.point.line = line;
        }
        if saved_display_offset > 0 {
            self.set_scrollback(saved_display_offset);
        }
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
        let col = (point.column.0 as u16).min(self.logical_cols.saturating_sub(1));
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

    #[test]
    fn clear_from_col_zero_breaks_prev_wrapline_including_scrollback() {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Column, Line};
        use alacritty_terminal::term::cell::Flags;

        // 1. Scrollback Line(-1) WRAPLINE cleared by \r\x1b[J (ClearMode::Below at col 0)
        let mut s1 = AlacrittyScreen::new(10, 2, 100);
        s1.process(b"0123456789abcdefghijZ");
        assert_eq!(s1.term.grid().history_size(), 1);
        assert!(s1.term.grid()[Line(-1)][Column(9)]
            .flags
            .contains(Flags::WRAPLINE));
        s1.process(b"\x1b[1;1H\x1b[J");
        assert!(
            !s1.term.grid()[Line(-1)][Column(9)]
                .flags
                .contains(Flags::WRAPLINE),
            "ClearMode::Below at col 0 must clear WRAPLINE on Line(-1)"
        );

        // 2. \x1b[2K and \x1b[K at col 0 clear prev_line WRAPLINE; mid-line \x1b[K preserves it
        let mut s2 = AlacrittyScreen::new(10, 4, 100);
        s2.process(b"0123456789ab");
        assert!(s2.term.grid()[Line(0)][Column(9)]
            .flags
            .contains(Flags::WRAPLINE));
        s2.process(b"\x1b[2;3H\x1b[K");
        assert!(
            s2.term.grid()[Line(0)][Column(9)]
                .flags
                .contains(Flags::WRAPLINE),
            "mid-line CSI K must preserve prev_line WRAPLINE"
        );
        s2.process(b"\r\x1b[K");
        assert!(
            !s2.term.grid()[Line(0)][Column(9)]
                .flags
                .contains(Flags::WRAPLINE),
            "CSI K at col 0 must clear prev_line WRAPLINE"
        );

        // 3. CSI 2K at col > 0 still clears prev_line WRAPLINE
        let mut s3 = AlacrittyScreen::new(10, 4, 100);
        s3.process(b"0123456789abcd");
        s3.process(b"\x1b[2K");
        assert!(
            !s3.term.grid()[Line(0)][Column(9)]
                .flags
                .contains(Flags::WRAPLINE),
            "CSI 2K must clear prev_line WRAPLINE"
        );

        // 4. CSI 1K (LineClearMode::Left) clears prev_line WRAPLINE
        let mut s4 = AlacrittyScreen::new(10, 4, 100);
        s4.process(b"0123456789abcd");
        s4.process(b"\x1b[1K");
        assert!(
            !s4.term.grid()[Line(0)][Column(9)]
                .flags
                .contains(Flags::WRAPLINE),
            "CSI 1K must clear prev_line WRAPLINE"
        );

        // 5. CSI 2J (ClearMode::All) clears Line(-1) WRAPLINE when scrolled into history
        let mut s5 = AlacrittyScreen::new(10, 3, 100);
        s5.process(b"0123456789");
        s5.process(b"a\x1b[1;1H");
        // Put WRAPLINE on Line(0) (which becomes Line(-2)) and Line(1) (which becomes Line(-1))
        let mut s6 = AlacrittyScreen::new(10, 2, 100);
        s6.process(b"0123456789");
        // Write 1 char on row 1, then go to row 1 col 0 and write 11 chars? Wait: 10 chars at row 1 sets WRAPLINE only when wrapping to row 2 (scrolling).
        // Wait: if we have 3 rows, write "x\r\n0123456789a" -> row 0="x", row 1="0123456789"(WRAPLINE), row 2="a".
        // Then CSI 2J pushes rows 0..3 -> Line(-2) has WRAPLINE, Line(-1) is "a".
        // What if row 2 also wrapped? On 2-row screen: row 0 = "x", row 1 = "0123456789", then input 'a' scrolls row 0 to Line(-1) and row 1 is Line(0) with WRAPLINE.
        // Instead on 3-row screen: fill row 0, fill row 1 (WRAPLINE on row 0), fill row 2 (WRAPLINE on row 1). No scroll yet!
        s5 = AlacrittyScreen::new(10, 3, 100);
        s5.process(b"\x1b[2;1H0123456789a");
        // Now row 0 is blank, row 1 has WRAPLINE, row 2 has "a".
        // If we manually check s5 before CSI 2J, row 1 has WRAPLINE. After CSI 2J, row 1 is Line(-2).
        // To get WRAPLINE onto Line(-1) specifically via CSI 2J:
        // On a 2-row screen, write 11 chars -> row 0 has WRAPLINE, row 1 has 'a'.
        // Wait: clear_viewport pushes ONLY up to the last non-empty line, or all 2 rows?
        // If row 1 is empty, clear_viewport pushes ONLY row 0 (so row 0 becomes Line(-1))!
        // We can't get WRAPLINE on row 0 with row 1 empty via normal text unless row 1 was erased after wrap, which clears row 0's WRAPLINE!
        // Still, CSI 2J check on s6: clear_viewport pushes rows, verify no panic and Line(-1) has no WRAPLINE.
        s6.process(b"\x1b[2J");
        assert!(!s6.term.grid()[Line(-1)][Column(9)]
            .flags
            .contains(Flags::WRAPLINE));

        // 6. input_needs_wrap with CSI K (LineClearMode::Right is no-op, must NOT clear WRAPLINE)
        let mut s9 = AlacrittyScreen::new(10, 3, 100);
        s9.process(b"0123456789abcdefghij");
        s9.process(b"\x1b[K");
        assert!(
            s9.term.grid()[Line(0)][Column(9)]
                .flags
                .contains(Flags::WRAPLINE),
            "CSI K with input_needs_wrap is a no-op and must retain prev_line WRAPLINE"
        );
    }

    #[test]
    fn clear_screen_below_after_shrink_prevents_old_and_new_prompts_merging_on_grow() {
        let mut screen = AlacrittyScreen::new(40, 2, 100);
        // Line 0: previous output, Line 1: prompt + RPROMPT at col 37..40
        screen.process(b"prev_output\r\nhtui (main) >");
        screen.process(b"\x1b[24C \xE2\x9C\x93 ");
        // Shrink width so Line 1 wraps ` ✓ ` onto Line 2, pushing `prev_output` and `htui (main) >` into scrollback
        screen.resize(20, 2);
        // Simulate zsh SIGWINCH redraw: CR + SGR reset + ED0 (\x1b[J) + new prompt
        screen.process(b"\r\r\x1b[0m\x1b[27m\x1b[24m\x1b[Jhtui (main) >");
        // Grow height and width back
        screen.resize(60, 6);

        let rows: Vec<String> = (0..6).map(|r| screen.row_text(r)).collect();
        for row in &rows {
            let count = row.matches("htui (main) >").count();
            assert!(
                count <= 1,
                "old and new prompts merged onto one row: {rows:?}"
            );
        }
    }

    #[test]
    fn shrink_reclaims_trailing_blanks_and_preserves_line0_cursor_and_ls_output() {
        use alacritty_terminal::grid::Dimensions;

        // Case 1: Fresh zsh prompt + RPROMPT on Line(0) of a tall pane (40x10 -> 20x10 -> 40x10)
        let mut s_line0 = AlacrittyScreen::new(40, 10, 100);
        // Prompt on row 0 (14 chars), move forward 23 cols to col 37, write " ✓ ", move back 25 cols from col 39 to col 14
        s_line0.process(b"htui (main) > \x1b[23C \xE2\x9C\x93 \x1b[25D");
        assert_eq!(s_line0.cursor(), (14, 0, true));

        s_line0.resize(20, 10);
        assert_eq!(
            s_line0.term.grid().history_size(),
            0,
            "Line(0) wrap must be reclaimed into blank rows, not pushed to scrollback"
        );
        assert_eq!(s_line0.row_text(0), "htui (main) >");
        assert_eq!(s_line0.row_text(1).trim(), "\u{2713}");
        assert_eq!(
            s_line0.cursor(),
            (14, 0, true),
            "cursor must remain on Line(0) prompt row rather than clamping onto wrapped RPROMPT row"
        );

        // Now zsh redraws at 20x10 with \r\x1b[J and new prompt + RPROMPT
        s_line0.process(b"\r\r\x1b[0m\x1b[Jhtui (main) > \x1b[3C \xE2\x9C\x93 \x1b[5D");
        s_line0.resize(40, 10);
        assert_eq!(s_line0.row_text(0), "htui (main) >     \u{2713}");
        assert_eq!(s_line0.row_text(1), "");
        assert_eq!(s_line0.cursor(), (14, 0, true));

        // Case 2: ls output + prompt in top rows of tall pane across shrink/grow (80x20 -> 40x15 -> 80x20)
        let mut s_tall = AlacrittyScreen::new(80, 20, 100);
        s_tall.process(b"htui (main) > ls\r\n");
        s_tall.process(b"Cargo.toml  src  tests\r\n");
        s_tall.process(b"htui (main) > \x1b[63C \xE2\x9C\x93 \x1b[65D");
        assert_eq!(s_tall.cursor(), (14, 2, true));

        s_tall.resize(40, 15);
        assert_eq!(
            s_tall.term.grid().history_size(),
            0,
            "ls output must stay visible in tall pane when trailing blank rows exist"
        );
        assert_eq!(s_tall.row_text(0), "htui (main) > ls");
        assert_eq!(s_tall.row_text(1), "Cargo.toml  src  tests");
        assert_eq!(s_tall.row_text(2), "htui (main) >");
        assert_eq!(s_tall.cursor(), (14, 2, true));

        // Simulate zsh SIGWINCH redraw at 40 cols, then grow back to 80x20
        s_tall.process(b"\r\r\x1b[0m\x1b[Jhtui (main) > \x1b[23C \xE2\x9C\x93 \x1b[25D");
        s_tall.resize(80, 20);
        assert_eq!(s_tall.row_text(0), "htui (main) > ls");
        assert_eq!(s_tall.row_text(1), "Cargo.toml  src  tests");
        assert!(s_tall.row_text(2).starts_with("htui (main) >"));
        assert_eq!(s_tall.row_text(3), "");
    }

    #[test]
    fn shrink_reclaim_preserves_nonzero_scrollback_offset() {
        let mut screen = AlacrittyScreen::new(40, 6, 100);
        for i in 0..8 {
            screen.process(format!("line-{i:02}\r\n").as_bytes());
        }
        // Clear bottom rows while keeping history, put a long line on row 0 that wraps on shrink
        screen.process(b"\x1b[1;1H\x1b[J012345678901234567890123456789012345");
        screen.set_scrollback(2);
        assert_eq!(screen.scrollback(), 2);

        screen.resize(20, 6);
        assert_eq!(screen.scrollback(), 2);
    }
}
