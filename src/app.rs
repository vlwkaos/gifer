use crate::clipboard::{copy_file_to_clipboard, get_url_from_clipboard, get_videos_from_clipboard};
use crate::config::{expand_tilde, Settings};
use crate::conversion::{spawn_conversion, ConversionJob, InputSource, JobStatus, ProgressUpdate};
use crate::event::{is_down_key, is_left_key, is_paste_key, is_quit_key, is_right_key, is_up_key};
use crossterm::event::{KeyCode, KeyEvent};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

/// Which section of the UI is currently focused
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedSection {
    Settings,
    JobList,
}

/// Which setting field is currently selected
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    Scale,
    Fps,
    Quality,
    Loop,
    OutputDir,
}

impl SettingsField {
    fn next(&self) -> Self {
        match self {
            SettingsField::Scale => SettingsField::Fps,
            SettingsField::Fps => SettingsField::Quality,
            SettingsField::Quality => SettingsField::Loop,
            SettingsField::Loop => SettingsField::OutputDir,
            SettingsField::OutputDir => SettingsField::Scale,
        }
    }

    fn prev(&self) -> Self {
        match self {
            SettingsField::Scale => SettingsField::OutputDir,
            SettingsField::Fps => SettingsField::Scale,
            SettingsField::Quality => SettingsField::Fps,
            SettingsField::Loop => SettingsField::Quality,
            SettingsField::OutputDir => SettingsField::Loop,
        }
    }
}

/// Main application state
pub struct App {
    /// Current focused section
    pub focused_section: FocusedSection,
    /// Currently selected setting field
    pub selected_setting: SettingsField,
    /// Currently selected job index
    pub selected_job_index: usize,
    /// Application settings
    pub settings: Settings,
    /// List of conversion jobs
    pub jobs: Vec<ConversionJob>,
    /// Set of processed inputs for duplicate detection (canonicalized paths or URLs)
    pub processed_inputs: HashSet<String>,
    /// Channel sender for progress updates
    pub progress_tx: mpsc::UnboundedSender<ProgressUpdate>,
    /// Channel receiver for progress updates
    pub progress_rx: mpsc::UnboundedReceiver<ProgressUpdate>,
    /// Whether the app should quit
    pub should_quit: bool,
    /// Temporary message to display
    pub message: Option<String>,
    /// Whether the message is an error
    pub message_is_error: bool,
    /// Counter for message timeout
    message_timeout: u8,
    /// Text input mode for output directory
    pub editing_output: bool,
    /// Text buffer for output directory editing
    pub output_input: String,
    /// Text input mode for renaming output file
    pub editing_rename: bool,
    /// Text buffer for rename editing (filename only, no path)
    pub rename_input: String,
    /// Scroll offset for horizontal text scrolling in status bar
    pub scroll_offset: u16,
}

impl App {
    pub fn new(settings: Settings) -> Self {
        let (progress_tx, progress_rx) = mpsc::unbounded_channel();
        let output_input = settings.output_dir_display();

        Self {
            focused_section: FocusedSection::JobList,
            selected_setting: SettingsField::Scale,
            selected_job_index: 0,
            settings,
            jobs: Vec::new(),
            processed_inputs: HashSet::new(),
            progress_tx,
            progress_rx,
            should_quit: false,
            message: None,
            message_is_error: false,
            message_timeout: 0,
            editing_output: false,
            output_input,
            editing_rename: false,
            rename_input: String::new(),
            scroll_offset: 0,
        }
    }

    /// Set a temporary message to display
    pub fn set_message(&mut self, msg: String, is_error: bool) {
        self.message = Some(msg);
        self.message_is_error = is_error;
        self.message_timeout = 20; // ~2 seconds at 100ms tick rate
    }

    /// Clear message if timeout expired
    pub fn tick(&mut self) {
        if self.message_timeout > 0 {
            self.message_timeout -= 1;
            if self.message_timeout == 0 {
                self.message = None;
            }
        }

        // Increment scroll offset for status bar text animation
        self.scroll_offset = self.scroll_offset.wrapping_add(1);

        // Start pending jobs if we have capacity
        self.start_pending_jobs();
    }

