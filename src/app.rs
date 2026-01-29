use crate::clipboard::{copy_file_to_clipboard, get_videos_from_clipboard};
use crate::config::{expand_tilde, Settings};
use crate::conversion::{spawn_conversion, ConversionJob, JobStatus, ProgressUpdate};
use crate::event::{is_down_key, is_left_key, is_paste_key, is_quit_key, is_right_key, is_up_key};
use crossterm::event::{KeyCode, KeyEvent};
use std::collections::HashSet;
use std::path::PathBuf;
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
    /// Set of processed file paths for duplicate detection
    pub processed_paths: HashSet<PathBuf>,
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
            processed_paths: HashSet::new(),
            progress_tx,
            progress_rx,
            should_quit: false,
            message: None,
            message_is_error: false,
            message_timeout: 0,
            editing_output: false,
            output_input,
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
        } else if is_up_key(&key) && !self.jobs.is_empty() {
            self.selected_job_index = if self.selected_job_index == 0 {
                self.jobs.len() - 1
            } else {
                self.selected_job_index - 1
            };
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
        match get_videos_from_clipboard() {
            Ok(videos) => {
                let mut added = 0;
                let mut skipped = 0;

                for video_path in videos {
                    // Check for duplicates
                    let canonical = video_path
                        .canonicalize()
                        .unwrap_or_else(|_| video_path.clone());
                    if self.processed_paths.contains(&canonical) {
                        skipped += 1;
                        continue;
                    }

                    // Create output path
                    let output_path = self.create_output_path(&video_path);

                    // Create job
                    let job = ConversionJob::new(video_path, output_path);
                    self.processed_paths.insert(canonical);
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
            }
            Err(e) => {
                self.set_message(format!("Clipboard: {}", e), true);
            }
        }
    }

    fn create_output_path(&self, input_path: &std::path::Path) -> PathBuf {
        let stem = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");

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

            // Remove from processed paths
            let canonical = job
                .input_path
                .canonicalize()
                .unwrap_or_else(|_| job.input_path.clone());
            self.processed_paths.remove(&canonical);
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
                    job.input_path.clone(),
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
