use crate::layout::tree::PaneId;
use crate::term::screen::ScreenModel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelection {
    pub pane_id: PaneId,
    pub start: (u16, u16),
    pub end: (u16, u16),
}

impl TextSelection {
    /// Returns `((start_col, start_row), (end_col, end_row))` ordered by `(row, col)`.
    pub fn normalized(&self) -> ((u16, u16), (u16, u16)) {
        let s = (self.start.1, self.start.0);
        let e = (self.end.1, self.end.0);
        if s <= e {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    pub fn contains(&self, col: u16, row: u16) -> bool {
        let ((sc, sr), (ec, er)) = self.normalized();
        if row < sr || row > er {
            return false;
        }
        if sr == er {
            col >= sc && col <= ec
        } else if row == sr {
            col >= sc
        } else if row == er {
            col <= ec
        } else {
            true
        }
    }
}

pub fn extract_selected_text(
    screen: &dyn ScreenModel,
    sel: &TextSelection,
    inner_w: u16,
    inner_h: u16,
) -> String {
    let ((sc, sr), (ec, er)) = sel.normalized();
    let (sw, sh) = screen.size();
    let max_w = inner_w.min(sw);
    let max_h = inner_h.min(sh);
    if max_w == 0 || max_h == 0 || sr >= max_h {
        return String::new();
    }

    let mut lines = Vec::new();
    let last_row = er.min(max_h - 1);
    for row in sr..=last_row {
        let col_start = if row == sr { sc } else { 0 };
        let col_end = if row == er {
            ec.min(max_w - 1)
        } else {
            max_w - 1
        };
        if col_start > col_end || col_start >= max_w {
            lines.push(String::new());
            continue;
        }
        let mut line = String::new();
        for col in col_start..=col_end {
            if let Some(cell) = screen.cell(row, col) {
                if cell.wide_continuation {
                    continue;
                }
                if cell.symbol.is_empty() {
                    line.push(' ');
                } else {
                    line.push_str(&cell.symbol);
                }
            }
        }
        lines.push(line.trim_end_matches(' ').to_string());
    }
    lines.join("\n")
}
