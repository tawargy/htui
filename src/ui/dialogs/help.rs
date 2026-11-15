use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::ui::dialogs::DialogOutcome;
use crate::ui::keymap::PrefixRouter;
use crate::ui::theme::Theme;

#[derive(Debug, Clone, Default)]
pub struct HelpDialog;

impl HelpDialog {
    pub fn handle_key(&mut self, key: KeyEvent) -> DialogOutcome {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') | KeyCode::Enter => {
                DialogOutcome::Close
            }
            _ => DialogOutcome::None,
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Keybindings Help (Esc to close) ")
            .borders(Borders::ALL)
            .border_style(Theme::modal_border());
        let inner = block.inner(area);
        block.render(area, buf);

        let lines: Vec<Line> = PrefixRouter::bindings_help()
            .into_iter()
            .map(|(k, desc)| Line::from(format!("{k:<16}  {desc}")))
            .collect();
        Paragraph::new(lines).render(inner, buf);
    }
}
