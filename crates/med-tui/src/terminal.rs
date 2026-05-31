use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use med_agent::{MedicalAgentThread, MedicalAgentThreadConfig};
use med_store::LocalStore;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;
use crate::ui;

pub fn run(store: LocalStore) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal, &store);
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>, store: &LocalStore) -> Result<()> {
    let mut app = App::from_store(store)?;
    let agent_thread = MedicalAgentThread::spawn(MedicalAgentThreadConfig::default());

    while !app.should_quit {
        app.drain_agent_events(&agent_thread, Duration::ZERO)?;
        terminal.draw(|frame| ui::render(frame, &app))?;

        if event::poll(Duration::from_millis(200))? {
            if let CrosstermEvent::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key_with_store_and_agent(key, store, Some(&agent_thread))?;
                }
            }
        }
    }

    agent_thread.shutdown_and_wait()?;
    Ok(())
}
