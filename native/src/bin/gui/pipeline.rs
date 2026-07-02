#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use egui::Context;
use native::run_analyzer_with_progress;
use crate::GuiMessage;
use crate::types::{QueuedStreakExport, CapturePhase};

pub fn analyze_files_async(ctx: Context, tx: mpsc::Sender<GuiMessage>, paths: Vec<PathBuf>) {
    tokio::task::spawn_blocking(move || {
        tx.send(GuiMessage::AnalyzerStart {
            _files: paths.len(),
        })
        .unwrap();

        for (index, demo_path) in paths.iter().enumerate() {
            let tx_clone = tx.clone();
            let ctx_clone = ctx.clone();
            let path_str = demo_path.to_string_lossy().into_owned();
            let start_time = std::time::SystemTime::now();
            let last_update = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

            let progress_cb = move |processed: usize, total: usize| {
                if total > 0 {
                    let elapsed_ms = start_time.elapsed().map(|d| d.as_millis() as u32).unwrap_or(0);
                    let last = last_update.load(std::sync::atomic::Ordering::Relaxed);
                    
                    // Force update at 100% completion or throttle to ~30fps (33ms)
                    if processed == total || elapsed_ms.saturating_sub(last) > 33 {
                        last_update.store(elapsed_ms, std::sync::atomic::Ordering::Relaxed);
                        let elapsed_sec = elapsed_ms as f32 / 1000.0;
                        let progress = processed as f32 / total as f32;
                        let eta_sec = if progress > 0.01 {
                            let total_estimated_sec = elapsed_sec / progress;
                            Some(total_estimated_sec - elapsed_sec)
                        } else {
                            None
                        };

                        let _ = tx_clone.send(GuiMessage::DemoParsingProgress {
                            path: path_str.clone(),
                            progress,
                            elapsed_sec,
                            eta_sec,
                        });
                        ctx_clone.request_repaint();
                    }
                }
            };

            match run_analyzer_with_progress(demo_path, progress_cb) {
                Ok((file_info, analysis)) => {
                    tx.send(GuiMessage::AnalyzerProgress {
                        file_info,
                        _progress: (index + 1, paths.len()),
                        analysis: Box::new(analysis),
                    })
                    .unwrap();
                }
                Err(e) => {
                    tx.send(GuiMessage::AnalyzerError {
                        path: demo_path.to_string_lossy().into_owned(),
                        error: e,
                    })
                    .unwrap();
                }
            }

            ctx.request_repaint();
        }

        tx.send(GuiMessage::Idle).unwrap();
    });
}

