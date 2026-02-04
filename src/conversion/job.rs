use std::path::PathBuf;
use tokio::sync::oneshot;
use uuid::Uuid;

/// Input source for a conversion job
#[derive(Debug, Clone)]
pub enum InputSource {
    /// Local file path
    LocalFile(PathBuf),
    /// Remote URL (http/https)
    RemoteUrl(String),
}

impl InputSource {
    /// Get display name (filename or URL truncated)
    pub fn display_name(&self) -> String {
        match self {
            InputSource::LocalFile(path) => path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            InputSource::RemoteUrl(url) => {
                // Extract filename from URL path
                if let Some(path) = url.split('?').next() {
                    if let Some(filename) = path.rsplit('/').next() {
                        if !filename.is_empty() && filename.contains('.') {
                            return filename.to_string();
                        }
                    }
                }
                // Fallback: truncate URL
                if url.chars().count() > 40 {
                    format!("{}...", url.chars().take(37).collect::<String>())
                } else {
                    url.clone()
                }
            }
        }
    }

    /// Get the input argument for FFmpeg
    pub fn ffmpeg_input(&self) -> String {
        match self {
            InputSource::LocalFile(path) => path.to_string_lossy().to_string(),
            InputSource::RemoteUrl(url) => url.clone(),
        }
    }

    /// Extract stem for output filename
    pub fn file_stem(&self) -> Option<String> {
        match self {
            InputSource::LocalFile(path) => {
                path.file_stem().and_then(|s| s.to_str()).map(String::from)
            }
            InputSource::RemoteUrl(url) => {
                // Extract filename from URL, then stem
                url.split('?')
                    .next()
                    .and_then(|path| path.rsplit('/').next())
                    .and_then(|filename| filename.rsplit_once('.'))
                    .map(|(stem, _)| stem.to_string())
            }
        }
    }

    /// Get unique key for duplicate detection
    pub fn dedup_key(&self) -> String {
        match self {
            InputSource::LocalFile(path) => path
                .canonicalize()
                .unwrap_or_else(|_| path.clone())
                .to_string_lossy()
                .to_string(),
            InputSource::RemoteUrl(url) => url.clone(),
        }
    }
}

/// Status of a conversion job
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    /// Waiting to be processed
    Pending,
    /// Currently being converted
    Converting,
    /// Successfully completed
    Complete,
    /// Failed with error message
    Failed(String),
    /// Cancelled by user
    Cancelled,
}

impl JobStatus {
    /// Get the status icon for display
    pub fn icon(&self) -> &'static str {
        match self {
            JobStatus::Pending => "○",
            JobStatus::Converting => "▶",
            JobStatus::Complete => "✓",
            JobStatus::Failed(_) => "✗",
            JobStatus::Cancelled => "⊘",
        }
    }

    /// Check if job is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobStatus::Complete | JobStatus::Failed(_) | JobStatus::Cancelled
        )
    }

    /// Check if job is actively converting
    pub fn is_converting(&self) -> bool {
        matches!(self, JobStatus::Converting)
    }
}

/// Progress information for a conversion
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct ConversionProgress {
    /// Current frame being processed
    pub frame: u64,
    /// Total frames (if known)
    pub total_frames: Option<u64>,
    /// Progress percentage (0-100)
    pub percentage: f32,
    /// Processing speed (e.g., "2.5x")
    pub speed: Option<String>,
    /// Output file size in bytes (when complete)
    pub output_size: Option<u64>,
}

impl ConversionProgress {
    /// Get progress bar representation
    pub fn progress_bar(&self, width: usize) -> String {
        let filled = ((self.percentage / 100.0) * width as f32) as usize;
        let empty = width.saturating_sub(filled);
        format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
    }
}

/// A video to GIF conversion job
pub struct ConversionJob {
    /// Unique identifier
    pub id: Uuid,
    /// Input source (local file or remote URL)
    pub input: InputSource,
    /// Output GIF file path
    pub output_path: PathBuf,
    /// Current status
    pub status: JobStatus,
    /// Conversion progress
    pub progress: ConversionProgress,
    /// Sender for cancellation signal (None if not cancellable)
    pub cancel_tx: Option<oneshot::Sender<()>>,
}

impl ConversionJob {
    /// Create a new pending conversion job
    pub fn new(input: InputSource, output_path: PathBuf) -> Self {
        Self {
            id: Uuid::new_v4(),
            input,
            output_path,
            status: JobStatus::Pending,
            progress: ConversionProgress::default(),
            cancel_tx: None,
        }
    }

    /// Get the input filename for display
    pub fn input_filename(&self) -> String {
        self.input.display_name()
    }

    /// Get the output filename for display
    pub fn output_filename(&self) -> String {
        self.output_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Get a short display path for the output
    pub fn output_display(&self) -> String {
        // Show parent dir + filename
        if let Some(parent) = self.output_path.parent() {
            if let Some(parent_name) = parent.file_name() {
                return format!(
                    "{}/{}",
                    parent_name.to_string_lossy(),
                    self.output_filename()
                );
            }
        }
        self.output_filename()
    }

    /// Format output size for display
    pub fn size_display(&self) -> Option<String> {
        self.progress.output_size.map(format_size)
    }

    /// Cancel the job if it's running
    pub fn cancel(&mut self) {
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
        if !self.status.is_terminal() {
            self.status = JobStatus::Cancelled;
        }
    }
}

/// Format bytes as human-readable size
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}KB", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

/// Progress update message sent from worker to main thread
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    /// Job ID this update is for
    pub job_id: Uuid,
    /// New status (if changed)
    pub status: Option<JobStatus>,
    /// Progress update
    pub progress: Option<ConversionProgress>,
}
