use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::layout::tree::NavDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    SplitHorizontal,
    SplitVertical,
    SplitSpawnHorizontal,
    SplitSpawnVertical,
    PrevPanel,
    NextPanel,
    ClosePane,
    FocusDir(NavDir),
    TakeOver,
    OpenSpawnDialog,
    OpenNewSpace,
    ConfirmKill,
    ConfirmRemove,
    OpenRenameSpace,
    ToggleSidebar,
    ToggleFocusSidebar,
    OpenHelp,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteResult {
    ModalKey(KeyEvent),
    Command(Command),
    PrefixArmed,
    PrefixCancelled,
    ForwardToPane(KeyEvent),
    Ignored,
}

pub struct PrefixRouter {
    armed_at: Option<Instant>,
    timeout: Duration,
}

impl Default for PrefixRouter {
    fn default() -> Self {
        Self::new(Duration::from_millis(1500))
    }
}

impl PrefixRouter {
    pub fn new(timeout: Duration) -> Self {
        Self {
            armed_at: None,
            timeout,
        }
    }

    pub fn is_armed(&self, now: Instant) -> bool {
        self.armed_at
            .is_some_and(|t| now.duration_since(t) < self.timeout)
    }

    pub fn tick(&mut self, now: Instant) {
        if let Some(t) = self.armed_at {
            if now.duration_since(t) >= self.timeout {
                self.armed_at = None;
            }
        }
    }

    pub fn route(&mut self, event: KeyEvent, modal_active: bool, now: Instant) -> RouteResult {
        if event.kind == KeyEventKind::Release {
            return RouteResult::Ignored;
        }
        if modal_active {
            self.armed_at = None;
            return RouteResult::ModalKey(event);
        }

        self.tick(now);

        if self.armed_at.is_some() {
            self.armed_at = None;
            return match event.code {
                KeyCode::Char('%') | KeyCode::Char('|') => {
                    RouteResult::Command(Command::SplitHorizontal)
                }
                KeyCode::Char('"') | KeyCode::Char('-') => {
                    RouteResult::Command(Command::SplitVertical)
                }
                KeyCode::Char('H') => RouteResult::Command(Command::SplitSpawnHorizontal),
                KeyCode::Char('V') => RouteResult::Command(Command::SplitSpawnVertical),
                KeyCode::Char('[') => RouteResult::Command(Command::PrevPanel),
                KeyCode::Char(']') => RouteResult::Command(Command::NextPanel),
                KeyCode::Char('x') | KeyCode::Char('q') => RouteResult::Command(Command::ClosePane),
                KeyCode::Char('h') | KeyCode::Left => {
                    RouteResult::Command(Command::FocusDir(NavDir::Left))
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    RouteResult::Command(Command::FocusDir(NavDir::Down))
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    RouteResult::Command(Command::FocusDir(NavDir::Up))
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    RouteResult::Command(Command::FocusDir(NavDir::Right))
                }
                KeyCode::Char('t') => RouteResult::Command(Command::TakeOver),
                KeyCode::Char('n') => RouteResult::Command(Command::OpenSpawnDialog),
                KeyCode::Char('c') => RouteResult::Command(Command::OpenNewSpace),
                KeyCode::Char('K') => RouteResult::Command(Command::ConfirmKill),
                KeyCode::Char('R') => RouteResult::Command(Command::ConfirmRemove),
                KeyCode::Char('r') => RouteResult::Command(Command::OpenRenameSpace),
                KeyCode::Char('b') => RouteResult::Command(Command::ToggleSidebar),
                KeyCode::Tab => RouteResult::Command(Command::ToggleFocusSidebar),
                KeyCode::Char('?') => RouteResult::Command(Command::OpenHelp),
                KeyCode::Char('d') => RouteResult::Command(Command::Quit),
                _ => RouteResult::PrefixCancelled,
            };
        }

        if event.modifiers.contains(KeyModifiers::CONTROL) && event.code == KeyCode::Char('b') {
            self.armed_at = Some(now);
            return RouteResult::PrefixArmed;
        }

        RouteResult::ForwardToPane(event)
    }

