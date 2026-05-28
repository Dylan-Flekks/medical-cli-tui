use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Color;
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{AiStatus, App, FocusArea};
use crate::theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Min(5),
            Constraint::Length(5),
        ])
        .split(area);

    render_list(frame, chunks[0], "Problems", &app.data.problems, app);
    render_list(frame, chunks[1], "Medications", &app.data.medications, app);
    render_list(frame, chunks[2], "Allergies", &app.data.allergies, app);
    render_safety(frame, chunks[3], app);
    render_billing_ready(frame, chunks[4], app);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, title: &str, items: &[String], app: &App) {
    let items = items.iter().map(|item| ListItem::new(item.as_str()));
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(theme::panel_border(app.focus, FocusArea::Context)),
    );
    frame.render_widget(list, area);
}

fn render_safety(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let ai_status = match app.data.ai_status {
        AiStatus::Locked => "AI: locked for PHI until executed BAA and approval",
        AiStatus::Allowed => "AI: approved provider available",
    };

    let safety = Paragraph::new(format!("Local storage only\nNo PHI in GitHub\n{ai_status}"))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Safety Context")
                .border_style(theme::panel_border(app.focus, FocusArea::Context)),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(safety, area);
}

fn render_billing_ready(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Billing Ready")
                .border_style(theme::panel_border(app.focus, FocusArea::Context)),
        )
        .gauge_style(ratatui::style::Style::default().fg(Color::Yellow))
        .percent(app.data.billing_ready_percent);
    frame.render_widget(gauge, area);
}
