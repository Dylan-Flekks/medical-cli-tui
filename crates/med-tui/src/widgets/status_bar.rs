use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{AiStatus, App, WorkspaceTab};

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let ai = match app.data.ai_status {
        AiStatus::Locked => " AI BAA gate: locked ",
        AiStatus::Allowed => " AI BAA gate: allowed ",
    };

    let mut spans = vec![
        " q ".black().on_cyan(),
        " quit  ".into(),
        " tab ".black().on_cyan(),
        " focus  ".into(),
        " 1-4 ".black().on_cyan(),
        " tabs  ".into(),
        " j/k ".black().on_cyan(),
        " select  ".into(),
        " n ".black().on_cyan(),
        " patient  ".into(),
        " e ".black().on_cyan(),
        " encounter  ".into(),
        " r ".black().on_cyan(),
        " refresh  ".into(),
        " F5 ".black().on_cyan(),
        " agent  ".into(),
        " F6 ".black().on_yellow(),
        " approve  ".into(),
        " F7 ".black().on_yellow(),
        " deny  ".into(),
        " F8 ".black().on_yellow(),
        " cancel  ".into(),
    ];

    if app.selected_tab == WorkspaceTab::Note {
        if app.note_is_signed() {
            spans.extend([" signed ".black().on_green(), " locked  ".into()]);
        } else {
            spans.extend([
                " ctrl+s ".black().on_cyan(),
                " save  ".into(),
                " S ".black().on_yellow(),
                if app.note_signing_armed {
                    " confirm sign  ".into()
                } else {
                    " sign  ".into()
                },
            ]);
        }

        if let Some(note_id) = app.note_draft_id {
            let status = app.note_status.as_deref().unwrap_or("Draft");
            let version = app
                .note_version
                .map(|version| format!(" v{version}"))
                .unwrap_or_default();
            spans.extend([format!(" note: {} {status}{version} ", short_id(note_id))
                .black()
                .on_dark_gray()]);
        }

        if let Some(signed_at) = &app.note_signed_at {
            spans.extend([format!(" signed: {signed_at} ").black().on_dark_gray()]);
        }
    } else if app.selected_tab == WorkspaceTab::Billing {
        spans.extend([" b ".black().on_cyan(), " superbill  ".into()]);
    }

    spans.extend([
        format!(" focus: {} ", app.focus.title())
            .black()
            .on_dark_gray(),
        format!(" agent: {} ", app.agent.status_label())
            .black()
            .on_dark_gray(),
        " PHI local-only ".black().on_green(),
        ai.black().on_yellow(),
        format!(" {} ", app.last_message).black().on_dark_gray(),
    ]);

    let status = Line::from(spans);

    frame.render_widget(Paragraph::new(status), area);
}

fn short_id(id: impl std::fmt::Display) -> String {
    id.to_string()[..8].to_owned()
}
