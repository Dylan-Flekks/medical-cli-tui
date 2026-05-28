use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::app::App;
use crate::widgets::{context_panel, patient_queue, status_bar, workspace};

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(32),
            Constraint::Min(54),
            Constraint::Length(36),
        ])
        .split(root[0]);

    patient_queue::render(frame, columns[0], app);
    workspace::render(frame, columns[1], app);
    context_panel::render(frame, columns[2], app);
    status_bar::render(frame, root[1], app);
}
