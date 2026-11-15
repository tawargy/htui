use std::collections::BTreeMap;

use crate::hc::types::{StatusKind, Terminal};

pub const UNASSIGNED_SPACE: &str = "(unassigned)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalEntry {
    pub terminal: Terminal,
}

impl TerminalEntry {
    pub fn display_name(&self) -> String {
        if let Some(name) = self.terminal.name.as_deref().filter(|s| !s.is_empty()) {
            return name.to_string();
        }
        if let Some(app) = self
            .terminal
            .status
            .as_ref()
            .map(|s| s.app.as_str())
            .filter(|s| !s.is_empty())
        {
            return app.to_string();
        }
        if let Some(fg) = self
            .terminal
            .foreground
            .as_ref()
            .map(|f| f.name.as_str())
            .filter(|s| !s.is_empty())
        {
            return fg.to_string();
        }
        if !self.terminal.title.is_empty() {
            return self.terminal.title.clone();
        }
        if let Some(tpl) = self.terminal.template.as_deref().filter(|s| !s.is_empty()) {
            return tpl.to_string();
        }
        if let Some(cmd) = self.terminal.argv.first().filter(|s| !s.is_empty()) {
            return cmd.clone();
        }
        self.terminal.address.clone()
    }

    pub fn space_name(&self) -> &str {
        self.terminal
            .labels
            .get("space")
            .map(String::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(UNASSIGNED_SPACE)
    }

    pub fn status_glyph(&self) -> &'static str {
        if !self.terminal.is_live() {
            return "○";
        }
        match self.terminal.status.as_ref().map(|s| s.kind_enum()) {
            Some(StatusKind::Working) => "◐",
            Some(StatusKind::Blocked) => "⚠",
            Some(StatusKind::Error) => "✖",
            Some(StatusKind::Done) => "✔",
            Some(StatusKind::Idle) => "●",
            Some(StatusKind::Other(_)) => "◈",
            None => "·",
        }
    }

    pub fn status_label(&self) -> &str {
        if !self.terminal.is_live() {
            return "finished";
        }
        self.terminal
            .status
            .as_ref()
            .map(|s| s.state.as_str())
            .unwrap_or("live")
    }

    pub fn is_agent(&self) -> bool {
        if self
            .terminal
            .template
            .as_deref()
            .is_some_and(|s| !s.is_empty())
            || self.terminal.reported
            || self.terminal.status.is_some()
        {
            return true;
        }
        const KNOWN_AGENT_BINS: &[&str] = &[
            "agy", "claude", "codex", "opencode", "gemini", "aider", "goose", "cursor", "pi",
        ];
        let matches_agent = |bin: &str| {
            let base = bin.rsplit('/').next().unwrap_or(bin).to_ascii_lowercase();
            KNOWN_AGENT_BINS.contains(&base.as_str())
        };
        if self
            .terminal
            .foreground
            .as_ref()
            .is_some_and(|f| matches_agent(&f.name))
        {
            return true;
        }
        self.terminal.argv.first().is_some_and(|a| matches_agent(a))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceNode {
    pub name: String,
    pub terminals: Vec<TerminalEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineNode {
    pub name: String,
    pub state: String,
    pub is_local: bool,
    pub spaces: BTreeMap<String, SpaceNode>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unassigned_space_and_status_glyphs() {
        let t = Terminal {
            address: "t1".into(),
            state: "live".into(),
            argv: vec!["claude".into()],
            ..Default::default()
        };
        let entry = TerminalEntry { terminal: t };
        assert_eq!(entry.space_name(), UNASSIGNED_SPACE);
        assert_eq!(entry.display_name(), "claude");
        assert_eq!(entry.status_glyph(), "·");
    }

    #[test]
    fn is_agent_filters_plain_shells_and_detects_agents() {
        use crate::hc::types::Foreground;

        let zsh = TerminalEntry {
            terminal: Terminal {
                address: "t1".into(),
                state: "live".into(),
                argv: vec!["zsh".into()],
                foreground: Some(Foreground {
                    pid: 100,
                    name: "zsh".into(),
                    argv: vec!["zsh".into()],
                    cwd: "/tmp".into(),
                }),
                ..Default::default()
            },
        };
        assert!(!zsh.is_agent());

        let nvim = TerminalEntry {
            terminal: Terminal {
                address: "t2".into(),
                state: "live".into(),
                argv: vec!["zsh".into()],
                foreground: Some(Foreground {
                    pid: 101,
                    name: "nvim".into(),
                    argv: vec!["nvim".into()],
                    cwd: "/tmp".into(),
                }),
                ..Default::default()
            },
        };
        assert!(!nvim.is_agent());

        let reported = TerminalEntry {
            terminal: Terminal {
                address: "t3".into(),
                state: "live".into(),
                argv: vec!["zsh".into()],
                reported: true,
                ..Default::default()
            },
        };
        assert!(reported.is_agent());

        let templated = TerminalEntry {
            terminal: Terminal {
                address: "t4".into(),
                state: "live".into(),
                template: Some("claude".into()),
                ..Default::default()
            },
        };
        assert!(templated.is_agent());

        let fg_agy = TerminalEntry {
            terminal: Terminal {
                address: "t5".into(),
                state: "live".into(),
                argv: vec!["zsh".into()],
                foreground: Some(Foreground {
                    pid: 102,
                    name: "agy".into(),
                    argv: vec!["agy".into()],
                    cwd: "/tmp".into(),
                }),
                ..Default::default()
            },
        };
        assert!(fg_agy.is_agent());

        let fg_opencode = TerminalEntry {
            terminal: Terminal {
                address: "t6".into(),
                state: "live".into(),
                argv: vec!["zsh".into()],
                foreground: Some(Foreground {
                    pid: 103,
                    name: "opencode".into(),
                    argv: vec!["opencode".into()],
                    cwd: "/tmp".into(),
                }),
                ..Default::default()
            },
        };
        assert!(fg_opencode.is_agent());
    }
}