    /// Handle key events
    pub fn handle_key(&mut self, key: KeyEvent) {
        // If editing output path, handle text input
        if self.editing_output {
            self.handle_output_input(key);
            return;
        }

        // If renaming output file, handle text input
        if self.editing_rename {
            self.handle_rename_input(key);
            return;
        }

        if is_quit_key(&key) {
            self.should_quit = true;
            return;
        }

        if is_paste_key(&key) || key.code == KeyCode::Char('p') {
            self.paste_from_clipboard();
            return;
        }

        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.focused_section = match self.focused_section {
                    FocusedSection::Settings => FocusedSection::JobList,
                    FocusedSection::JobList => FocusedSection::Settings,
                };
            }
            KeyCode::Char('y') => {
                self.copy_selected_output();
            }
            KeyCode::Char('x') => {
                self.delete_selected_job();
            }
            KeyCode::Char('r') => {
                self.start_rename();
            }
            KeyCode::Enter => {
                // Enter edit mode for output directory
                if self.focused_section == FocusedSection::Settings
                    && self.selected_setting == SettingsField::OutputDir
                {
                    self.editing_output = true;
                    self.output_input = self.settings.output_dir_display();
                }
            }
            _ => match self.focused_section {
                FocusedSection::Settings => self.handle_settings_key(key),
                FocusedSection::JobList => self.handle_job_list_key(key),
            },
        }
    }

    fn handle_output_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                // Apply the input
                let path = expand_tilde(&self.output_input);
                self.settings.output_dir = path;
                self.editing_output = false;
                self.set_message("Output directory updated".to_string(), false);
            }
            KeyCode::Esc => {
                // Cancel editing
                self.editing_output = false;
                self.output_input = self.settings.output_dir_display();
            }
            KeyCode::Backspace => {
                self.output_input.pop();
            }
            KeyCode::Tab => {
                // Auto-complete path
                self.autocomplete_path();
            }
            KeyCode::Char(c) => {
                self.output_input.push(c);
            }
            _ => {}
        }
    }

    fn start_rename(&mut self) {
        if let Some(job) = self.jobs.get(self.selected_job_index) {
            // Allow rename for pending and completed jobs
            if !matches!(job.status, JobStatus::Pending | JobStatus::Complete) {
                self.set_message("Can only rename pending or completed jobs".to_string(), true);
                return;
            }

            // Initialize with current filename (without .gif extension)
            let stem = job
                .output_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            self.rename_input = stem.to_string();
            self.editing_rename = true;
        }
    }

    fn handle_rename_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                // Apply the rename
                let new_name = self.rename_input.trim();
                if new_name.is_empty() {
                    self.set_message("Filename cannot be empty".to_string(), true);
                    return;
                }

                if let Some(job) = self.jobs.get_mut(self.selected_job_index) {
                    let new_path = self.settings.output_dir.join(format!("{}.gif", new_name));

                    // For completed jobs, rename the actual file on disk
                    if job.status == JobStatus::Complete {
                        if let Err(e) = std::fs::rename(&job.output_path, &new_path) {
                            self.set_message(format!("Rename failed: {}", e), true);
                            self.editing_rename = false;
                            return;
                        }
                    }

                    job.output_path = new_path;
                }

                self.editing_rename = false;
                self.set_message("Renamed".to_string(), false);
            }
            KeyCode::Esc => {
                self.editing_rename = false;
            }
            KeyCode::Backspace => {
                self.rename_input.pop();
            }
            KeyCode::Char(c) => {
                // Filter out invalid filename characters
                if !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                    self.rename_input.push(c);
                }
            }
            _ => {}
        }
    }

    fn autocomplete_path(&mut self) {
        let input = expand_tilde(&self.output_input);
        let input_str = input.to_string_lossy();

        // Find the directory and partial filename
        let (dir, prefix) = if input.is_dir() {
            (input.clone(), String::new())
        } else if let Some(parent) = input.parent() {
            let prefix = input
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            (parent.to_path_buf(), prefix)
        } else {
            return;
        };

        // Read directory entries
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };

        // Find matching entries (directories only for output path)
        let mut matches: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .filter(|p| {
                p.file_name()
                    .map(|f| f.to_string_lossy().starts_with(&prefix))
                    .unwrap_or(false)
            })
            .collect();

        matches.sort();

        if matches.len() == 1 {
            // Single match - complete it
            let path = &matches[0];
            self.output_input = crate::config::collapse_tilde(&format!("{}/", path.display()));
        } else if matches.len() > 1 {
            // Multiple matches - find common prefix
            let first = matches[0].to_string_lossy().to_string();
            let mut common_len = first.len();
            for m in &matches[1..] {
                let s = m.to_string_lossy();
                common_len = first
                    .chars()
                    .zip(s.chars())
                    .take_while(|(a, b)| a == b)
                    .count()
                    .min(common_len);
            }
            if common_len > input_str.len() {
                self.output_input = crate::config::collapse_tilde(&first[..common_len]);
            }
        }
    }

    fn handle_settings_key(&mut self, key: KeyEvent) {
        if is_down_key(&key) {
            self.selected_setting = self.selected_setting.next();
        } else if is_up_key(&key) {
            self.selected_setting = self.selected_setting.prev();
        } else if is_right_key(&key) {
            self.adjust_setting(true);
        } else if is_left_key(&key) {
            self.adjust_setting(false);
        }
    }

    fn handle_job_list_key(&mut self, key: KeyEvent) {
        if is_down_key(&key) && !self.jobs.is_empty() {
            self.selected_job_index = (self.selected_job_index + 1) % self.jobs.len();
            self.scroll_offset = 0;
        } else if is_up_key(&key) && !self.jobs.is_empty() {
            self.selected_job_index = if self.selected_job_index == 0 {
                self.jobs.len() - 1
            } else {
                self.selected_job_index - 1
            };
            self.scroll_offset = 0;
        }
    }

    fn adjust_setting(&mut self, increment: bool) {
        match self.selected_setting {
            SettingsField::Scale => {
                self.settings.scale = if increment {
                    self.settings.scale.next()
                } else {
                    self.settings.scale.prev()
                };
            }
            SettingsField::Fps => {
                self.settings.fps = if increment {
                    self.settings.fps.next()
                } else {
                    self.settings.fps.prev()
                };
            }
            SettingsField::Quality => {
                self.settings.quality = if increment {
                    self.settings.quality.next()
                } else {
                    self.settings.quality.prev()
                };
            }
            SettingsField::Loop => {
                self.settings.loop_count = if increment {
                    self.settings.loop_count.next()
                } else {
                    self.settings.loop_count.prev()
                };
            }
            SettingsField::OutputDir => {
                if increment {
                    self.settings.next_output_dir();
                } else {
                    self.settings.prev_output_dir();
                }
                self.output_input = self.settings.output_dir_display();
            }
        }
    }

    fn paste_from_clipboard(&mut self) {
        // Try file paths first (existing behavior)
        match get_videos_from_clipboard() {
            Ok(videos) => {
                let mut added = 0;
                let mut skipped = 0;

                for video_path in videos {
                    let input = InputSource::LocalFile(video_path);
                    let dedup_key = input.dedup_key();

                    if self.processed_inputs.contains(&dedup_key) {
                        skipped += 1;
                        continue;
                    }

                    let output_path = self.create_output_path(&input);
                    let job = ConversionJob::new(input, output_path);
                    self.processed_inputs.insert(dedup_key);
                    self.jobs.push(job);
                    added += 1;
                }

                if added > 0 && skipped > 0 {
                    self.set_message(
                        format!("Added {} video(s), {} skipped (duplicate)", added, skipped),
                        false,
                    );
                } else if added > 0 {
                    self.set_message(format!("Added {} video(s) to queue", added), false);
                } else if skipped > 0 {
                    self.set_message(format!("{} video(s) already in queue", skipped), true);
                }
                return;
            }
            Err(_) => {
                // No files, try URL
            }
        }

        // Try URL from clipboard text
        match get_url_from_clipboard() {
            Ok(Some(url)) => {
                let input = InputSource::RemoteUrl(url.clone());
                let dedup_key = input.dedup_key();

                if self.processed_inputs.contains(&dedup_key) {
                    self.set_message("URL already in queue".to_string(), true);
                    return;
                }

                let output_path = self.create_output_path(&input);
                let job = ConversionJob::new(input, output_path);
                self.processed_inputs.insert(dedup_key);
                self.jobs.push(job);
                self.set_message("Added URL to queue".to_string(), false);
            }
            Ok(None) => {
                self.set_message("No video file or URL in clipboard".to_string(), true);
            }
            Err(e) => {
                self.set_message(format!("Clipboard: {}", e), true);
            }
        }
    }

    fn create_output_path(&self, input: &InputSource) -> PathBuf {
        let stem = input.file_stem().unwrap_or_else(|| {
            // Fallback: generate timestamp-based name
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("video-{}", ts)
        });

        let mut output = self.settings.output_dir.join(format!("{}.gif", stem));

        // Handle existing files by adding a number suffix
        let mut counter = 1;
        while output.exists() {
            output = self
                .settings
                .output_dir
                .join(format!("{}_{}.gif", stem, counter));
            counter += 1;
        }

        output
    }

    fn copy_selected_output(&mut self) {
        if let Some(job) = self.jobs.get(self.selected_job_index) {
            if job.status == JobStatus::Complete {
                match copy_file_to_clipboard(&job.output_path) {
                    Ok(()) => {
                        self.set_message(format!("Copied: {}", job.output_filename()), false);
                    }
                    Err(e) => {
                        self.set_message(format!("Copy failed: {}", e), true);
                    }
                }
            } else {
                self.set_message("Can only copy completed jobs".to_string(), true);
            }
        }
    }

    fn delete_selected_job(&mut self) {
        if self.jobs.is_empty() {
            return;
        }

        let index = self.selected_job_index;
        if let Some(job) = self.jobs.get_mut(index) {
            // Cancel if running
            job.cancel();

            // Remove from processed inputs
            let dedup_key = job.input.dedup_key();
            self.processed_inputs.remove(&dedup_key);
        }

        // Remove job
        self.jobs.remove(index);

        // Adjust selection
        if self.selected_job_index >= self.jobs.len() && !self.jobs.is_empty() {
            self.selected_job_index = self.jobs.len() - 1;
        }

        self.set_message("Job deleted".to_string(), false);
    }

    /// Start pending jobs up to max_concurrent limit
    fn start_pending_jobs(&mut self) {
        // Count currently running jobs
        let running_count = self
            .jobs
            .iter()
            .filter(|j| j.status.is_converting())
            .count();

        // Start pending jobs up to limit
        let slots_available = self.settings.max_concurrent.saturating_sub(running_count);

        for job in self.jobs.iter_mut() {
            if slots_available == 0 {
                break;
            }

            if job.status == JobStatus::Pending {
                // Ensure output directory exists
                if let Err(e) = self.settings.ensure_output_dir() {
                    job.status = JobStatus::Failed(format!("Output dir error: {}", e));
                    continue;
                }

                // Spawn conversion worker
                let cancel_tx = spawn_conversion(
                    job.id,
                    job.input.clone(),
                    job.output_path.clone(),
                    self.settings.clone(),
                    self.progress_tx.clone(),
                );

                job.cancel_tx = Some(cancel_tx);
                job.status = JobStatus::Converting;
            }
        }
    }

    /// Process a progress update from a worker
    pub fn handle_progress_update(&mut self, update: ProgressUpdate) {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == update.job_id) {
            if let Some(status) = update.status {
                job.status = status;
            }
            if let Some(progress) = update.progress {
                job.progress = progress;
            }
        }
    }
}
