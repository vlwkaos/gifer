use crate::app::{App, FocusedSection, SettingsField};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

/// Settings panel widget
pub struct SettingsPanel<'a> {
    app: &'a App,
    focused: bool,
}

impl<'a> SettingsPanel<'a> {
    pub fn new(app: &'a App) -> Self {
        Self {
            app,
            focused: app.focused_section == FocusedSection::Settings,
        }
    }

    fn field_style(&self, field: SettingsField) -> Style {
        if self.focused && self.app.selected_setting == field {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        }
    }

    fn label_style(&self) -> Style {
        Style::default().fg(Color::DarkGray)
    }

    fn render_field(&self, label: &str, value: &str, field: SettingsField) -> Vec<Span<'a>> {
        vec![
            Span::styled(format!("{}: ", label), self.label_style()),
            Span::styled(format!("[{}]", value), self.field_style(field)),
            Span::raw("  "),
        ]
    }
}

impl Widget for SettingsPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" Settings ")
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        // Build the two rows of settings
        let mut row1_spans: Vec<Span> = Vec::new();
        row1_spans.extend(self.render_field(
            "Scale",
            self.app.settings.scale.as_str(),
            SettingsField::Scale,
        ));
        row1_spans.extend(self.render_field(
            "FPS",
            self.app.settings.fps.as_str(),
            SettingsField::Fps,
        ));
        row1_spans.extend(self.render_field(
            "Quality",
            self.app.settings.quality.as_str(),
            SettingsField::Quality,
        ));
        row1_spans.extend(self.render_field(
            "Loop",
            &self.app.settings.loop_count.as_str(),
            SettingsField::Loop,
        ));
        row1_spans.extend(self.render_field(
            "Split",
            self.app.settings.size_limit.as_str(),
            SettingsField::SizeLimit,
        ));

        // Output directory - show text input if editing
        let output_value = if self.app.editing_output {
            format!("{}|", self.app.output_input) // Show cursor
        } else {
            self.app.settings.output_dir_display()
        };

        let output_style = if self.app.editing_output {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            self.field_style(SettingsField::OutputDir)
        };

        let row2_spans: Vec<Span> = vec![
            Span::styled("Output: ", self.label_style()),
            Span::styled(format!("[{}]", output_value), output_style),
            if self.focused
                && self.app.selected_setting == SettingsField::OutputDir
                && !self.app.editing_output
            {
                Span::styled(" (Enter to edit)", Style::default().fg(Color::DarkGray))
            } else if self.app.editing_output {
                Span::styled(
                    " (Tab=complete, Enter=save, Esc=cancel)",
                    Style::default().fg(Color::DarkGray),
                )
            } else {
                Span::raw("")
            },
        ];

        let lines = vec![Line::from(row1_spans), Line::from(row2_spans)];

        let paragraph = Paragraph::new(lines);

        // Center vertically if there's room
        let y_offset = if inner.height > 2 {
            (inner.height - 2) / 2
        } else {
            0
        };

        let text_area = Rect {
            x: inner.x + 1,
            y: inner.y + y_offset,
            width: inner.width.saturating_sub(2),
            height: 2.min(inner.height),
        };

        paragraph.render(text_area, buf);
    }
}
