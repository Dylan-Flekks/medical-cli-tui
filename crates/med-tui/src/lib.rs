use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Gauge, List, ListItem, Paragraph, Row, Sparkline, Table, Tabs, Wrap,
};
use ratatui::{Frame, Terminal};

pub fn run() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    let mut app = App::default();

    loop {
        terminal.draw(|frame| render(frame, &app))?;

        if event::poll(Duration::from_millis(200))? {
            if let CrosstermEvent::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Tab => app.next_tab(),
                        KeyCode::BackTab => app.previous_tab(),
                        _ => {}
                    }
                }
            }
        }
    }
}

#[derive(Debug, Default)]
struct App {
    selected_tab: usize,
}

impl App {
    fn next_tab(&mut self) {
        self.selected_tab = (self.selected_tab + 1) % CHART_TABS.len();
    }

    fn previous_tab(&mut self) {
        self.selected_tab = if self.selected_tab == 0 {
            CHART_TABS.len() - 1
        } else {
            self.selected_tab - 1
        };
    }
}

const CHART_TABS: [&str; 4] = ["Chart", "Note", "Audit", "Billing"];

fn render(frame: &mut Frame<'_>, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(30),
            Constraint::Min(50),
            Constraint::Length(34),
        ])
        .split(root[0]);

    render_patient_queue(frame, columns[0]);
    render_main_panel(frame, columns[1], app);
    render_context_panel(frame, columns[2]);
    render_status_bar(frame, root[1]);
}

fn render_patient_queue(frame: &mut Frame<'_>, area: Rect) {
    let patients = vec![
        ListItem::new("Jane Example  MRN-0001"),
        ListItem::new("Sam Sample    MRN-0002"),
        ListItem::new("Avery Demo    MRN-0003"),
        ListItem::new("Unsigned notes: 2").style(Style::default().fg(Color::Yellow)),
        ListItem::new("Billing flags: 3").style(Style::default().fg(Color::LightRed)),
    ];

    let list = List::new(patients).block(
        Block::default()
            .title("Patient Queue")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(list, area);
}

fn render_main_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Min(10),
            Constraint::Length(5),
        ])
        .split(area);

    let tabs = Tabs::new(
        CHART_TABS
            .iter()
            .map(|tab| Line::from(*tab))
            .collect::<Vec<_>>(),
    )
    .select(app.selected_tab)
    .block(Block::default().borders(Borders::ALL).title("Workspace"))
    .highlight_style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(tabs, chunks[0]);

    let summary = Paragraph::new(vec![
        Line::from(vec![Span::styled(
            "Jane Example",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("DOB: 1984-04-12  |  Active encounter: Office visit"),
        Line::from("Chief concern: synthetic demo record for terminal dashboard design"),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Chart Summary"),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(summary, chunks[1]);

    match CHART_TABS[app.selected_tab] {
        "Note" => render_note_editor_preview(frame, chunks[2]),
        "Audit" => render_audit_preview(frame, chunks[2]),
        "Billing" => render_billing_preview(frame, chunks[2]),
        _ => render_chart_preview(frame, chunks[2]),
    }

    let sparkline = Sparkline::default()
        .block(Block::default().borders(Borders::ALL).title("Vitals Trend"))
        .data([98, 99, 97, 101, 100, 99, 98, 97])
        .style(Style::default().fg(Color::Green));
    frame.render_widget(sparkline, chunks[3]);
}

fn render_chart_preview(frame: &mut Frame<'_>, area: Rect) {
    let rows = [
        Row::new(["Problem", "Status", "Linked Dx"]),
        Row::new(["Low back pain", "Active", "M54.50"]),
        Row::new(["Hypertension", "Active", "I10"]),
        Row::new(["Medication review", "Due", "-"]),
    ];

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(45),
            Constraint::Percentage(25),
            Constraint::Percentage(30),
        ],
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Clinical Chart"),
    )
    .style(Style::default().fg(Color::White));

    frame.render_widget(table, area);
}

fn render_note_editor_preview(frame: &mut Frame<'_>, area: Rect) {
    let text = [
        "SOAP Note - Draft",
        "",
        "Subjective:",
        "  Synthetic patient reports improving symptoms.",
        "",
        "Objective:",
        "  Vitals and exam findings pending.",
        "",
        "Assessment:",
        "  Link assessment to diagnosis codes before signing.",
        "",
        "Plan:",
        "  Complete plan and run documentation audit.",
    ]
    .join("\n");

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Structured Note"),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn render_audit_preview(frame: &mut Frame<'_>, area: Rect) {
    let items = vec![
        ListItem::new("warning: Assessment missing linked diagnosis"),
        ListItem::new("warning: Procedure code lacks supporting note section"),
        ListItem::new("info: Note is still unsigned"),
        ListItem::new("blocked: AI PHI request has no executed BAA"),
    ];

    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title("Audit Flags")),
        area,
    );
}

fn render_billing_preview(frame: &mut Frame<'_>, area: Rect) {
    let rows = [
        Row::new(vec![
            Cell::from("Code"),
            Cell::from("Type"),
            Cell::from("Status"),
        ]),
        Row::new(vec![
            Cell::from("M54.50"),
            Cell::from("ICD-10-CM"),
            Cell::from("linked"),
        ]),
        Row::new(vec![
            Cell::from("97110"),
            Cell::from("CPT"),
            Cell::from("needs note support"),
        ]),
    ];

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(25),
            Constraint::Percentage(35),
            Constraint::Percentage(40),
        ],
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Billing Workbench"),
    );

    frame.render_widget(table, area);
}

fn render_context_panel(frame: &mut Frame<'_>, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Min(5),
            Constraint::Length(5),
        ])
        .split(area);

    frame.render_widget(
        List::new(vec![
            ListItem::new("Low back pain"),
            ListItem::new("Hypertension"),
            ListItem::new("Medication review due"),
        ])
        .block(Block::default().borders(Borders::ALL).title("Problems")),
        chunks[0],
    );

    frame.render_widget(
        List::new(vec![
            ListItem::new("Lisinopril 10 mg"),
            ListItem::new("Ibuprofen PRN"),
        ])
        .block(Block::default().borders(Borders::ALL).title("Medications")),
        chunks[1],
    );

    frame.render_widget(
        Paragraph::new(
            "Allergy: NKDA\n\nAI: PHI blocked until provider BAA is executed and approved.",
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Safety Context"),
        )
        .wrap(Wrap { trim: true }),
        chunks[2],
    );

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Billing Ready"),
        )
        .gauge_style(Style::default().fg(Color::Yellow))
        .percent(42);
    frame.render_widget(gauge, chunks[3]);
}

fn render_status_bar(frame: &mut Frame<'_>, area: Rect) {
    let status = Line::from(vec![
        " q ".black().on_cyan(),
        " quit  ".into(),
        " tab ".black().on_cyan(),
        " switch  ".into(),
        " PHI local-only ".black().on_green(),
        " AI BAA gate: locked ".black().on_yellow(),
    ]);

    frame.render_widget(Paragraph::new(status), area);
}