pub fn generate_python_queue_sequencer(hlae_path: &str, game_path: &str) -> String {
    format!(
        r#"# Automated Day of Defeat Highlight Recording Sequencer
import os
import sys
import shutil
import subprocess
import time
import json

HLAE_PATH = r"{hlae_path}"
GAME_PATH = r"{game_path}"

def main():
    print("=== Day of Defeat Highlight Recording Sequencer ===")
    
    # Check settings
    hlae = HLAE_PATH
    game = GAME_PATH
    
    if not hlae or not os.path.exists(hlae):
        print(f"Error: HLAE path '{{hlae}}' not found. Please edit this script or configure it in the UI.")
        sys.exit(1)
        
    if not game or not os.path.exists(game):
        print(f"Error: Game (hl.exe) path '{{game}}' not found. Please edit this script or configure it in the UI.")
        sys.exit(1)
        
    game_dir = os.path.dirname(game)
    dod_dir = os.path.join(game_dir, "dod")
    if not os.path.isdir(dod_dir):
        print(f"Error: 'dod' folder not found at '{{dod_dir}}'.")
        sys.exit(1)
        
    queue_json = os.path.join(os.path.dirname(os.path.abspath(__file__)), "capture_queue.json")
    if not os.path.exists(queue_json):
        print(f"Error: capture_queue.json not found.")
        sys.exit(1)
        
    with open(queue_json, "r", encoding="utf-8") as f:
        queue = json.load(f)
        
    print(f"Found {{len(queue)}} highlight(s) to capture.\n")
    
    for idx, item in enumerate(queue):
        src_demo = item["demo_path"]
        player = item["player"]
        kills = item["kills"]
        streak_idx = item["streak_index"]
        
        if not os.path.exists(src_demo):
            print(f"[{{idx+1}}/{{len(queue)}}] Error: Demo file '{{src_demo}}' does not exist. Skipping.")
            continue
            
        demo_name = os.path.basename(src_demo)
        dest_demo_path = os.path.join(dod_dir, demo_name)
        
        print(f"[{{idx+1}}/{{len(queue)}}] Recording streak {{streak_idx}} ({{kills}} kills) by {{player}}")
        print(f"  Copying demo to game folder...")
        shutil.copy2(src_demo, dest_demo_path)
        
        # Strip .dem extension for playdemo
        demo_name_no_ext = os.path.splitext(demo_name)[0]
        
        # Launch HLAE
        hook_dll = os.path.join(os.path.dirname(hlae), "AfxHookGoldSrc.dll")
        cmd_line = f"-game dod -insecure -windowed -w 1280 -h 720 +playdemo {{demo_name_no_ext}}"
        cmd = [
            hlae,
            "-customLoader",
            "-noGui",
            "-autoStart",
            "-hookDllPath", hook_dll,
            "-programPath", game,
            "-cmdLine", cmd_line
        ]
        
        # Inject SteamAppId environment variable
        run_env = os.environ.copy()
        run_env["SteamAppId"] = "30"
        
        print(f"  Running: {{' '.join(cmd)}}")
        process = subprocess.Popen(cmd, env=run_env)
        
        print(f"  Waiting for recording to complete (the game will auto-close when done)...")
        process.wait()
        
        # Clean up
        print(f"  Cleaning up demo file from game folder...")
        try:
            if os.path.exists(dest_demo_path):
                os.remove(dest_demo_path)
        except Exception as e:
            print(f"  Warning: Failed to delete temporary demo '{{dest_demo_path}}': {{e}}")
            
        print(f"  Finished recording streak.\n")
        time.sleep(1.0)
        
    print("=== All recordings completed! ===")

if __name__ == '__main__':
    main()
"#,
        hlae_path = hlae_path,
        game_path = game_path
    )
}

