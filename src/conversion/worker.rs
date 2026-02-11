use crate::config::Settings;
use crate::conversion::ffmpeg::{build_filter_chain, get_loop_arg};
use crate::conversion::job::{ConversionProgress, InputSource, JobStatus, ProgressUpdate};
use anyhow::Result;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

/// Spawn a conversion worker for a single job
/// Returns a cancellation sender that can be used to cancel the job
pub fn spawn_conversion(
    job_id: Uuid,
    input: InputSource,
    output_path: PathBuf,
    settings: Settings,
    progress_tx: mpsc::UnboundedSender<ProgressUpdate>,
) -> oneshot::Sender<()> {
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        run_conversion(job_id, input, output_path, settings, progress_tx, cancel_rx).await;
    });

    cancel_tx
}

async fn run_conversion(
    job_id: Uuid,
    input: InputSource,
    output_path: PathBuf,
    settings: Settings,
    progress_tx: mpsc::UnboundedSender<ProgressUpdate>,
    cancel_rx: oneshot::Receiver<()>,
) {
    // Send initial "converting" status
    let _ = progress_tx.send(ProgressUpdate {
        job_id,
        status: Some(JobStatus::Converting),
        progress: None,
    });

    // Build FFmpeg command
    let filter_chain = build_filter_chain(&settings);
    let loop_arg = get_loop_arg(&settings);

    // Cancellation flag shared between tasks
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_clone = cancelled.clone();

    // Spawn a task to listen for cancellation
    tokio::spawn(async move {
        let _ = cancel_rx.await;
        cancelled_clone.store(true, Ordering::SeqCst);
    });

    // Run FFmpeg in a blocking task
    let output_path_clone = output_path.clone();
    let result = tokio::task::spawn_blocking(move || {
        run_ffmpeg_blocking(
            job_id,
            input,
            output_path_clone,
            filter_chain,
            loop_arg,
            settings,
            progress_tx,
            cancelled,
        )
    })
    .await;

    match result {
        Ok(Ok(())) => {
            // Success handled inside run_ffmpeg_blocking
        }
        Ok(Err(e)) => {
            eprintln!("FFmpeg error: {}", e);
        }
        Err(e) => {
            eprintln!("Task join error: {}", e);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_ffmpeg_blocking(
    job_id: Uuid,
    input: InputSource,
    output_path: PathBuf,
    filter_chain: String,
    loop_arg: String,
    settings: Settings,
    progress_tx: mpsc::UnboundedSender<ProgressUpdate>,
    cancelled: Arc<AtomicBool>,
) -> Result<()> {
    let input_arg = input.ffmpeg_input();

    // First, get the duration using ffprobe
    let duration_secs = get_video_duration(&input_arg);

    // Get ffmpeg path (prefer system for better protocol support)
    let ffmpeg_path = get_ffmpeg_path();

    let mut child = Command::new(&ffmpeg_path)
        .args(["-i", &input_arg])
        .args(["-filter_complex", &filter_chain])
        .args(["-loop", &loop_arg])
        .args(["-progress", "pipe:1"]) // Send progress to stdout
        .args(["-nostats"]) // Don't send stats to stderr
        .args(["-y"]) // Overwrite output
        .arg(output_path.to_string_lossy().as_ref())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let stderr = child.stderr.take().expect("Failed to capture stderr");
    let reader = BufReader::new(stdout);

    // Spawn thread to collect stderr
    let stderr_handle = std::thread::spawn(move || {
        let mut stderr_output = String::new();
        let mut reader = BufReader::new(stderr);
        let _ = std::io::Read::read_to_string(&mut reader, &mut stderr_output);
        stderr_output
    });

    let mut current_frame: u64 = 0;

    // Parse progress output from stdout
    for line in reader.lines() {
        // Check for cancellation
        if cancelled.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = progress_tx.send(ProgressUpdate {
                job_id,
                status: Some(JobStatus::Cancelled),
                progress: None,
            });
            return Ok(());
        }

        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        // Parse progress output (key=value format)
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "frame" => {
                    if let Ok(f) = value.trim().parse::<u64>() {
                        current_frame = f;
                    }
                }
                "out_time_us" => {
                    if let Ok(time_us) = value.trim().parse::<u64>() {
                        // Calculate percentage based on time
                        let percentage = if let Some(dur) = duration_secs {
                            let current_secs = time_us as f64 / 1_000_000.0;
                            ((current_secs / dur) * 100.0).min(99.0) as f32
                        } else {
                            0.0
                        };

                        let _ = progress_tx.send(ProgressUpdate {
                            job_id,
                            status: None,
                            progress: Some(ConversionProgress {
                                frame: current_frame,
                                total_frames: None,
                                percentage,
                                speed: None,
                                output_size: None,
                                split_count: None,
                            }),
                        });
                    }
                }
                "progress" => {
                    if value.trim() == "end" {
                        // Don't send complete here - let the post-processing handle it
                        // This allows us to check file size and potentially split
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    // Wait for process to finish
    let status = child.wait()?;
    let stderr_output = stderr_handle.join().unwrap_or_default();

    // Check final result
    if status.success() && output_path.exists() {
        let output_size = std::fs::metadata(&output_path).map(|m| m.len()).ok();

        // Check if we need to split based on size limit
        let size_limit = settings.size_limit.bytes();
        if let (Some(limit), Some(size)) = (size_limit, output_size) {
            if size > limit {
                // Need to split - calculate number of parts
                // Use 75% of limit as target to account for variable compression
                let parts = ((size as f64) / (limit as f64 * 0.75)).ceil() as usize;
                let duration = get_video_duration(&input.ffmpeg_input());

                if let Some(dur) = duration {
                    // Send splitting status
                    let _ = progress_tx.send(ProgressUpdate {
                        job_id,
                        status: Some(JobStatus::Splitting(parts)),
                        progress: None,
                    });

                    // Delete the oversized file
                    let _ = std::fs::remove_file(&output_path);

                    // Convert in segments
                    let segment_duration = dur / (parts as f64);
                    let stem = output_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("output");
                    let parent = output_path.parent().unwrap_or(std::path::Path::new("."));

                    let mut total_size: u64 = 0;
                    for i in 0..parts {
                        if cancelled.load(Ordering::SeqCst) {
                            let _ = progress_tx.send(ProgressUpdate {
                                job_id,
                                status: Some(JobStatus::Cancelled),
                                progress: None,
                            });
                            return Ok(());
                        }

                        let start_time = i as f64 * segment_duration;
                        let part_output = parent.join(format!("{}_{}.gif", stem, i + 1));

                        let result = run_ffmpeg_segment(
                            &input,
                            &part_output,
                            &filter_chain,
                            &loop_arg,
                            start_time,
                            segment_duration,
                            &ffmpeg_path,
                        );

                        if let Err(e) = result {
                            let _ = progress_tx.send(ProgressUpdate {
                                job_id,
                                status: Some(JobStatus::Failed(format!(
                                    "Split part {} failed: {}",
                                    i + 1,
                                    e
                                ))),
                                progress: None,
                            });
                            return Ok(());
                        }

                        if let Ok(meta) = std::fs::metadata(&part_output) {
                            total_size += meta.len();
                        }

                        // Report progress for splitting
                        let _ = progress_tx.send(ProgressUpdate {
                            job_id,
                            status: None,
                            progress: Some(ConversionProgress {
                                frame: 0,
                                total_frames: None,
                                percentage: ((i + 1) as f32 / parts as f32) * 100.0,
                                speed: None,
                                output_size: Some(total_size),
                                split_count: Some(parts),
                            }),
                        });
                    }

                    // Complete with split info
                    let _ = progress_tx.send(ProgressUpdate {
                        job_id,
                        status: Some(JobStatus::Complete),
                        progress: Some(ConversionProgress {
                            frame: current_frame,
                            total_frames: None,
                            percentage: 100.0,
                            speed: None,
                            output_size: Some(total_size),
                            split_count: Some(parts),
                        }),
                    });
                    return Ok(());
                }
            }
        }

        // No split needed
        let _ = progress_tx.send(ProgressUpdate {
            job_id,
            status: Some(JobStatus::Complete),
            progress: Some(ConversionProgress {
                frame: current_frame,
                total_frames: None,
                percentage: 100.0,
                speed: None,
                output_size,
                split_count: None,
            }),
        });
    } else {
        // Extract last meaningful line from stderr
        let error_msg = stderr_output
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty() && !l.starts_with(' '))
            .unwrap_or("Unknown error")
            .to_string();
        let _ = progress_tx.send(ProgressUpdate {
            job_id,
            status: Some(JobStatus::Failed(error_msg)),
            progress: None,
        });
    }

    Ok(())
}

/// Run FFmpeg for a specific time segment
fn run_ffmpeg_segment(
    input: &InputSource,
    output_path: &std::path::Path,
    filter_chain: &str,
    loop_arg: &str,
    start_time: f64,
    duration: f64,
    ffmpeg_path: &std::path::Path,
) -> Result<()> {
    let input_arg = input.ffmpeg_input();

    let output = Command::new(ffmpeg_path)
        .args(["-ss", &format!("{:.3}", start_time)])
        .args(["-t", &format!("{:.3}", duration)])
        .args(["-i", &input_arg])
        .args(["-filter_complex", filter_chain])
        .args(["-loop", loop_arg])
        .args(["-y"])
        .arg(output_path.to_string_lossy().as_ref())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let error_msg = stderr
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty() && !l.starts_with(' '))
            .unwrap_or("Unknown error");
        return Err(anyhow::anyhow!("{}", error_msg));
    }

    Ok(())
}

/// Get video duration using ffprobe (works with local files and URLs)
fn get_video_duration(input: &str) -> Option<f64> {
    let ffprobe_path = get_ffprobe_path();

    let output = Command::new(&ffprobe_path)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(input)
        .output()
        .ok()?;

    let duration_str = String::from_utf8_lossy(&output.stdout);
    duration_str.trim().parse::<f64>().ok()
}

/// Get FFmpeg path, preferring system installation for better codec/protocol support
fn get_ffmpeg_path() -> std::path::PathBuf {
    // Check for system ffmpeg first (has better protocol support)
    if let Ok(output) = Command::new("which").arg("ffmpeg").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return std::path::PathBuf::from(path);
            }
        }
    }
    // Fall back to sidecar
    ffmpeg_sidecar::paths::ffmpeg_path()
}

/// Get FFprobe path, preferring system installation
fn get_ffprobe_path() -> std::path::PathBuf {
    if let Ok(output) = Command::new("which").arg("ffprobe").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return std::path::PathBuf::from(path);
            }
        }
    }
    ffmpeg_sidecar::ffprobe::ffprobe_path()
}

/// Check if ffmpeg is available
pub fn check_ffmpeg() -> Result<()> {
    // Prefer system ffmpeg - check if it exists and works
    let ffmpeg_path = get_ffmpeg_path();
    let output = Command::new(&ffmpeg_path)
        .arg("-version")
        .output()
        .map_err(|e| anyhow::anyhow!("FFmpeg not found: {}", e))?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("FFmpeg check failed"));
    }

    Ok(())
}
