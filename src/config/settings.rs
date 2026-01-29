use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Output directory suggestions in order of preference
pub const OUTPUT_DIR_SUGGESTIONS: &[&str] = &[
    "~/gifs",
    "~/Pictures/gifs",
    "~/Movies/gifs",
    "~/Desktop/gifs",
];

/// Scale presets for easy selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Scale {
    /// Keep original size
    Original,
    /// Scale to 75% of original
    Percent75,
    /// Scale to 50% of original
    #[default]
    Percent50,
    /// Scale to 33% of original
    Percent33,
    /// Scale to 25% of original
    Percent25,
    /// Scale to 720p height
    H720,
    /// Scale to 480p height
    H480,
    /// Scale to 360p height
    H360,
    /// Scale to 240p height
    H240,
}

impl Scale {
    pub fn as_str(&self) -> &'static str {
        match self {
            Scale::Original => "Original",
            Scale::Percent75 => "75%",
            Scale::Percent50 => "50%",
            Scale::Percent33 => "33%",
            Scale::Percent25 => "25%",
            Scale::H720 => "720p",
            Scale::H480 => "480p",
            Scale::H360 => "360p",
            Scale::H240 => "240p",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Scale::Original => Scale::Percent75,
            Scale::Percent75 => Scale::Percent50,
            Scale::Percent50 => Scale::Percent33,
            Scale::Percent33 => Scale::Percent25,
            Scale::Percent25 => Scale::H720,
            Scale::H720 => Scale::H480,
            Scale::H480 => Scale::H360,
            Scale::H360 => Scale::H240,
            Scale::H240 => Scale::Original,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Scale::Original => Scale::H240,
            Scale::Percent75 => Scale::Original,
            Scale::Percent50 => Scale::Percent75,
            Scale::Percent33 => Scale::Percent50,
            Scale::Percent25 => Scale::Percent33,
            Scale::H720 => Scale::Percent25,
            Scale::H480 => Scale::H720,
            Scale::H360 => Scale::H480,
            Scale::H240 => Scale::H360,
        }
    }

    /// Get the FFmpeg scale filter string
    pub fn ffmpeg_scale(&self) -> Option<String> {
        match self {
            Scale::Original => None, // No scaling
            Scale::Percent75 => Some("scale=iw*0.75:ih*0.75:flags=lanczos".to_string()),
            Scale::Percent50 => Some("scale=iw*0.5:ih*0.5:flags=lanczos".to_string()),
            Scale::Percent33 => Some("scale=iw*0.33:ih*0.33:flags=lanczos".to_string()),
            Scale::Percent25 => Some("scale=iw*0.25:ih*0.25:flags=lanczos".to_string()),
            Scale::H720 => Some("scale=-1:720:flags=lanczos".to_string()),
            Scale::H480 => Some("scale=-1:480:flags=lanczos".to_string()),
            Scale::H360 => Some("scale=-1:360:flags=lanczos".to_string()),
            Scale::H240 => Some("scale=-1:240:flags=lanczos".to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Quality {
    Low,
    #[default]
    Medium,
    High,
}

impl Quality {
    pub fn as_str(&self) -> &'static str {
        match self {
            Quality::Low => "Low",
            Quality::Medium => "Medium",
            Quality::High => "High",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Quality::Low => Quality::Medium,
            Quality::Medium => Quality::High,
            Quality::High => Quality::Low,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Quality::Low => Quality::High,
            Quality::Medium => Quality::Low,
            Quality::High => Quality::Medium,
        }
    }

    /// Get dithering option for ffmpeg paletteuse filter
    pub fn dither_option(&self) -> &'static str {
        match self {
            Quality::Low => "bayer:bayer_scale=5",
            Quality::Medium => "sierra2_4a",
            Quality::High => "floyd_steinberg",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LoopCount {
    #[default]
    Infinite,
    Once,
    Count(u32),
}

impl LoopCount {
    pub fn as_str(&self) -> String {
        match self {
            LoopCount::Infinite => "Infinite".to_string(),
            LoopCount::Once => "Once".to_string(),
            LoopCount::Count(n) => format!("{n}x"),
        }
    }

    pub fn next(&self) -> Self {
        match self {
            LoopCount::Infinite => LoopCount::Once,
            LoopCount::Once => LoopCount::Count(2),
            LoopCount::Count(n) if *n >= 10 => LoopCount::Infinite,
            LoopCount::Count(n) => LoopCount::Count(n + 1),
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            LoopCount::Infinite => LoopCount::Count(10),
            LoopCount::Once => LoopCount::Infinite,
            LoopCount::Count(2) => LoopCount::Once,
            LoopCount::Count(n) => LoopCount::Count(n - 1),
        }
    }

    /// Get the -loop value for ffmpeg (0 = infinite, -1 = no loop, n = loop n times)
    pub fn ffmpeg_loop_value(&self) -> i32 {
        match self {
            LoopCount::Infinite => 0,
            LoopCount::Once => -1,
            LoopCount::Count(n) => *n as i32,
        }
    }
}

/// FPS presets for easy selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FpsPreset {
    Fps5,
    #[default]
    Fps10,
    Fps15,
    Fps20,
    Fps24,
    Fps30,
}

impl FpsPreset {
    pub fn as_str(&self) -> &'static str {
        match self {
            FpsPreset::Fps5 => "5",
            FpsPreset::Fps10 => "10",
            FpsPreset::Fps15 => "15",
            FpsPreset::Fps20 => "20",
            FpsPreset::Fps24 => "24",
            FpsPreset::Fps30 => "30",
        }
    }

    pub fn value(&self) -> u8 {
        match self {
            FpsPreset::Fps5 => 5,
            FpsPreset::Fps10 => 10,
            FpsPreset::Fps15 => 15,
            FpsPreset::Fps20 => 20,
            FpsPreset::Fps24 => 24,
            FpsPreset::Fps30 => 30,
        }
    }

    pub fn next(&self) -> Self {
        match self {
            FpsPreset::Fps5 => FpsPreset::Fps10,
            FpsPreset::Fps10 => FpsPreset::Fps15,
            FpsPreset::Fps15 => FpsPreset::Fps20,
            FpsPreset::Fps20 => FpsPreset::Fps24,
            FpsPreset::Fps24 => FpsPreset::Fps30,
            FpsPreset::Fps30 => FpsPreset::Fps5,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            FpsPreset::Fps5 => FpsPreset::Fps30,
            FpsPreset::Fps10 => FpsPreset::Fps5,
            FpsPreset::Fps15 => FpsPreset::Fps10,
            FpsPreset::Fps20 => FpsPreset::Fps15,
            FpsPreset::Fps24 => FpsPreset::Fps20,
            FpsPreset::Fps30 => FpsPreset::Fps24,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Scale preset
    pub scale: Scale,
    /// Frames per second preset
    pub fps: FpsPreset,
    /// Quality preset
    pub quality: Quality,
    /// Loop behavior
    pub loop_count: LoopCount,
    /// Output directory for GIFs
    pub output_dir: PathBuf,
    /// Maximum concurrent conversion jobs
    pub max_concurrent: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scale: Scale::Percent50,
            fps: FpsPreset::Fps10,
            quality: Quality::Medium,
            loop_count: LoopCount::Infinite,
            output_dir: expand_tilde("~/gifs"),
            max_concurrent: 3,
        }
    }
}

impl Settings {
    /// Get the config file path
    fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
        Ok(config_dir.join("gifer").join("config.toml"))
    }

    /// Load settings from config file, or return defaults if not found
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let settings: Settings = toml::from_str(&content)?;
            Ok(settings)
        } else {
            Ok(Settings::default())
        }
    }

    /// Save settings to config file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Cycle to next output directory suggestion
    pub fn next_output_dir(&mut self) {
        let current = self.output_dir.display().to_string();
        let current_collapsed = collapse_tilde(&current);

        let mut found = false;
        for (i, suggestion) in OUTPUT_DIR_SUGGESTIONS.iter().enumerate() {
            if *suggestion == current_collapsed {
                let next_idx = (i + 1) % OUTPUT_DIR_SUGGESTIONS.len();
                self.output_dir = expand_tilde(OUTPUT_DIR_SUGGESTIONS[next_idx]);
                found = true;
                break;
            }
        }
        if !found {
            self.output_dir = expand_tilde(OUTPUT_DIR_SUGGESTIONS[0]);
        }
    }

    /// Cycle to previous output directory suggestion
    pub fn prev_output_dir(&mut self) {
        let current = self.output_dir.display().to_string();
        let current_collapsed = collapse_tilde(&current);

        let mut found = false;
        for (i, suggestion) in OUTPUT_DIR_SUGGESTIONS.iter().enumerate() {
            if *suggestion == current_collapsed {
                let prev_idx = if i == 0 {
                    OUTPUT_DIR_SUGGESTIONS.len() - 1
                } else {
                    i - 1
                };
                self.output_dir = expand_tilde(OUTPUT_DIR_SUGGESTIONS[prev_idx]);
                found = true;
                break;
            }
        }
        if !found {
            self.output_dir = expand_tilde(OUTPUT_DIR_SUGGESTIONS[0]);
        }
    }

    /// Get display string for output directory (collapsed with ~)
    pub fn output_dir_display(&self) -> String {
        collapse_tilde(&self.output_dir.display().to_string())
    }

    /// Ensure output directory exists
    pub fn ensure_output_dir(&self) -> Result<()> {
        if !self.output_dir.exists() {
            std::fs::create_dir_all(&self.output_dir)?;
        }
        Ok(())
    }
}

/// Expand ~ to home directory
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

/// Collapse home directory to ~
pub fn collapse_tilde(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();
        if path.starts_with(&home_str) {
            return path.replacen(&home_str, "~", 1);
        }
    }
    path.to_string()
}
