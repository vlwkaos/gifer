use crate::config::Settings;
use crate::conversion::ffmpeg::{build_filter_chain, get_loop_arg};
use crate::conversion::job::{ConversionProgress, JobStatus, ProgressUpdate};
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
    input_path: PathBuf,
    output_path: PathBuf,
    settings: Settings,
    progress_tx: mpsc::UnboundedSender<ProgressUpdate>,
) -> oneshot::Sender<()> {
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        run_conversion(
            job_id,
            input_path,
            output_path,
            settings,
            progress_tx,
            cancel_rx,
        )
        .await;
    });

    cancel_tx
}

async fn run_conversion(
    job_id: Uuid,
    input_path: PathBuf,
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
    let input_path_clone = input_path.clone();
    let result = tokio::task::spawn_blocking(move || {
        run_ffmpeg_blocking(
            job_id,
            input_path_clone,
            output_path_clone,
            filter_chain,
            loop_arg,
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
    input_path: PathBuf,
    output_path: PathBuf,
    filter_chain: String,
    loop_arg: String,
    progress_tx: mpsc::UnboundedSender<ProgressUpdate>,
    cancelled: Arc<AtomicBool>,
) -> Result<()> {
    // First, get the duration using ffprobe
    let duration_secs = get_video_duration(&input_path);

    // Get ffmpeg path from ffmpeg-sidecar or system
    let ffmpeg_path = ffmpeg_sidecar::paths::ffmpeg_path();

    let mut child = Command::new(&ffmpeg_path)
        .args(["-i", input_path.to_string_lossy().as_ref()])
        .args(["-filter_complex", &filter_chain])
        .args(["-loop", &loop_arg])
        .args(["-progress", "pipe:1"]) // Send progress to stdout
        .args(["-nostats"]) // Don't send stats to stderr
        .args(["-y"]) // Overwrite output
        .arg(output_path.to_string_lossy().as_ref())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let reader = BufReader::new(stdout);

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
                            }),
                        });
                    }
                }
                "progress" => {
                    if value.trim() == "end" {
                        // Conversion complete
                        let output_size = std::fs::metadata(&output_path).map(|m| m.len()).ok();
                        let _ = progress_tx.send(ProgressUpdate {
                            job_id,
                            status: Some(JobStatus::Complete),
                            progress: Some(ConversionProgress {
                                frame: current_frame,
                                total_frames: None,
                                percentage: 100.0,
                                speed: None,
                                output_size,
                            }),
                        });
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }

    // Wait for process to finish
    let status = child.wait()?;

    // Check final result
    if status.success() && output_path.exists() {
        let output_size = std::fs::metadata(&output_path).map(|m| m.len()).ok();
        let _ = progress_tx.send(ProgressUpdate {
            job_id,
            status: Some(JobStatus::Complete),
            progress: Some(ConversionProgress {
                frame: current_frame,
                total_frames: None,
                percentage: 100.0,
                speed: None,
                output_size,
            }),
        });
    } else {
        let _ = progress_tx.send(ProgressUpdate {
            job_id,
            status: Some(JobStatus::Failed(format!(
                "FFmpeg exited with status: {}",
                status
            ))),
            progress: None,
        });
    }

    Ok(())
}

/// Get video duration using ffprobe
fn get_video_duration(path: &PathBuf) -> Option<f64> {
    let ffprobe_path = ffmpeg_sidecar::ffprobe::ffprobe_path();

    let output = Command::new(&ffprobe_path)
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path.to_string_lossy().as_ref())
        .output()
        .ok()?;

    let duration_str = String::from_utf8_lossy(&output.stdout);
    duration_str.trim().parse::<f64>().ok()
}

/// Check if ffmpeg is available
pub fn check_ffmpeg() -> Result<()> {
    ffmpeg_sidecar::download::auto_download()
        .map_err(|e| anyhow::anyhow!("Failed to ensure FFmpeg is available: {}", e))
}
