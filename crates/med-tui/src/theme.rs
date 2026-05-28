use crate::app::{FocusArea, Severity};
use ratatui::style::{Color, Modifier, Style};

pub fn panel_border(focus: FocusArea, area: FocusArea) -> Style {
    if focus == area {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

pub fn selected_row() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

pub fn severity(severity: Severity) -> Style {
    match severity {
        Severity::Info => Style::default().fg(Color::Gray),
        Severity::Warning => Style::default().fg(Color::Yellow),
        Severity::Error => Style::default().fg(Color::LightRed),
        Severity::Blocked => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    }
}
