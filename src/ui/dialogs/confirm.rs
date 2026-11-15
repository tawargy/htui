use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::ui::dialogs::DialogOutcome;
use crate::ui::theme::Theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    KillTerminal(String),
    KillBeforeRemove(String),
    RemoveTerminal(String),
}

#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    pub title: String,
    pub message: String,
    pub action: ConfirmAction,
}

impl ConfirmDialog {
    pub fn kill(terminal: String) -> Self {
        Self {
            title: " Confirm Kill ".to_string(),
            message: format!("Kill live terminal '{terminal}'? [y/N]"),
            action: ConfirmAction::KillTerminal(terminal),
        }
    }

    pub fn kill_before_remove(terminal: String) -> Self {
        Self {
            title: " Terminal Still Live ".to_string(),
            message: format!("Terminal '{terminal}' is still live. Kill it first? [y/N]"),
            action: ConfirmAction::KillBeforeRemove(terminal),
        }
    }

    pub fn remove(terminal: String) -> Self {
        Self {
            title: " Confirm Remove ".to_string(),
            message: format!("Remove finished terminal '{terminal}'? [y/N]"),
            action: ConfirmAction::RemoveTerminal(terminal),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> DialogOutcome {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                DialogOutcome::SubmitConfirm(self.action.clone())
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => DialogOutcome::Close,
            _ => DialogOutcome::None,
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(self.title.as_str())
            .borders(Borders::ALL)
            .border_style(Theme::modal_border());
        let inner = block.inner(area);
        block.render(area, buf);
        Paragraph::new(self.message.as_str()).render(inner, buf);
    }
}
