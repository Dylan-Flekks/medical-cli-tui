use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, List, ListItem, Paragraph, Row, Sparkline, Table, Tabs, Wrap,
};
use ratatui::Frame;

use crate::app::{App, FocusArea, WorkspaceTab};
use crate::theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Min(10),
            Constraint::Length(5),
        ])
        .split(area);

    render_tabs(frame, chunks[0], app);
    render_summary(frame, chunks[1], app);

    match app.selected_tab {
        WorkspaceTab::Chart => render_chart(frame, chunks[2], app),
        WorkspaceTab::Note => render_note(frame, chunks[2], app),
        WorkspaceTab::Audit => render_audit(frame, chunks[2], app),
        WorkspaceTab::Billing => render_billing(frame, chunks[2], app),
    }

    render_vitals(frame, chunks[3], app);
}

fn render_tabs(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let titles = WorkspaceTab::ALL
        .iter()
        .map(|tab| Line::from(tab.title()))
        .collect::<Vec<_>>();

    let tabs = Tabs::new(titles)
        .select(app.selected_tab.index())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Workspace")
                .border_style(theme::panel_border(app.focus, FocusArea::Workspace)),
        )
        .highlight_style(
            Style::default()
                .fg(ratatui::style::Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

fn render_summary(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let patient = app.active_patient();
    let name = patient
        .map(|patient| patient.display_name.as_str())
        .unwrap_or("No patient selected");
    let mrn = patient.map(|patient| patient.mrn.as_str()).unwrap_or("-");
    let status = patient
        .map(|patient| patient.status.as_str())
        .unwrap_or("-");
    let active_encounter = app
        .active_encounter()
        .map(|encounter| format!("{} | {}", encounter.encounter_type, encounter.status))
        .unwrap_or_else(|| "None".to_owned());

    let summary = Paragraph::new(vec![
        Line::from(vec![Span::styled(
            name,
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!(
            "MRN: {mrn}  |  Active encounter: {active_encounter}"
        )),
        Line::from(format!("Status: {status}")),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Chart Summary")
            .border_style(theme::panel_border(app.focus, FocusArea::Workspace)),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(summary, area);
}

fn render_chart(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if app.data.encounters.is_empty() {
        let empty = Paragraph::new("No local encounters")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Encounters")
                    .border_style(theme::panel_border(app.focus, FocusArea::Workspace)),
            )
            .wrap(Wrap { trim: true });

        frame.render_widget(empty, area);
        return;
    }

    let rows = app.data.encounters.iter().map(|encounter| {
        Row::new([
            encounter.short_id.as_str(),
            encounter.started_at.as_str(),
            encounter.encounter_type.as_str(),
            encounter.status.as_str(),
            encounter.reason.as_str(),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Percentage(24),
            Constraint::Percentage(22),
            Constraint::Percentage(32),
        ],
    )
    .header(Row::new(["ID", "Started", "Type", "Status", "Reason"]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Encounters")
            .border_style(theme::panel_border(app.focus, FocusArea::Workspace)),
    );

    frame.render_widget(table, area);
}

fn render_note(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut editor = app.note_editor.clone();
    let title = if app.note_dirty {
        "Structured SOAP Draft *"
    } else {
        "Structured SOAP Draft"
    };

    editor.set_block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(theme::panel_border(app.focus, FocusArea::Workspace)),
    );

    frame.render_widget(&editor, area);
}

fn render_audit(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let items = app
        .data
        .audit_flags
        .iter()
        .map(|flag| ListItem::new(flag.message.clone()).style(theme::severity(flag.severity)));

    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title("Audit Flags")),
        area,
    );
}

fn render_billing(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = app.data.billing_rows.iter().map(|row| {
        Row::new(vec![
            Cell::from(row.code.as_str()),
            Cell::from(row.kind.as_str()),
            Cell::from(row.status.as_str()),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(25),
            Constraint::Percentage(35),
            Constraint::Percentage(40),
        ],
    )
    .header(Row::new(["Code", "Type", "Status"]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Billing Workbench"),
    );

    frame.render_widget(table, area);
}

fn render_vitals(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sparkline = Sparkline::default()
        .block(Block::default().borders(Borders::ALL).title("Vitals Trend"))
        .data(app.data.vitals_trend.iter().copied())
        .style(Style::default().fg(ratatui::style::Color::Green));
    frame.render_widget(sparkline, area);
}