pub fn start_capture_pipeline(
    ctx: Context,
    tx: mpsc::Sender<GuiMessage>,
    enabled_items: Vec<QueuedStreakExport>,
    player_deaths_map: HashMap<String, Vec<f32>>,
    game_path: String,
    hlae_path: String,
    cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    tokio::spawn(async move {
        let game_dir = match std::path::Path::new(&game_path).parent() {
            Some(p) => p.to_path_buf(),
            None => return,
        };
        let dod_dir = game_dir.join("dod");

        for item in enabled_items {
            if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }

            let item_id = item.id.clone();
            let safe_output_name = item.output_name.replace("-", "_");
            if tx.send(GuiMessage::CapturePipelineUpdate {
                item_id: item_id.clone(),
                phase: CapturePhase::Patching,
                sub_status: Some("Preparing folder structure...".to_string()),
                debug_command: None,
                error: None,
            }).is_err() || cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            ctx.request_repaint();

            // Prepare absolute destination folder for HLAE frames
            let capture_dest = dod_dir.join("hlcr_captures").join(&safe_output_name);
            if let Err(e) = tokio::fs::create_dir_all(&capture_dest).await {
                let _ = tx.send(GuiMessage::CapturePipelineUpdate {
                    item_id: item_id.clone(),
                    phase: CapturePhase::Failed,
                    sub_status: None,
                    debug_command: None,
                    error: Some(format!("Failed to create capture folder: {}", e)),
                });
                ctx.request_repaint();
                continue;
            }

            // Get absolute path
            let abs_path = match tokio::fs::canonicalize(&capture_dest).await {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(_) => capture_dest.to_string_lossy().to_string(),
            };
            // Format for HLAE commands
            let mut abs_path_clean = abs_path.replace("\\", "/");
            if abs_path_clean.starts_with("//?/") {
                abs_path_clean = abs_path_clean[4..].to_string();
            }

            // Prepended record command
            let mirv_record_cmd = native::patch::CustomCommand {
                command: format!("mirv_recordmovie_start \"{}\"", abs_path_clean),
                offset: item.record_start_lead,
                relation: native::patch::CommandRelation::Before,
            };

            // Prepend it to the custom commands list
            let mut custom_commands = vec![mirv_record_cmd];
            custom_commands.extend(item.custom_commands.clone());

            // Prepare patch options
            let player_deaths = player_deaths_map.get(&item.id).cloned().unwrap_or_default();
            let options = native::patch::PatchOptions {
                exit_on_finish: item.exit_on_finish,
                init_commands: item.init_commands.lines().map(String::from).collect(),
                custom_commands,
                fast_forward_speed: Some(item.fast_forward_speed),
                hltv_spec_player: item.hltv_spec_player.clone(),
                initial_delay: Some(item.initial_delay),
                pre_record_buffer: Some(item.pre_record_buffer),
                record_start_lead: Some(item.record_start_lead),
                record_stop_trail: Some(item.record_stop_trail),
                post_record_buffer: Some(item.post_record_buffer),
                player_deaths: Some(player_deaths),
            };

            // Read source demo bytes
            if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }

            if tx.send(GuiMessage::CapturePipelineUpdate {
                item_id: item_id.clone(),
                phase: CapturePhase::Patching,
                sub_status: Some("Reading source demo file...".to_string()),
                debug_command: None,
                error: None,
            }).is_err() || cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            ctx.request_repaint();

            let bytes_res = tokio::fs::read(&item.input_path).await;
            let bytes = match bytes_res {
                Ok(b) => b,
                Err(e) => {
                    let _ = tx.send(GuiMessage::CapturePipelineUpdate {
                        item_id: item_id.clone(),
                        phase: CapturePhase::Failed,
                        sub_status: None,
                        debug_command: None,
                        error: Some(format!("Failed to read source demo: {}", e)),
                    });
                    ctx.request_repaint();
                    continue;
                }
            };

            // Call patcher inside spawn_blocking
            if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }

            if tx.send(GuiMessage::CapturePipelineUpdate {
                item_id: item_id.clone(),
                phase: CapturePhase::Patching,
                sub_status: Some("Patching game demo commands...".to_string()),
                debug_command: None,
                error: None,
            }).is_err() || cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            ctx.request_repaint();

            let intervals = vec![(item.start_time, item.stop_time)];
            let patch_res = tokio::task::spawn_blocking(move || {
                native::patch::patch_demo_highlights(&bytes, &intervals, &options)
            }).await;

            let patched_bytes = match patch_res {
                Ok(Ok(b)) => b,
                Ok(Err(e)) => {
                    let _ = tx.send(GuiMessage::CapturePipelineUpdate {
                        item_id: item_id.clone(),
                        phase: CapturePhase::Failed,
                        sub_status: None,
                        debug_command: None,
                        error: Some(format!("Patching failed: {}", e)),
                    });
                    ctx.request_repaint();
                    continue;
                }
                Err(e) => {
                    let _ = tx.send(GuiMessage::CapturePipelineUpdate {
                        item_id: item_id.clone(),
                        phase: CapturePhase::Failed,
                        sub_status: None,
                        debug_command: None,
                        error: Some(format!("Blocking task panicked: {}", e)),
                    });
                    ctx.request_repaint();
                    continue;
                }
            };

            // Write patched demo to game's dod/ directory
            if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }

            if tx.send(GuiMessage::CapturePipelineUpdate {
                item_id: item_id.clone(),
                phase: CapturePhase::Patching,
                sub_status: Some("Copying demo to game folder...".to_string()),
                debug_command: None,
                error: None,
            }).is_err() || cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            ctx.request_repaint();

            let patched_demo_path = dod_dir.join(&safe_output_name);
            if let Err(e) = tokio::fs::write(&patched_demo_path, patched_bytes).await {
                let _ = tx.send(GuiMessage::CapturePipelineUpdate {
                    item_id: item_id.clone(),
                    phase: CapturePhase::Failed,
                    sub_status: None,
                    debug_command: None,
                    error: Some(format!("Failed to write patched demo: {}", e)),
                });
                ctx.request_repaint();
                continue;
            }

            // Diagnostic checks for HLAE and DoD executables existence
            if !std::path::Path::new(&hlae_path).exists() {
                let _ = tx.send(GuiMessage::CapturePipelineUpdate {
                    item_id: item_id.clone(),
                    phase: CapturePhase::Failed,
                    sub_status: None,
                    debug_command: None,
                    error: Some(format!("HLAE executable not found at: {}", hlae_path)),
                });
                ctx.request_repaint();
                continue;
            }
            if !std::path::Path::new(&game_path).exists() {
                let _ = tx.send(GuiMessage::CapturePipelineUpdate {
                    item_id: item_id.clone(),
                    phase: CapturePhase::Failed,
                    sub_status: None,
                    debug_command: None,
                    error: Some(format!("DoD executable (hl.exe) not found at: {}", game_path)),
                });
                ctx.request_repaint();
                continue;
            }

            // Strip .dem extension for playdemo
            let demo_name_no_ext = match std::path::Path::new(&safe_output_name).file_stem() {
                Some(stem) => stem.to_string_lossy().to_string(),
                None => safe_output_name.clone(),
            };

            let hlae_dir = std::path::Path::new(&hlae_path).parent().unwrap();
            let hook_dll = hlae_dir.join("AfxHookGoldSrc.dll");
            let hook_dll_str = hook_dll.to_string_lossy().to_string();

            let args_str = format!("-game dod -insecure -windowed -w 1280 -h 720 +exec dod_tools_helper.cfg +playdemo {}", demo_name_no_ext);
            let mut cmd = tokio::process::Command::new(&hlae_path);
            cmd.kill_on_drop(true);
            cmd.env("SteamAppId", "30");
            cmd.args(&[
                "-customLoader",
                "-noGui",
                "-autoStart",
                "-hookDllPath",
                &hook_dll_str,
                "-programPath",
                &game_path,
                "-cmdLine",
                &args_str,
            ]);

            if let Some(parent_dir) = std::path::Path::new(&game_path).parent() {
                cmd.current_dir(parent_dir);
            }

            let debug_command_str = format!(
                "\"{}\" -customLoader -noGui -autoStart -hookDllPath \"{}\" -programPath \"{}\" -cmdLine \"{}\"",
                hlae_path, hook_dll_str, game_path, args_str
            );

            // Launch HLAE sequential capture
            if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }

            if tx.send(GuiMessage::CapturePipelineUpdate {
                item_id: item_id.clone(),
                phase: CapturePhase::HlaeCapture,
                sub_status: Some("Launching HLAE...".to_string()),
                debug_command: Some(debug_command_str),
                error: None,
            }).is_err() || cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            ctx.request_repaint();

            match cmd.spawn() {
                Ok(mut child) => {
                    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
                    let wait_res = loop {
                        tokio::select! {
                            res = child.wait() => {
                                break res;
                            }
                            _ = interval.tick() => {
                                if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                                    let _ = child.kill().await;
                                    return;
                                }
                                if tx.send(GuiMessage::CapturePipelineUpdate {
                                    item_id: item_id.clone(),
                                    phase: CapturePhase::HlaeCapture,
                                    sub_status: Some("HLAE process active, waiting for completion...".to_string()),
                                    debug_command: None,
                                    error: None,
                                }).is_err() {
                                    let _ = child.kill().await;
                                    return;
                                }
                                ctx.request_repaint();
                            }
                        }
                    };

                    match wait_res {
                        Ok(status) => {
                            if !status.success() {
                                let _ = tx.send(GuiMessage::CapturePipelineUpdate {
                                    item_id: item_id.clone(),
                                    phase: CapturePhase::Failed,
                                    sub_status: None,
                                    debug_command: None,
                                    error: Some(format!("HLAE exited with non-zero status: {}", status)),
                                });
                                ctx.request_repaint();
                                continue;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(GuiMessage::CapturePipelineUpdate {
                                item_id: item_id.clone(),
                                phase: CapturePhase::Failed,
                                sub_status: None,
                                debug_command: None,
                                error: Some(format!("Failed to wait for HLAE: {}", e)),
                            });
                            ctx.request_repaint();
                            continue;
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(GuiMessage::CapturePipelineUpdate {
                        item_id: item_id.clone(),
                        phase: CapturePhase::Failed,
                        sub_status: None,
                        debug_command: None,
                        error: Some(format!("Failed to spawn HLAE: {}", e)),
                    });
                    ctx.request_repaint();
                    continue;
                }
            }

            // Mark this item complete
            if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }

            if tx.send(GuiMessage::CapturePipelineUpdate {
                item_id: item_id.clone(),
                phase: CapturePhase::Complete,
                sub_status: Some("Capture complete!".to_string()),
                debug_command: None,
                error: None,
            }).is_err() || cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            ctx.request_repaint();
        }

        // Notify that the entire queue has completed HLAE capture
        let _ = tx.send(GuiMessage::CaptureStudioFinished);
        ctx.request_repaint();
    });
}
