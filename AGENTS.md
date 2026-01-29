# Gifer - Rust TUI Video-to-GIF Converter

## Build & Run

```bash
cargo build              # Build debug
cargo build --release    # Build release
cargo run                # Run app
cargo clippy             # Lint
cargo fmt                # Format
```

## Project Overview

TUI application for converting videos to GIFs. Users paste video files from clipboard, configure conversion settings (dimensions, fps, quality, loop), monitor progress, and copy output paths.

## Dependencies

- **ratatui** + **crossterm**: TUI framework
- **clipboard-rs**: File path clipboard (NOT arboard - doesn't support files)
- **ffmpeg-sidecar**: FFmpeg wrapper with progress events
- **tokio**: Async runtime for conversion workers
- **serde** + **toml** + **dirs**: Config persistence

## Code Style

- Rust 2021 edition
- Use `anyhow::Result` for error handling
- Async functions for I/O and conversion workers
- Channel-based communication between async workers and UI

## Architecture

- **main.rs**: Terminal setup, main loop with `tokio::select!`
- **app.rs**: Central state, event dispatch
- **ui/**: Ratatui widgets and layout
- **config/**: Settings struct with TOML persistence
- **conversion/**: FFmpeg job management and progress tracking
- **clipboard.rs**: File path read/write
- **event.rs**: Crossterm event polling

## Key Patterns

1. **Async Progress**: Conversion workers send `ProgressUpdate` via mpsc channel
2. **Focus System**: `FocusedSection` enum controls which panel receives input
3. **Settings Persistence**: Auto-save to `~/.config/gifer/config.toml` on quit
4. **FFmpeg Quality**: Two-pass palette generation for high-quality GIFs
