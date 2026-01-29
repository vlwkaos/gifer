use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use std::time::Duration;
use tokio::time::{interval, Interval};

/// Application events
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AppEvent {
    /// Terminal key event
    Key(KeyEvent),
    /// Terminal resize event
    Resize(u16, u16),
    /// Periodic tick for UI refresh
    Tick,
}

/// Event handler that polls for terminal events
pub struct EventHandler {
    event_stream: EventStream,
    tick_interval: Interval,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let mut tick_interval = interval(tick_rate);
        tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        Self {
            event_stream: EventStream::new(),
            tick_interval,
        }
    }

    /// Poll for the next event
    pub async fn next(&mut self) -> Result<AppEvent> {
        loop {
            tokio::select! {
                _ = self.tick_interval.tick() => {
                    return Ok(AppEvent::Tick);
                }
                event = self.event_stream.next() => {
                    if let Some(Ok(event)) = event {
                        match event {
                            Event::Key(key) => return Ok(AppEvent::Key(key)),
                            Event::Resize(w, h) => return Ok(AppEvent::Resize(w, h)),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

/// Check if a key event is a paste command (Cmd+V on macOS or Ctrl+V)
pub fn is_paste_key(key: &KeyEvent) -> bool {
    matches!(
        (key.code, key.modifiers),
        (KeyCode::Char('v'), KeyModifiers::SUPER) | (KeyCode::Char('v'), KeyModifiers::CONTROL)
    )
}

/// Check if a key event is a quit command
pub fn is_quit_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
}

/// Check if a key event is navigation up
pub fn is_up_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Up | KeyCode::Char('k'))
}

/// Check if a key event is navigation down
pub fn is_down_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Down | KeyCode::Char('j'))
}

/// Check if a key event is navigation left
pub fn is_left_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Left | KeyCode::Char('h'))
}

/// Check if a key event is navigation right
pub fn is_right_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Right | KeyCode::Char('l'))
}
