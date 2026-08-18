#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use tokio::process::Command;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};


use super::config::RenderConfig;
use super::scanner::ClipData;

#[derive(Clone, Debug)]
pub enum RenderUpdate {
    Progress(String, u32),     // (job_id, percentage)
    Speed(String, String),     // (job_id, speed_text)
    Status(String, String),    // (job_id, status_text)
    Finished(String, bool, Option<String>), // (job_id, success, error_log)
}

pub async fn run_render_job(
    job_id: String,
    clip: ClipData,
    config: RenderConfig,
    tx: mpsc::Sender<RenderUpdate>,
    cancel_rx: Arc<AtomicBool>,
) {
    let ffmpeg_path = PathBuf::from(&config.ffmpeg_path);
    let fps = config.fps.to_string();

    let take_folder = PathBuf::from(&clip.take_folder);
    let wav_file = take_folder.join(&clip.wav_file);

    let is_global = config.ffmpeg_path == "ffmpeg";

    // Initial validations
    if !is_global && (!ffmpeg_path.exists() || !ffmpeg_path.is_file()) {
        let _ = tx.send(RenderUpdate::Finished(
            job_id.clone(),
            false,
            Some(format!("FFmpeg not found at: {}", ffmpeg_path.display())),
        ));
        return;
    }
    if !take_folder.exists() || !take_folder.is_dir() {
        let _ = tx.send(RenderUpdate::Finished(
            job_id.clone(),
            false,
            Some(format!("Take folder not found: {}", take_folder.display())),
        ));
        return;
    }
    if !wav_file.exists() {
        let _ = tx.send(RenderUpdate::Finished(
            job_id.clone(),
            false,
            Some(format!("Audio file not found: {}", wav_file.display())),
        ));
        return;
    }

    let clip_type = clip.clip_type.as_str();
    let is_hud = clip_type == "hud_only";

    // ── JIT export-drive routing ──────────────────────────────────────────────
    // Iterate the priority pool and select the first directory that has at
    // least 2 GiB of free space to safely hold the encoded output.
    const EXPORT_THRESHOLD: u64 = 20 * 1024 * 1024 * 1024; // 20 GiB — safe buffer for 300 FPS ProRes
    let mut selected_export_dir: Option<PathBuf> = None;
    for dir in &config.export_directories {
        if crate::sys::disk::get_available_bytes(dir) > EXPORT_THRESHOLD {
            selected_export_dir = Some(dir.clone());
            break;
        }
    }
    if selected_export_dir.is_none() && !config.export_directories.is_empty() {
        let _ = tx.send(RenderUpdate::Finished(
            job_id.clone(),
            false,
            Some(format!(
                "JIT routing failed: all {} export drive(s) have less than 2 GiB free. Render halted.",
                config.export_directories.len()
            )),
        ));
        return;
    }

    let output_folder = selected_export_dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    if let Err(e) = std::fs::create_dir_all(&output_folder) {
        let _ = tx.send(RenderUpdate::Finished(
            job_id.clone(),
            false,
            Some(format!("Failed to create output folder: {}", e)),
        ));
        return;
    }

    let mut codec_args: Vec<&'static str> = Vec::new();
    if is_hud {
        codec_args.extend_from_slice(&["-c:v", "prores_ks", "-profile:v", "4444", "-pix_fmt", "yuva444p10le"]);
    } else {
        match config.target_codec {
            super::config::RenderCodec::NvencH264 => codec_args.extend_from_slice(&["-c:v", "h264_nvenc", "-preset", "p6", "-tune", "hq", "-cq", "15", "-pix_fmt", "yuv420p"]),
            super::config::RenderCodec::H264Software => codec_args.extend_from_slice(&["-c:v", "libx264", "-preset", "fast", "-crf", "16", "-pix_fmt", "yuv420p"]),
            super::config::RenderCodec::ProRes => codec_args.extend_from_slice(&["-c:v", "prores_ks", "-profile:v", "3", "-pix_fmt", "yuv422p10le"]),
            super::config::RenderCodec::DnxHr => codec_args.extend_from_slice(&["-c:v", "dnxhd", "-profile:v", "dnxhr_hq", "-pix_fmt", "yuv422p"]),
        }
    }

    let stream_type = if is_hud { "hud" } else { "all" };
    let wav_stem = std::path::Path::new(&clip.wav_file).file_stem().unwrap_or_default().to_string_lossy();
    let wav_part = if wav_stem.to_lowercase() == "sound" {
        "".to_string()
    } else {
        format!("_{}", wav_stem)
    };
    
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_micros();
    let hash_str = format!("{:04x}", timestamp % 0x10000);

    let take_path = PathBuf::from(&clip.take_folder);
    let take_name = take_path.file_name().unwrap_or_default().to_string_lossy();
    let demo_name = take_path.parent().and_then(|p| p.file_name()).unwrap_or_default().to_string_lossy();

    let final_name = format!("{}_{}{}_{}_{}.mov", demo_name, take_name, wav_part, stream_type, hash_str);
    let out_file = output_folder.join(&final_name);

    // Calculate thread scaling
    let max_concurrent = config.max_concurrent_renders;
    let threads_per_process = match std::thread::available_parallelism() {
        Ok(val) => std::cmp::max(1, val.get() / max_concurrent),
        Err(_) => 2,
    };

    let mut cmd_args = vec!["-y", "-hide_banner"];
    let img_input: String;

    if clip_type == "hud_only" {
        cmd_args.extend(vec![
            // Skip probe/analyze on known BMP sequences; add read-ahead buffering.
            "-probesize", "32", "-analyzeduration", "0", "-thread_queue_size", "512",
            "-framerate", &fps, "-i", "hudcolor/%05d.bmp",
            "-probesize", "32", "-analyzeduration", "0", "-thread_queue_size", "512",
            "-framerate", &fps, "-i", "hudalpha/%05d.bmp",
            "-thread_queue_size", "512",
            "-i", &clip.wav_file,
            "-filter_complex", "[1:v]extractplanes=r[alpha];[0:v][alpha]alphamerge[hud]",
            "-map", "[hud]", "-map", "2:a",
        ]);
    } else {
        img_input = format!("{}/%05d.bmp", clip.img_folder);
        cmd_args.extend(vec![
            // Skip probe/analyze on known BMP sequences; add read-ahead buffering.
            "-probesize", "32", "-analyzeduration", "0", "-thread_queue_size", "512",
            "-framerate", &fps,
            "-i", &img_input,
            "-thread_queue_size", "512",
            "-i", &clip.wav_file,
        ]);
    }

    cmd_args.extend(codec_args);
    let threads_str = threads_per_process.to_string();
    let out_file_str = out_file.to_string_lossy().into_owned();

    cmd_args.extend(vec![
        "-threads", &threads_str,
        // +faststart is only needed for HTTP streaming; omitting it avoids the
        // post-render moov-atom rewrite pass on what can be multi-GB files.
        "-c:a", "pcm_s16le", "-shortest",
        "-progress", "pipe:1", "-loglevel", "error",
        &out_file_str,
    ]);

    let mut cmd = Command::new(ffmpeg_path);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);
    cmd.kill_on_drop(true);
    cmd.args(cmd_args);
    cmd.current_dir(&take_folder);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let _ = tx.send(RenderUpdate::Status(job_id.clone(), "Rendering".to_string()));

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let error_msg = if e.kind() == std::io::ErrorKind::NotFound {
                "FFmpeg not found. Please install FFmpeg or set a custom path in Settings.".to_string()
            } else {
                format!("Failed to spawn FFmpeg process: {}", e)
            };
            let _ = tx.send(RenderUpdate::Finished(
                job_id.clone(),
                false,
                Some(error_msg),
            ));
            return;
        }
    };

    // Spawn stderr reader task to prevent deadlock
    let mut stderr_handle = child.stderr.take().unwrap();
    let stderr_log = Arc::new(Mutex::new(String::new()));
    let stderr_log_clone = Arc::clone(&stderr_log);

    tokio::spawn(async move {
        let mut buf = vec![0u8; 1024];
        while let Ok(n) = stderr_handle.read(&mut buf).await {
            if n == 0 {
                break;
            }
            if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                if let Ok(mut log) = stderr_log_clone.lock() {
                    log.push_str(s);
                }
            }
        }
    });

    // Read stdout for progress updates
    let stdout_handle = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout_handle);
    let mut line = String::new();

    let mut current_fps = "0".to_string();
    let mut current_speed = "0x".to_string();

    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(200));

    loop {
        line.clear();
        tokio::select! {
            _ = interval.tick() => {
                if cancel_rx.load(Ordering::Relaxed) {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    let _ = tx.send(RenderUpdate::Status(job_id.clone(), "Cancelled".to_string()));
                    let _ = tx.send(RenderUpdate::Finished(
                        job_id.clone(),
                        false,
                        Some("Cancelled by user".to_string()),
                    ));
                    return;
                }
            }
            read_res = reader.read_line(&mut line) => {
                match read_res {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if let Some(pos) = trimmed.find('=') {
                            let key = trimmed[..pos].trim();
                            let val = trimmed[pos + 1..].trim();

                            match key {
                                "frame" => {
                                    if let Ok(current_frame) = val.parse::<usize>() {
                                        let total_frames = clip.frame_count;
                                        if total_frames > 0 {
                                            let percent = std::cmp::min(100, (current_frame * 100) / total_frames) as u32;
                                            let _ = tx.send(RenderUpdate::Progress(job_id.clone(), percent));
                                        }
                                    }
                                }
                                "fps" => {
                                    current_fps = val.to_string();
                                    let _ = tx.send(RenderUpdate::Speed(
                                        job_id.clone(),
                                        format!("{} fps ({})", current_fps, current_speed),
                                    ));
                                }
                                "speed" => {
                                    current_speed = val.to_string();
                                    let _ = tx.send(RenderUpdate::Speed(
                                        job_id.clone(),
                                        format!("{} fps ({})", current_fps, current_speed),
                                    ));
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }

    // Wait for the process to exit with cancellation check
    let exit_status = loop {
        tokio::select! {
            res = child.wait() => {
                break res;
            }
            _ = interval.tick() => {
                if cancel_rx.load(Ordering::Relaxed) {
                    let _ = child.kill().await;
                    let _ = tx.send(RenderUpdate::Status(job_id.clone(), "Cancelled".to_string()));
                    let _ = tx.send(RenderUpdate::Finished(
                        job_id.clone(),
                        false,
                        Some("Cancelled by user".to_string()),
                    ));
                    return;
                }
            }
        }
    };

    let err_log = if let Ok(log) = stderr_log.lock() {
        if log.trim().is_empty() {
            None
        } else {
            Some(log.clone())
        }
    } else {
        None
    };

    match exit_status {
        Ok(status) if status.success() => {
            let _ = tx.send(RenderUpdate::Status(job_id.clone(), "Finished".to_string()));
            let _ = tx.send(RenderUpdate::Progress(job_id.clone(), 100));
            let _ = tx.send(RenderUpdate::Finished(job_id, true, None));
        }
        Ok(status) => {
            let exit_code = status.code().map(|c| c.to_string()).unwrap_or_else(|| "Unknown".to_string());
            let error_msg = format!("FFmpeg failed with exit code: {}. Log: {}", exit_code, err_log.as_deref().unwrap_or(""));
            let _ = tx.send(RenderUpdate::Status(job_id.clone(), "Error".to_string()));
            let _ = tx.send(RenderUpdate::Finished(job_id, false, Some(error_msg)));
        }
        Err(e) => {
            let error_msg = format!("Failed to wait for FFmpeg: {}", e);
            let _ = tx.send(RenderUpdate::Status(job_id.clone(), "Error".to_string()));
            let _ = tx.send(RenderUpdate::Finished(job_id, false, Some(error_msg)));
        }
    }
}

/// Opaque wake-lock guard for a render batch's duration. Wrapped so
/// `desktop-studio/src-tauri` never needs `keepawake` as a direct
/// dependency — `native` already depends on it (see
/// `capture_engine.rs::CaptureCleanupGuard`'s own wake lock, the
/// capture-side equivalent of this).
pub struct RenderWakeLock(#[allow(dead_code)] keepawake::KeepAwake);

pub fn hold_render_wake_lock() -> Option<RenderWakeLock> {
    keepawake::Builder::default()
        .display(false)
        .idle(true)
        .sleep(true)
        .create()
        .ok()
        .map(RenderWakeLock)
}

#[allow(dead_code)]
fn get_unique_filename(output_dir: &Path, base_name: &str, ext: &str) -> String {
    let mut counter = 1;
    let mut final_name = format!("{}{}", base_name, ext);
    while output_dir.join(&final_name).exists() {
        final_name = format!("{}_{}{}", base_name, counter, ext);
        counter += 1;
    }
    final_name
}
