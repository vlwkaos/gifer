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

    // Main layout: Settings, Path Info, Job List, Help Bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Settings panel
            Constraint::Length(1), // Path info (scrolling)
            Constraint::Min(5),    // Job list
            Constraint::Length(3), // Help bar (keybindings)
        ])
        .split(area);

    // Render settings panel
    let settings_panel = SettingsPanel::new(app);
    frame.render_widget(settings_panel, chunks[0]);

    // Render path info bar
    let path_bar = PathInfoBar::new(app);
    frame.render_widget(path_bar, chunks[1]);

    // Render job list
    let job_list = JobListPanel::new(&app.jobs, app.focused_section, app.selected_job_index);
    frame.render_widget(job_list, chunks[2]);

    // Render help bar
    let help_bar = HelpBar::new(app);
    frame.render_widget(help_bar, chunks[3]);
}

/// Path info bar showing selected job's full path with scroll
struct PathInfoBar<'a> {
    app: &'a App,
}

impl<'a> PathInfoBar<'a> {
    fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for PathInfoBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let desc_style = Style::default().fg(Color::DarkGray);

        if self.app.jobs.is_empty() {
            return;
        }

        if let Some(job) = self.app.jobs.get(self.app.selected_job_index) {
            let input_display = job.input.ffmpeg_input();
            let output_display = job.output_path.display().to_string();
            let path_info = format!("{} -> {}", input_display, output_display);

            let max_width = area.width.saturating_sub(2) as usize;
            let char_count = path_info.chars().count();
            let display = if char_count > max_width {
                let offset = (self.app.scroll_offset as usize) % char_count;
                let padded = format!("{}   {}", path_info, path_info);
                padded.chars().skip(offset).take(max_width).collect::<String>()
            } else {
                path_info
            };

            let paragraph = Paragraph::new(Span::styled(display, desc_style));
            paragraph.render(area, buf);
        }
    }
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

        // Show rename input, message, or keybindings
        if self.app.editing_rename {
            spans.push(Span::styled("Rename: ", desc_style));
            spans.push(Span::styled(self.app.rename_input.clone(), key_style));
            spans.push(Span::styled(".gif  ", desc_style));
            spans.push(Span::styled("(enter)", key_style));
            spans.push(Span::styled(" save ", desc_style));
            spans.push(Span::styled("(esc)", key_style));
            spans.push(Span::styled(" cancel", desc_style));
        } else if let Some(msg) = &self.app.message {
            let msg_style = if self.app.message_is_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };
            spans.push(Span::styled(msg.clone(), msg_style));
        } else {
            // Keybindings
            let help = vec![
                ("(p)", "aste  "),
                ("(y)", " copy  "),
                ("(x)", " del  "),
                ("(r)", "ename  "),
                ("(tab)", " switch  "),
                ("(j/k)", " nav  "),
                ("(q)", "uit"),
            ];
            let keys_len: usize = help.iter().map(|(k, d)| k.len() + d.len()).sum();
            for (key, desc) in help {
                spans.push(Span::styled(key, key_style));
                spans.push(Span::styled(desc, desc_style));
            }

            // Version on the right
            let version = env!("CARGO_PKG_VERSION");
            let padding = inner.width.saturating_sub(keys_len as u16 + version.len() as u16 + 3);
            spans.push(Span::raw(" ".repeat(padding as usize)));
            spans.push(Span::styled(format!("v{}", version), Style::default().fg(Color::DarkGray)));
        }

        let paragraph = Paragraph::new(Line::from(spans));
        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_unicode_scroll_slicing() {
        // Test that scrolling works with Unicode characters (e.g., narrow no-break space \u{202f})
        let path_info = "Screen\u{202f}Recording 2026-02-02 at 7.27.31\u{202f}PM.mov -> output.gif";
        let max_width = 40;
        let char_count = path_info.chars().count();

        // Test various scroll offsets don't panic
        for offset in 0..char_count {
            let padded = format!("{}   {}", path_info, path_info);
            let display: String = padded.chars().skip(offset).take(max_width).collect();
            assert!(display.chars().count() <= max_width);
        }
    }
}
