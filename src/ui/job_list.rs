use crate::app::FocusedSection;
use crate::conversion::{ConversionJob, JobStatus};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Widget},
};

/// Job list widget
pub struct JobListPanel<'a> {
    jobs: &'a [ConversionJob],
    focused: bool,
    selected_index: usize,
}

impl<'a> JobListPanel<'a> {
    pub fn new(
        jobs: &'a [ConversionJob],
        focused_section: FocusedSection,
        selected_index: usize,
    ) -> Self {
        Self {
            jobs,
            focused: focused_section == FocusedSection::JobList,
            selected_index,
        }
    }

    fn status_style(status: &JobStatus) -> Style {
        match status {
            JobStatus::Pending => Style::default().fg(Color::DarkGray),
            JobStatus::Converting => Style::default().fg(Color::Yellow),
            JobStatus::Complete => Style::default().fg(Color::Green),
            JobStatus::Failed(_) => Style::default().fg(Color::Red),
            JobStatus::Cancelled => Style::default().fg(Color::Magenta),
        }
    }

    fn format_job(&self, job: &ConversionJob, is_selected: bool, width: u16) -> ListItem<'a> {
        let status_style = Self::status_style(&job.status);

        let icon = Span::styled(format!("{} ", job.status.icon()), status_style);

        let input_name = job.input_filename();
        let output_display = job.output_display();

        // Calculate available width for the path display
        let available_width = width.saturating_sub(4) as usize; // Account for borders and padding

        // Build the status part based on job status
        let status_part = match &job.status {
            JobStatus::Pending => "[Pending]".to_string(),
            JobStatus::Converting => {
                let bar = job.progress.progress_bar(10);
                format!("{} {:.0}%", bar, job.progress.percentage)
            }
            JobStatus::Complete => {
                if let Some(size) = job.size_display() {
                    format!("[Complete] {}", size)
                } else {
                    "[Complete]".to_string()
                }
            }
            JobStatus::Failed(err) => {
                let short_err = if err.len() > 30 {
                    format!("{}...", &err[..27])
                } else {
                    err.clone()
                };
                format!("[Failed: {}]", short_err)
            }
            JobStatus::Cancelled => "[Cancelled]".to_string(),
        };

        // Build the path part
        let path_part = format!("{} -> {}", input_name, output_display);

        // Truncate path if needed to fit status
        let path_max = available_width.saturating_sub(status_part.len() + 4);
        let path_display = if path_part.len() > path_max && path_max > 3 {
            format!("{}...", &path_part[..path_max - 3])
        } else {
            path_part
        };

        // Calculate padding
        let padding_len =
            available_width.saturating_sub(path_display.len() + status_part.len() + 2);
        let padding = " ".repeat(padding_len);

        let path_span = Span::styled(
            path_display,
            if is_selected && self.focused {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::Gray)
            },
        );

        let status_span = Span::styled(status_part, status_style);

        let line = Line::from(vec![icon, path_span, Span::raw(padding), status_span]);

        let style = if is_selected && self.focused {
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        ListItem::new(line).style(style)
    }
}

impl Widget for JobListPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" Conversion Queue ")
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        if self.jobs.is_empty() {
            let empty_msg = Line::from(vec![
                Span::styled("No jobs. Press ", Style::default().fg(Color::DarkGray)),
                Span::styled("Cmd+V", Style::default().fg(Color::Cyan)),
                Span::styled(" or ", Style::default().fg(Color::DarkGray)),
                Span::styled("p", Style::default().fg(Color::Cyan)),
                Span::styled(
                    " to paste a video file.",
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            let y = inner.y + inner.height / 2;
            let x = inner.x + (inner.width.saturating_sub(40)) / 2;
            buf.set_line(x, y, &empty_msg, inner.width);
            return;
        }

        let items: Vec<ListItem> = self
            .jobs
            .iter()
            .enumerate()
            .map(|(i, job)| self.format_job(job, i == self.selected_index, inner.width))
            .collect();

        let list = List::new(items);
        list.render(inner, buf);
    }
}
