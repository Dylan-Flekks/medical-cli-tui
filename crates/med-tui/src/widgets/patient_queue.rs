use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::{App, FocusArea};
use crate::theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(7),
        ])
        .split(area);

    render_search(frame, chunks[0], app);
    render_patients(frame, chunks[1], app);
    render_tasks(frame, chunks[2], app);
}

fn render_search(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let search = Paragraph::new("/ search patients").block(
        Block::default()
            .borders(Borders::ALL)
            .title("Search")
            .border_style(theme::panel_border(app.focus, FocusArea::PatientQueue)),
    );
    frame.render_widget(search, area);
}

fn render_patients(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = app
        .data
        .patients
        .iter()
        .enumerate()
        .map(|(index, patient)| {
            let age = patient
                .age
                .map(|age| age.to_string())
                .unwrap_or_else(|| "-".to_owned());
            let row = Row::new([
                Cell::from(patient.display_name.as_str()),
                Cell::from(patient.mrn.as_str()),
                Cell::from(age),
                Cell::from(patient.status.as_str()),
            ]);

            if index == app.selected_patient {
                row.style(theme::selected_row())
            } else {
                row
            }
        });

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(34),
            Constraint::Percentage(26),
            Constraint::Length(4),
            Constraint::Percentage(30),
        ],
    )
    .header(Row::new(["Name", "MRN", "Age", "Status"]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Patient Queue")
            .border_style(theme::panel_border(app.focus, FocusArea::PatientQueue)),
    );

    frame.render_widget(table, area);
}

fn render_tasks(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = app.data.tasks.iter().map(|task| {
        Row::new([
            Cell::from(task.label.as_str()),
            Cell::from(task.count.to_string()),
        ])
        .style(theme::severity(task.severity))
    });

    let table = Table::new(
        rows,
        [Constraint::Percentage(75), Constraint::Percentage(25)],
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Work Queue")
            .border_style(theme::panel_border(app.focus, FocusArea::PatientQueue)),
    );

    frame.render_widget(table, area);
}
