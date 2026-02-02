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
        let desc_style = Style::default().fg(Color::Gray);

        let mut spans: Vec<Span> = Vec::new();

        // Show rename input if editing
        if self.app.editing_rename {
            spans.push(Span::styled("Rename: ", desc_style));
            spans.push(Span::styled(self.app.rename_input.clone(), key_style));
            spans.push(Span::styled(".gif  ", desc_style));
            spans.push(Span::styled("(enter)", key_style));
            spans.push(Span::styled(" save ", desc_style));
            spans.push(Span::styled("(esc)", key_style));
            spans.push(Span::styled(" cancel", desc_style));
        } else if let Some(msg) = &self.app.message {
            // Show message if there is one
            let msg_style = if self.app.message_is_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };
            spans.push(Span::styled(msg.clone(), msg_style));
        } else if !self.app.jobs.is_empty() {
            // Show selected job's full path
            if let Some(job) = self.app.jobs.get(self.app.selected_job_index) {
                let input_display = job.input.ffmpeg_input();
                let output_display = job.output_path.display().to_string();
                let path_info = format!("{} -> {}", input_display, output_display);

                // Truncate if too wide, with scroll offset
                let max_width = inner.width.saturating_sub(2) as usize;
                let display = if path_info.len() > max_width {
                    let offset = (self.app.scroll_offset as usize) % path_info.len();
                    let padded = format!("{}   {}", path_info, path_info);
                    padded[offset..].chars().take(max_width).collect::<String>()
                } else {
                    path_info
                };
                spans.push(Span::styled(display, desc_style));
            }
        } else {
            // Show keybindings when no jobs
            // Format: (p)aste (y) copy (x) del (r)ename (tab) switch (j/k) nav (q)uit
            let help = vec![
                ("(p)", "aste  "),
                ("(y)", " copy  "),
                ("(x)", " del  "),
                ("(r)", "ename  "),
                ("(tab)", " switch  "),
                ("(j/k)", " nav  "),
                ("(q)", "uit"),
            ];
            for (key, desc) in help {
                spans.push(Span::styled(key, key_style));
                spans.push(Span::styled(desc, desc_style));
            }
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line);
        paragraph.render(inner, buf);
    }
}
