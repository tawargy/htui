use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::ui::dialogs::DialogOutcome;
use crate::ui::theme::Theme;

#[derive(Debug, Clone)]
pub struct RenameSpaceDialog {
    pub old_space: String,
    pub new_space: String,
}

impl RenameSpaceDialog {
    pub fn new(old_space: String) -> Self {
        Self {
            new_space: old_space.clone(),
            old_space,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> DialogOutcome {
        match key.code {
            KeyCode::Esc => DialogOutcome::Close,
            KeyCode::Enter => {
                let trimmed = self.new_space.trim().to_string();
                if trimmed.is_empty() {
                    DialogOutcome::Close
                } else {
                    DialogOutcome::SubmitRename {
                        old_space: self.old_space.clone(),
                        new_space: trimmed,
                    }
                }
            }
            KeyCode::Backspace => {
                self.new_space.pop();
                DialogOutcome::None
            }
            KeyCode::Char(c) => {
                self.new_space.push(c);
                DialogOutcome::None
            }
            _ => DialogOutcome::None,
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Rename Space ")
            .borders(Borders::ALL)
            .border_style(Theme::modal_border());
        let inner = block.inner(area);
        block.render(area, buf);
        let text = format!(
            "Old: {}\nNew: {}_\n\n[Enter] Rename   [Esc] Cancel",
            self.old_space, self.new_space
        );
        Paragraph::new(text).render(inner, buf);
    }
}
