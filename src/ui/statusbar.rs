use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Paragraph, Widget};

use crate::ui::theme::Theme;

pub fn render_statusbar(
    area: Rect,
    buf: &mut Buffer,
    core_name: &str,
    space_name: &str,
    prefix_armed: bool,
    toast: Option<&str>,
) {
    let prefix_tag = if prefix_armed { " [PREFIX] " } else { "" };
    let right_msg = toast.unwrap_or("Ctrl+B ? Help | Ctrl+B d Quit");
    let text = format!(" htui | {core_name} | space: {space_name}{prefix_tag}  —  {right_msg}");
    Paragraph::new(text)
        .style(Theme::status_bar())
        .render(area, buf);
}
