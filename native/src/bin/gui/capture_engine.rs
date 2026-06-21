use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use crate::types::{CaptureJob, EngineEvent};

pub fn spawn_capture_engine(
    jobs: Vec<CaptureJob>,
    hlae_path: Arc<PathBuf>,
    hl_path: Arc<PathBuf>,
    tx: Sender<EngineEvent>,
) {
    std::thread::Builder::new()
        .name("capture_engine".into())
        .spawn(move || {
            let total = jobs.len();
            if tx.send(EngineEvent::Starting(total)).is_err() {
                return;
            }

            let dod_dir = match hl_path.parent() {
                Some(parent) => parent.join("dod"),
                None => {
                    let _ = tx.send(EngineEvent::Error("Invalid hl.exe path".to_string()));
                    return;
                }
            };

            let dll_path = match hlae_path.parent() {
                Some(parent) => parent.join("AfxHookGoldSrc.dll"),
                None => {
                    let _ = tx.send(EngineEvent::Error("Invalid hlae.exe path".to_string()));
                    return;
                }
            };

            for job in jobs {
                let demo_filename = match job.patched_demo_path.file_name() {
                    Some(name) => name,
                    None => {
                        let _ = tx.send(EngineEvent::Error(format!("Invalid demo path: {:?}", job.patched_demo_path)));
                        continue;
                    }
                };

                let dest_demo_path = dod_dir.join(demo_filename);
                if let Err(e) = std::fs::copy(&job.patched_demo_path, &dest_demo_path) {
                    let _ = tx.send(EngineEvent::Error(format!("Failed to copy demo to game folder: {}", e)));
                    continue;
                }

                let demo_name_no_ext = match std::path::Path::new(demo_filename).file_stem() {
                    Some(stem) => stem.to_string_lossy().to_string(),
                    None => demo_filename.to_string_lossy().to_string(),
                };

                if tx.send(EngineEvent::Launching(demo_name_no_ext.clone())).is_err() {
                    let _ = std::fs::remove_file(&dest_demo_path);
                    return;
                }

                let cmd_line_str = format!("-game dod -insecure -windowed -w 1280 -h 720 +map dod_donner +playdemo {}", demo_name_no_ext);

                let mut cmd = std::process::Command::new(hlae_path.as_ref());
                cmd.args(&[
                    "-customLoader",
                    "-noGui",
                    "-autoStart",
                    "-hookDllPath",
                    &dll_path.to_string_lossy(),
                    "-programPath",
                    &hl_path.to_string_lossy(),
                    "-cmdLine",
                    &cmd_line_str,
                ]);
                cmd.env("SteamAppId", "30");

                let mut child = match cmd.spawn() {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(EngineEvent::Error(format!("Failed to spawn HLAE: {}", e)));
                        let _ = std::fs::remove_file(&dest_demo_path);
                        continue;
                    }
                };

                match child.wait() {
                    Ok(_) => {
                        let _ = tx.send(EngineEvent::Finished(demo_name_no_ext.clone()));
                    }
                    Err(e) => {
                        let _ = tx.send(EngineEvent::Error(format!("Failed to wait for HLAE: {}", e)));
                        let _ = std::fs::remove_file(&dest_demo_path);
                        continue;
                    }
                }

                // TODO: Re-enable temporary demo cleanup once the Phase 7 capture pipeline is fully verified.
                // let _ = std::fs::remove_file(&dest_demo_path);

                // Output Verification
                let expected_wav = job.expected_take_folder.join("sound.wav");
                if expected_wav.exists() && expected_wav.is_file() {
                    let _ = tx.send(EngineEvent::Verified(demo_name_no_ext.clone()));
                } else {
                    let _ = tx.send(EngineEvent::Error(format!(
                        "Verification failed: sound.wav not found in {:?}",
                        job.expected_take_folder
                    )));
                }
            }

            let _ = tx.send(EngineEvent::AllCompleted);
        })
        .unwrap();
}
