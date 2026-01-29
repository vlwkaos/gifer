use crate::app::App;
use crate::ui::job_list::JobListPanel;
use crate::ui::settings::SettingsPanel;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
    Frame,
};

/// Render the main application layout
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Main layout: Settings (fixed), Job List (flexible), Help Bar (fixed)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Settings panel
            Constraint::Min(5),    // Job list
            Constraint::Length(3), // Help bar
        ])
        .split(area);

    // Render settings panel
    let settings_panel = SettingsPanel::new(app);
    frame.render_widget(settings_panel, chunks[0]);

    // Render job list
    let job_list = JobListPanel::new(&app.jobs, app.focused_section, app.selected_job_index);
    frame.render_widget(job_list, chunks[1]);

    // Render help bar
    let help_bar = HelpBar::new(app);
    frame.render_widget(help_bar, chunks[2]);
}

/// Help bar widget showing keybindings
struct HelpBar<'a> {
    app: &'a App,
}

impl<'a> HelpBar<'a> {
    fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for HelpBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        block.render(area, buf);

        let key_style = Style::default().fg(Color::Cyan);
        let sep_style = Style::default().fg(Color::DarkGray);
        let desc_style = Style::default().fg(Color::Gray);

        let bindings = [
            ("p", "Paste"),
            ("y", "Copy"),
            ("x", "Del"),
            ("Tab", "Switch"),
            ("jk", "Nav"),
            ("hl", "Set"),
            ("q", "Quit"),
        ];

        let mut spans: Vec<Span> = Vec::new();
        for (i, (key, desc)) in bindings.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(" | ", sep_style));
            }
            spans.push(Span::styled(*key, key_style));
            spans.push(Span::styled(":", sep_style));
            spans.push(Span::styled(*desc, desc_style));
        }

        // Show message if there is one
        if let Some(msg) = &self.app.message {
            spans.clear();
            let msg_style = if self.app.message_is_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };
            spans.push(Span::styled(msg.clone(), msg_style));
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line);
        paragraph.render(inner, buf);
    }
}
