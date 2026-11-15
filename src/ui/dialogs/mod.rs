pub mod confirm;
pub mod help;
pub mod new_space;
pub mod rename;
pub mod spawn;

use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Clear, Widget};

pub use confirm::{ConfirmAction, ConfirmDialog};
pub use help::HelpDialog;
pub use new_space::NewSpaceDialog;
pub use rename::RenameSpaceDialog;
pub use spawn::SpawnDialog;

#[derive(Debug, Clone)]
pub enum ActiveDialog {
    Spawn(SpawnDialog),
    Confirm(ConfirmDialog),
    NewSpace(NewSpaceDialog),
    RenameSpace(RenameSpaceDialog),
    Help(HelpDialog),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogOutcome {
    None,
    Close,
    SubmitSpawn {
        command_or_template: String,
        task: String,
        space: String,
    },
    SubmitConfirm(ConfirmAction),
    SubmitNewSpace(String),
    SubmitRename {
        old_space: String,
        new_space: String,
    },
}

impl ActiveDialog {
    pub fn handle_key(&mut self, key: KeyEvent) -> DialogOutcome {
        match self {
            Self::Spawn(d) => d.handle_key(key),
            Self::Confirm(d) => d.handle_key(key),
            Self::NewSpace(d) => d.handle_key(key),
            Self::RenameSpace(d) => d.handle_key(key),
            Self::Help(d) => d.handle_key(key),
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let popup = centered_rect(60, 50, area);
        Clear.render(popup, buf);
        match self {
            Self::Spawn(d) => d.render(popup, buf),
            Self::Confirm(d) => d.render(popup, buf),
            Self::NewSpace(d) => d.render(popup, buf),
            Self::RenameSpace(d) => d.render(popup, buf),
            Self::Help(d) => d.render(popup, buf),
        }
    }
}

pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let w = ((area.width as u32 * percent_x as u32) / 100)
        .max(20)
        .min(area.width as u32) as u16;
    let h = ((area.height as u32 * percent_y as u32) / 100)
        .max(7)
        .min(area.height as u32) as u16;
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w, h)
}
