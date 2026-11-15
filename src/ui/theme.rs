use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

impl Theme {
    pub const SUMI_INK0: Color = Color::Rgb(22, 22, 29);
    pub const SUMI_INK1: Color = Color::Rgb(31, 31, 40);
    pub const SUMI_INK3: Color = Color::Rgb(54, 54, 70);
    pub const SUMI_INK4: Color = Color::Rgb(84, 84, 109);
    pub const FUJI_WHITE: Color = Color::Rgb(220, 215, 186);
    pub const OLD_WHITE: Color = Color::Rgb(200, 192, 147);
    pub const CRYSTAL_BLUE: Color = Color::Rgb(126, 156, 216);
    pub const WAVE_BLUE2: Color = Color::Rgb(45, 79, 103);
    pub const SPRING_GREEN: Color = Color::Rgb(152, 187, 108);
    pub const CARP_YELLOW: Color = Color::Rgb(230, 195, 132);
    pub const WAVE_RED: Color = Color::Rgb(228, 104, 118);
    pub const ONI_VIOLET: Color = Color::Rgb(149, 127, 184);
    pub const WAVE_AQUA2: Color = Color::Rgb(122, 168, 159);

    pub fn border_normal() -> Style {
        Style::default().fg(Self::SUMI_INK4)
    }

    pub fn border_focused() -> Style {
        Style::default()
            .fg(Self::CRYSTAL_BLUE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn badge_active() -> Style {
        Style::default()
            .fg(Self::SUMI_INK0)
            .bg(Self::SPRING_GREEN)
            .add_modifier(Modifier::BOLD)
    }

    pub fn badge_observing() -> Style {
        Style::default()
            .fg(Self::SUMI_INK0)
            .bg(Self::CARP_YELLOW)
            .add_modifier(Modifier::BOLD)
    }

    pub fn takeover_button() -> Style {
        Style::default()
            .fg(Self::FUJI_WHITE)
            .bg(Self::WAVE_RED)
            .add_modifier(Modifier::BOLD)
    }

    pub fn selected_row() -> Style {
        Style::default()
            .bg(Self::WAVE_BLUE2)
            .fg(Self::FUJI_WHITE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn status_bar() -> Style {
        Style::default().fg(Self::OLD_WHITE)
    }

    pub fn modal_border() -> Style {
        Style::default()
            .fg(Self::WAVE_AQUA2)
            .add_modifier(Modifier::BOLD)
    }

    pub fn machine_online() -> Style {
        Style::default().fg(Self::SPRING_GREEN)
    }

    pub fn machine_offline() -> Style {
        Style::default().fg(Self::CARP_YELLOW)
    }

    pub fn space_label() -> Style {
        Style::default()
            .fg(Self::CRYSTAL_BLUE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn terminal_live() -> Style {
        Style::default().fg(Self::FUJI_WHITE)
    }

    pub fn terminal_finished() -> Style {
        Style::default().fg(Self::SUMI_INK4)
    }

    pub fn new_space_action() -> Style {
        Style::default().fg(Self::OLD_WHITE).bg(Self::SUMI_INK3)
    }

    pub fn tab_bar_bg() -> Style {
        Style::default().fg(Self::SUMI_INK4)
    }

    pub fn tab_active() -> Style {
        Style::default()
            .fg(Self::SUMI_INK0)
            .bg(Self::CRYSTAL_BLUE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn tab_inactive() -> Style {
        Style::default().fg(Self::OLD_WHITE).bg(Self::SUMI_INK3)
    }

    pub fn tab_new_terminal() -> Style {
        Style::default().fg(Self::OLD_WHITE).bg(Self::SUMI_INK3)
    }

    pub fn selection() -> Style {
        Style::default().bg(Self::WAVE_BLUE2).fg(Self::FUJI_WHITE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kanagawa_wave_palette_styles() {
        assert_eq!(Theme::SUMI_INK0, Color::Rgb(22, 22, 29));
        assert_eq!(Theme::CRYSTAL_BLUE, Color::Rgb(126, 156, 216));
        assert_eq!(Theme::WAVE_BLUE2, Color::Rgb(45, 79, 103));
        assert_eq!(Theme::border_focused().fg, Some(Theme::CRYSTAL_BLUE));
        assert_eq!(Theme::status_bar().bg, None);
        assert_eq!(Theme::status_bar().fg, Some(Theme::OLD_WHITE));
        assert_eq!(Theme::tab_bar_bg().bg, None);
        assert_eq!(Theme::tab_bar_bg().fg, Some(Theme::SUMI_INK4));
        assert_eq!(Theme::tab_new_terminal().bg, Some(Theme::SUMI_INK3));
        assert_eq!(Theme::tab_new_terminal().fg, Some(Theme::OLD_WHITE));
        assert!(!Theme::tab_new_terminal()
            .add_modifier
            .contains(Modifier::BOLD));
        assert_eq!(Theme::new_space_action().bg, Some(Theme::SUMI_INK3));
        assert_eq!(Theme::new_space_action().fg, Some(Theme::OLD_WHITE));
        assert!(!Theme::new_space_action()
            .add_modifier
            .contains(Modifier::BOLD));
        assert_eq!(Theme::tab_active().bg, Some(Theme::CRYSTAL_BLUE));
        assert_eq!(Theme::tab_active().fg, Some(Theme::SUMI_INK0));
        assert!(Theme::tab_active().add_modifier.contains(Modifier::BOLD));
        assert_eq!(Theme::tab_inactive().bg, Some(Theme::SUMI_INK3));
        assert_eq!(Theme::tab_inactive().fg, Some(Theme::OLD_WHITE));
        assert_eq!(Theme::selection().bg, Some(Theme::WAVE_BLUE2));
        assert_eq!(Theme::selection().fg, Some(Theme::FUJI_WHITE));
    }
}
