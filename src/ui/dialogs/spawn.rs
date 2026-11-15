use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::ui::dialogs::DialogOutcome;
use crate::ui::theme::Theme;

pub fn default_shell_from_env(shell_env: Option<&str>) -> String {
    let raw = shell_env.map(str::trim).unwrap_or("");
    if raw.is_empty() {
        return "sh".to_string();
    }
    Path::new(raw)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(raw)
        .to_string()
}

pub fn default_shell() -> String {
    let val = std::env::var("SHELL").ok();
    default_shell_from_env(val.as_deref())
}

#[derive(Debug, Clone)]
pub struct SpawnDialog {
    pub command_or_template: String,
    pub task: String,
    pub space: String,
    pub templates: Vec<String>,
    pub active_field: usize,
}

impl SpawnDialog {
    pub fn new(space: String, templates: Vec<String>) -> Self {
        let default_cmd = templates.first().cloned().unwrap_or_else(default_shell);
        let prefill_space = if space == crate::model::tree::UNASSIGNED_SPACE {
            String::new()
        } else {
            space
        };
        Self {
            command_or_template: default_cmd,
            task: String::new(),
            space: prefill_space,
            templates,
            active_field: 0,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> DialogOutcome {
        match key.code {
            KeyCode::Esc => DialogOutcome::Close,
            KeyCode::Tab => {
                self.active_field = (self.active_field + 1) % 3;
                DialogOutcome::None
            }
            KeyCode::Enter => DialogOutcome::SubmitSpawn {
                command_or_template: self.command_or_template.trim().to_string(),
                task: self.task.trim().to_string(),
                space: self.space.trim().to_string(),
            },
            KeyCode::Backspace => {
                self.current_buf().pop();
                DialogOutcome::None
            }
            KeyCode::Char(c) => {
                self.current_buf().push(c);
                DialogOutcome::None
            }
            _ => DialogOutcome::None,
        }
    }

    fn current_buf(&mut self) -> &mut String {
        match self.active_field {
            0 => &mut self.command_or_template,
            1 => &mut self.task,
            _ => &mut self.space,
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Spawn Agent / Shell ")
            .borders(Borders::ALL)
            .border_style(Theme::modal_border());
        let inner = block.inner(area);
        block.render(area, buf);
        let tpl_hint = if self.templates.is_empty() {
            "none".to_string()
        } else {
            self.templates.join(", ")
        };
        let text = format!(
            "Templates: {tpl_hint}\n\n{} Command/Template: {}\n{} Task:             {}\n{} Space:            {}\n\n[Tab] Next field   [Enter] Spawn   [Esc] Cancel",
            if self.active_field == 0 { ">" } else { " " },
            self.command_or_template,
            if self.active_field == 1 { ">" } else { " " },
            self.task,
            if self.active_field == 2 { ">" } else { " " },
            self.space,
        );
        Paragraph::new(text).render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shell_detection_from_env() {
        assert_eq!(default_shell_from_env(Some("/usr/bin/zsh")), "zsh");
        assert_eq!(default_shell_from_env(Some("/bin/sh")), "sh");
        assert_eq!(default_shell_from_env(Some("bash")), "bash");
        assert_eq!(default_shell_from_env(Some("")), "sh");
        assert_eq!(default_shell_from_env(None), "sh");
    }
}
