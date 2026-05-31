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
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Length(7),
            Constraint::Min(5),
            Constraint::Length(4),
        ])
        .split(area);

    render_list(frame, chunks[0], "Problems", &app.data.problems, app);
    render_list(frame, chunks[1], "Medications", &app.data.medications, app);
    render_list(frame, chunks[2], "Allergies", &app.data.allergies, app);
    render_agent(frame, chunks[3], app);
    render_safety(frame, chunks[4], app);
    render_billing_ready(frame, chunks[5], app);
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

fn render_agent(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let agent = &app.agent;
    let turn = agent
        .active_turn_id
        .as_deref()
        .map(short_text)
        .unwrap_or_else(|| "-".to_owned());
    let note = agent
        .note_id
        .map(short_text)
        .unwrap_or_else(|| "-".to_owned());
    let approval = agent
        .pending_approval
        .as_ref()
        .map(|pending| {
            format!(
                "Approval: {} | {}",
                approval_class_label(pending.class),
                pending.redacted_reason
            )
        })
        .unwrap_or_else(|| "Approval: none".to_owned());
    let last = agent
        .events
        .last()
        .map(|event| event.message.as_str())
        .unwrap_or("No agent events yet");
    let phi = if agent.contains_phi { "yes" } else { "no" };

    let panel = Paragraph::new(format!(
        "State: {} ({})\nTurn: {turn}  Note: {note}\nPHI: {phi}  Limits: {}/{}s\n{approval}\nLast: {last}",
        agent.status_label(),
        agent.thread_status_label(),
        agent.loop_limits.max_steps,
        agent.loop_limits.max_wall_clock_seconds,
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Agent Turn")
            .border_style(theme::panel_border(app.focus, FocusArea::Context)),
    )
    .wrap(Wrap { trim: true });

    frame.render_widget(panel, area);
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

fn short_text(id: impl std::fmt::Display) -> String {
    id.to_string()[..8].to_owned()
}

fn approval_class_label(class: med_agent::MedicalApprovalClass) -> &'static str {
    match class {
        med_agent::MedicalApprovalClass::OutboundPhi => "outbound PHI",
        med_agent::MedicalApprovalClass::SignedClinicalChange => "signed change",
        med_agent::MedicalApprovalClass::BillingSupportExport => "billing export",
        med_agent::MedicalApprovalClass::DestructiveLocalWrite => "destructive write",
        med_agent::MedicalApprovalClass::DesktopAutomation => "desktop",
        med_agent::MedicalApprovalClass::BulkImport => "bulk import",
        med_agent::MedicalApprovalClass::BulkExport => "bulk export",
        med_agent::MedicalApprovalClass::PluginInstall => "plugin install",
    }
}
