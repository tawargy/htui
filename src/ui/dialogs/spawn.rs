use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::layout::panel::PanelId;
use crate::layout::tree::{PaneId, SplitAxis};
use crate::ui::dialogs::DialogOutcome;
use crate::ui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpawnTarget {
    #[default]
    NewPanel,
    IntoPane {
        panel_id: PanelId,
        pane_id: PaneId,
    },
    SplitPane {
        panel_id: PanelId,
        pane_id: PaneId,
        axis: SplitAxis,
    },
}

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
    pub target: SpawnTarget,
}

impl SpawnDialog {
    pub fn new(space: String, templates: Vec<String>) -> Self {
        Self::with_target(space, templates, SpawnTarget::NewPanel)
    }

    pub fn with_target(space: String, templates: Vec<String>, target: SpawnTarget) -> Self {
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
            target,
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
                target: self.target,
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
        let target_label = match self.target {
            SpawnTarget::NewPanel => "New panel",
            SpawnTarget::IntoPane { .. } => "Selected pane",
            SpawnTarget::SplitPane {
                axis: SplitAxis::Horizontal,
                ..
            } => "Horizontal split",
            SpawnTarget::SplitPane {
                axis: SplitAxis::Vertical,
                ..
            } => "Vertical split",
        };
        let text = format!(
            "Target:    {target_label}\nTemplates: {tpl_hint}\n\n{} Command/Template: {}\n{} Task:             {}\n{} Space:            {}\n\n[Tab] Next field   [Enter] Spawn   [Esc] Cancel",
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

    #[test]
    fn spawn_target_defaults_and_with_target_propagates_to_outcome_and_render() {
        use crossterm::event::KeyModifiers;

        let mut d_new = SpawnDialog::new("api".into(), vec!["claude".into()]);
        assert_eq!(d_new.target, SpawnTarget::NewPanel);
        let area = Rect::new(0, 0, 60, 12);
        let mut buf = Buffer::empty(area);
        d_new.render(area, &mut buf);
        let text_new: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(text_new.contains("New panel"));

        let outcome_new = d_new.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            outcome_new,
            DialogOutcome::SubmitSpawn {
                command_or_template: "claude".into(),
                task: String::new(),
                space: "api".into(),
                target: SpawnTarget::NewPanel,
            }
        );

        let split_target = SpawnTarget::SplitPane {
            panel_id: 2,
            pane_id: 3,
            axis: SplitAxis::Horizontal,
        };
        let mut d_split = SpawnDialog::with_target("api".into(), vec!["zsh".into()], split_target);
        assert_eq!(d_split.target, split_target);
        let mut buf_h = Buffer::empty(area);
        d_split.render(area, &mut buf_h);
        let text_h: String = buf_h.content().iter().map(|c| c.symbol()).collect();
        assert!(text_h.contains("Horizontal split"));

        let outcome_split = d_split.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            outcome_split,
            DialogOutcome::SubmitSpawn {
                command_or_template: "zsh".into(),
                task: String::new(),
                space: "api".into(),
                target: split_target,
            }
        );

        let v_target = SpawnTarget::SplitPane {
            panel_id: 2,
            pane_id: 3,
            axis: SplitAxis::Vertical,
        };
        let d_v = SpawnDialog::with_target("api".into(), vec!["zsh".into()], v_target);
        let mut buf_v = Buffer::empty(area);
        d_v.render(area, &mut buf_v);
        let text_v: String = buf_v.content().iter().map(|c| c.symbol()).collect();
        assert!(text_v.contains("Vertical split"));
    }
}