    pub fn bindings_help() -> Vec<(&'static str, &'static str)> {
        vec![
            ("Ctrl+B % / |", "Split pane horizontally"),
            ("Ctrl+B \" / -", "Split pane vertically"),
            ("Ctrl+B H", "Split horizontally & spawn"),
            ("Ctrl+B V", "Split vertically & spawn"),
            ("Ctrl+B [ / ]", "Previous / next panel"),
            ("Ctrl+B x / q", "Close focused pane"),
            ("Ctrl+B h/j/k/l", "Focus pane by direction"),
            ("Ctrl+B t", "Take over observing pane"),
            ("Ctrl+B n", "Spawn new agent / shell"),
            ("Ctrl+B c", "Create new space"),
            ("Ctrl+B K", "Kill focused terminal"),
            ("Ctrl+B R", "Remove finished terminal"),
            ("Ctrl+B r", "Rename current space"),
            ("Ctrl+B b", "Toggle sidebar"),
            ("Ctrl+B Tab", "Toggle focus sidebar / panes"),
            ("Ctrl+B ?", "Show this help"),
            ("Ctrl+B d", "Detach and quit htui"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_dispatch_timeout_and_modal() {
        let mut router = PrefixRouter::new(Duration::from_millis(1500));
        let t0 = Instant::now();
        let ctrl_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);

        assert_eq!(router.route(ctrl_b, false, t0), RouteResult::PrefixArmed);
        assert!(router.is_armed(t0 + Duration::from_millis(100)));

        // Command key works within timeout
        assert_eq!(
            router.route(
                KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE),
                false,
                t0 + Duration::from_millis(200)
            ),
            RouteResult::Command(Command::TakeOver)
        );

        // Timeout expires after 1500ms
        assert_eq!(router.route(ctrl_b, false, t0), RouteResult::PrefixArmed);
        assert_eq!(
            router.route(
                KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE),
                false,
                t0 + Duration::from_millis(1600)
            ),
            RouteResult::ForwardToPane(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE))
        );

        // Modal captures all keys
        let modal_key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        assert_eq!(
            router.route(modal_key, true, t0),
            RouteResult::ModalKey(modal_key)
        );
    }

    #[test]
    fn ctrl_b_q_and_x_route_to_close_pane_and_d_routes_to_quit() {
        let mut router = PrefixRouter::new(Duration::from_millis(1500));
        let t0 = Instant::now();
        let ctrl_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);

        assert_eq!(router.route(ctrl_b, false, t0), RouteResult::PrefixArmed);
        assert_eq!(
            router.route(
                KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
                false,
                t0 + Duration::from_millis(50)
            ),
            RouteResult::Command(Command::ClosePane)
        );

        assert_eq!(router.route(ctrl_b, false, t0), RouteResult::PrefixArmed);
        assert_eq!(
            router.route(
                KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
                false,
                t0 + Duration::from_millis(50)
            ),
            RouteResult::Command(Command::ClosePane)
        );

        assert_eq!(router.route(ctrl_b, false, t0), RouteResult::PrefixArmed);
        assert_eq!(
            router.route(
                KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
                false,
                t0 + Duration::from_millis(50)
            ),
            RouteResult::Command(Command::Quit)
        );
    }

    #[test]
    fn bindings_help_lists_close_pane_x_or_q_and_quit_d() {
        let bindings = PrefixRouter::bindings_help();
        assert!(bindings.contains(&("Ctrl+B x / q", "Close focused pane")));
        assert!(bindings.contains(&("Ctrl+B d", "Detach and quit htui")));
        assert!(!bindings.iter().any(|(key, _)| *key == "Ctrl+B d / q"));
        assert!(bindings.len() <= 18);
        assert!(bindings.iter().any(|(k, _)| k.contains("Ctrl+B H")));
        assert!(bindings.iter().any(|(k, _)| k.contains("Ctrl+B V")));
        assert!(bindings
            .iter()
            .any(|(k, _)| k.contains("[") && k.contains("]")));
    }

    #[test]
    fn ctrl_b_panel_and_split_spawn_commands() {
        let mut router = PrefixRouter::new(Duration::from_millis(1500));
        let t0 = Instant::now();
        let ctrl_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);

        let cases = [
            (
                KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT),
                Command::SplitSpawnHorizontal,
            ),
            (
                KeyEvent::new(KeyCode::Char('V'), KeyModifiers::SHIFT),
                Command::SplitSpawnVertical,
            ),
            (
                KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE),
                Command::PrevPanel,
            ),
            (
                KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE),
                Command::NextPanel,
            ),
        ];

        for (key, expected) in cases {
            assert_eq!(router.route(ctrl_b, false, t0), RouteResult::PrefixArmed);
            assert_eq!(
                router.route(key, false, t0 + Duration::from_millis(50)),
                RouteResult::Command(expected)
            );
        }
    }
}
