use anyhow::{anyhow, Result};
use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext, ContentFormat};
use std::path::PathBuf;

/// Video file extensions we support
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "avi", "mkv", "webm", "m4v", "wmv", "flv", "mpeg", "mpg",
];

/// Check if a URL looks like a video URL
fn is_video_url(url: &str) -> bool {
    // Check scheme
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return false;
    }

    // Extract path without query params
    let path = url.split('?').next().unwrap_or(url);

    // Check extension
    if let Some(ext) = path.rsplit('.').next() {
        if VIDEO_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
            return true;
        }
    }

    // Accept URLs that look like video streaming endpoints
    // (e.g., Twitter/X video CDN, etc.)
    path.contains("/video/") || path.contains("/vid/")
}

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
    copy_files_to_clipboard(&[path.to_path_buf()])
}

/// Copy multiple files to the clipboard
pub fn copy_files_to_clipboard(paths: &[PathBuf]) -> Result<()> {
    let ctx = ClipboardContext::new().map_err(|e| anyhow!("Failed to access clipboard: {}", e))?;

    let path_strs: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    ctx.set_files(path_strs)
        .map_err(|e| anyhow!("Failed to copy files to clipboard: {}", e))?;

    Ok(())
}

/// Get video URL from clipboard text
pub fn get_url_from_clipboard() -> Result<Option<String>> {
    let ctx = ClipboardContext::new().map_err(|e| anyhow!("Failed to access clipboard: {}", e))?;

    // Check if clipboard has text content
    if !ctx.has(ContentFormat::Text) {
        return Ok(None);
    }

    // Get text content
    let contents = ctx
        .get(&[ContentFormat::Text])
        .map_err(|e| anyhow!("Failed to read clipboard: {}", e))?;

    // Find text content
    for content in contents {
        if let ClipboardContent::Text(text) = content {
            let text = text.trim();
            if is_video_url(text) {
                return Ok(Some(text.to_string()));
            }
        }
    }

    Ok(None)
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

    #[test]
    fn test_is_video_url() {
        // Valid video URLs
        assert!(is_video_url("https://example.com/video.mp4"));
        assert!(is_video_url("http://example.com/path/to/video.webm"));
        assert!(is_video_url("https://example.com/video.mp4?query=param"));
        assert!(is_video_url(
            "https://video.twimg.com/vid/avc1/file.mp4?tag=16"
        ));

        // Invalid
        assert!(!is_video_url("ftp://example.com/video.mp4"));
        assert!(!is_video_url("https://example.com/image.png"));
        assert!(!is_video_url("not a url"));
        assert!(!is_video_url("/local/path/video.mp4"));
    }
}
