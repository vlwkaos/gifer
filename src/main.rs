mod app;
mod clipboard;
mod config;
mod conversion;
mod event;
mod ui;

use anyhow::Result;
use app::App;
use config::Settings;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use event::{AppEvent, EventHandler};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{stdout, Stdout};
use std::time::Duration;

/// Initialize the terminal for TUI
fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restore terminal to original state
fn teardown_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Check for FFmpeg availability
    if let Err(e) = conversion::check_ffmpeg() {
        eprintln!("FFmpeg not available: {}", e);
        eprintln!("Please install FFmpeg or let ffmpeg-sidecar download it.");
        return Err(e);
    }

    // Load settings
    let settings = Settings::load().unwrap_or_default();

    // Create app state
    let mut app = App::new(settings);

    // Setup terminal
    let mut terminal = setup_terminal()?;

    // Create event handler
    let tick_rate = Duration::from_millis(100);
    let mut event_handler = EventHandler::new(tick_rate);

    // Main event loop
    let result = run_app(&mut terminal, &mut app, &mut event_handler).await;

    // Teardown terminal
    teardown_terminal(&mut terminal)?;

    // Save settings on exit
    if let Err(e) = app.settings.save() {
        eprintln!("Failed to save settings: {}", e);
    }

    result
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    event_handler: &mut EventHandler,
) -> Result<()> {
    loop {
        // Render UI
        terminal.draw(|frame| {
            ui::render(frame, app);
        })?;

        // Handle events
        tokio::select! {
            // Handle terminal events
            event = event_handler.next() => {
                match event? {
                    AppEvent::Key(key) => {
                        app.handle_key(key);
                    }
                    AppEvent::Tick => {
                        app.tick();
                    }
                    AppEvent::Resize(_, _) => {
                        // Terminal will re-render automatically
                    }
                }
            }

            // Handle progress updates from conversion workers
            Some(update) = app.progress_rx.recv() => {
                app.handle_progress_update(update);
            }
        }

        if app.should_quit {
            // Cancel all running jobs
            for job in &mut app.jobs {
                job.cancel();
            }
            break;
        }
    }

    Ok(())
}
