use anyhow::{anyhow, Result};
use clipboard_rs::{Clipboard, ClipboardContext};
use std::path::PathBuf;

/// Video file extensions we support
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "avi", "mkv", "webm", "m4v", "wmv", "flv", "mpeg", "mpg",
];

/// Check if a path has a video extension
fn is_video_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| VIDEO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Get video file paths from clipboard
/// Returns all video files found in clipboard
pub fn get_videos_from_clipboard() -> Result<Vec<PathBuf>> {
    let ctx = ClipboardContext::new().map_err(|e| anyhow!("Failed to access clipboard: {}", e))?;

    // Try to get file paths from clipboard
    let files = ctx
        .get_files()
        .map_err(|e| anyhow!("Failed to read files from clipboard: {}", e))?;

    if files.is_empty() {
        return Err(anyhow!("No files in clipboard"));
    }

    // Filter for video files
    let videos: Vec<PathBuf> = files
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| is_video_file(p))
        .collect();

    if videos.is_empty() {
        return Err(anyhow!("No video files in clipboard"));
    }

    Ok(videos)
}

/// Copy a file to the clipboard (so it can be pasted in Finder, etc.)
pub fn copy_file_to_clipboard(path: &std::path::Path) -> Result<()> {
    let ctx = ClipboardContext::new().map_err(|e| anyhow!("Failed to access clipboard: {}", e))?;

    let path_str = path.to_string_lossy().to_string();
    ctx.set_files(vec![path_str])
        .map_err(|e| anyhow!("Failed to copy file to clipboard: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_video_file() {
        assert!(is_video_file(&PathBuf::from("test.mp4")));
        assert!(is_video_file(&PathBuf::from("test.MOV")));
        assert!(is_video_file(&PathBuf::from("/path/to/video.mkv")));
        assert!(!is_video_file(&PathBuf::from("test.txt")));
        assert!(!is_video_file(&PathBuf::from("test.gif")));
        assert!(!is_video_file(&PathBuf::from("test")));
    }
}
