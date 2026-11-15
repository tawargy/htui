use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::ui::dialogs::DialogOutcome;
use crate::ui::theme::Theme;

#[derive(Debug, Clone, Default)]
pub struct NewSpaceDialog {
    pub name: String,
}

impl NewSpaceDialog {
    pub fn handle_key(&mut self, key: KeyEvent) -> DialogOutcome {
        match key.code {
            KeyCode::Esc => DialogOutcome::Close,
            KeyCode::Enter => {
                let trimmed = self.name.trim().to_string();
                if trimmed.is_empty() {
                    DialogOutcome::Close
                } else {
                    DialogOutcome::SubmitNewSpace(trimmed)
                }
            }
            KeyCode::Backspace => {
                self.name.pop();
                DialogOutcome::None
            }
            KeyCode::Char(c) => {
                self.name.push(c);
                DialogOutcome::None
            }
            _ => DialogOutcome::None,
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Create New Space ")
            .borders(Borders::ALL)
            .border_style(Theme::modal_border());
        let inner = block.inner(area);
        block.render(area, buf);
        let text = format!(
            "Space name: {}_\n\n[Enter] Create   [Esc] Cancel",
            self.name
        );
        Paragraph::new(text).render(inner, buf);
    }
}
