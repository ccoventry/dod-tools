use dem::open_demo_from_bytes;
use dem::types::{Frame, FrameData, ConsoleCommand, ByteString};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CustomCommand {
    pub command: String,
    pub offset: f32,
    pub relation: CommandRelation,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum CommandRelation {
    Before,
    After,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PatchOptions {
    pub exit_on_finish: bool,
    pub init_commands: Vec<String>,
    pub custom_commands: Vec<CustomCommand>,
    pub fast_forward_speed: Option<f32>,
    pub hltv_spec_player: Option<String>,
    pub initial_delay: Option<f32>,
    pub pre_record_buffer: Option<f32>,
    pub record_start_lead: Option<f32>,
    pub record_stop_trail: Option<f32>,
    pub post_record_buffer: Option<f32>,
    pub player_deaths: Option<Vec<f32>>,
}

pub fn patch_demo_highlights(
    demo_bytes: &[u8],
    intervals: &[(f32, f32)], // (start_time, stop_time)
    options: &PatchOptions,
) -> Result<Vec<u8>, String> {
    let mut demo = open_demo_from_bytes(demo_bytes)
        .map_err(|e| format!("Could not parse demo file: {}", e))?;

    // Find the Playback entry
    let mut entry = demo.directory.entries.iter_mut().find(|e| e.type_ == 1);
    if entry.is_none() {
        entry = demo.directory.entries.iter_mut().find(|e| {
            e.description
                .to_str()
                .unwrap_or("")
                .contains("Playback")
        });
    }
    let entry = entry.ok_or_else(|| "Could not find Playback entry in demo directory".to_string())?;

    let start_of_playback = entry.frames.first().map(|f| f.time).unwrap_or(0.0);

    // 1. Inject init commands
    for cmd in &options.init_commands {
        entry.frames.push(Frame {
            time: start_of_playback,
            frame: 0,
            frame_data: FrameData::ConsoleCommand(ConsoleCommand {
                command: ByteString::from(cmd.as_str()),
            }),
        });
    }

    // Sort intervals
    let mut sorted_intervals = intervals.to_vec();
    sorted_intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let ff_speed = options.fast_forward_speed.unwrap_or(0.2);
    let initial_delay = options.initial_delay.unwrap_or(3.0);
    let pre_record_buffer = options.pre_record_buffer.unwrap_or(6.0);
    let record_start_lead = options.record_start_lead.unwrap_or(2.0);
    let record_stop_trail = options.record_stop_trail.unwrap_or(2.0);
    let post_record_buffer = options.post_record_buffer.unwrap_or(4.0);

    let ff_cmd = format!("host_framerate {}", ff_speed);
    let overlap_threshold = record_start_lead + record_stop_trail;

    // Merge close or overlapping intervals (where recording periods would overlap)
    let mut merged_intervals: Vec<(f32, f32)> = Vec::new();
    for interval in sorted_intervals {
        if let Some(last) = merged_intervals.last_mut() {
            if interval.0 <= last.1 + overlap_threshold {
                last.1 = last.1.max(interval.1);
            } else {
                merged_intervals.push(interval);
            }
        } else {
            merged_intervals.push(interval);
        }
    }

    // Fast forward initially if first highlight is a bit in
    if let Some(&(first_start, _)) = merged_intervals.first() {
        if first_start > initial_delay + pre_record_buffer {
            entry.frames.push(Frame {
                time: start_of_playback + initial_delay,
                frame: 0,
                frame_data: FrameData::ConsoleCommand(ConsoleCommand {
                    command: ByteString::from(ff_cmd.as_str()),
                }),
            });
        }
    }

    for &(start_time, stop_time) in &merged_intervals {
        // Check if player died within 5.0 seconds before streak start
        let mut delay_deathnotice = false;
        if let Some(ref deaths) = options.player_deaths {
            for &death_time in deaths {
                if death_time >= start_time - 5.0 && death_time < start_time {
                    delay_deathnotice = true;
                    break;
                }
            }
        }

        // a. Pre-Streak Normalization: drop speed and run stop_commands
        let norm_time = (start_time - pre_record_buffer).max(start_of_playback + initial_delay + 0.1);
        
        entry.frames.push(Frame {
            time: norm_time,
            frame: 0,
            frame_data: FrameData::ConsoleCommand(ConsoleCommand {
                command: ByteString::from("host_framerate 0"),
            }),
        });

        if let Some(ref player_name) = options.hltv_spec_player {
            let spec_cmd = format!("spec_player \"{}\"", player_name);
            entry.frames.push(Frame {
                time: norm_time + 0.005,
                frame: 0,
                frame_data: FrameData::ConsoleCommand(ConsoleCommand {
                    command: ByteString::from(spec_cmd.as_str()),
                }),
            });
            entry.frames.push(Frame {
                time: norm_time + 0.006,
                frame: 0,
                frame_data: FrameData::ConsoleCommand(ConsoleCommand {
                    command: ByteString::from("spec_mode 4"),
                }),
            });
        }

        for item in &options.custom_commands {
            let mut time = match item.relation {
                CommandRelation::Before => start_time - item.offset,
                CommandRelation::After => stop_time + item.offset,
            };

            // Apply death notice delay scenario
            if delay_deathnotice 
                && item.relation == CommandRelation::Before
                && item.command.to_lowercase().contains("hud_deathnotice_time")
                && time < start_time - 0.01 
            {
                time = start_time - 0.01;
            }

            let final_time = time.max(start_of_playback + initial_delay + 0.1);

            entry.frames.push(Frame {
                time: final_time,
                frame: 0,
                frame_data: FrameData::ConsoleCommand(ConsoleCommand {
                    command: ByteString::from(item.command.as_str()),
                }),
            });
        }

        // b. Record Start: mirv_recordmovie_start
        let record_start_time = (start_time - record_start_lead).max(start_of_playback + initial_delay + 0.2);
        entry.frames.push(Frame {
            time: record_start_time,
            frame: 0,
            frame_data: FrameData::ConsoleCommand(ConsoleCommand {
                command: ByteString::from("mirv_recordmovie_start"),
            }),
        });

        // c. Record Stop: mirv_recordmovie_stop
        let record_stop_time = stop_time + record_stop_trail;
        entry.frames.push(Frame {
            time: record_stop_time,
            frame: 0,
            frame_data: FrameData::ConsoleCommand(ConsoleCommand {
                command: ByteString::from("mirv_recordmovie_stop"),
            }),
        });

        // d. Post-Streak Fast-Forward: resume host_framerate <ff_speed>
        let resume_ff_time = stop_time + post_record_buffer;
        entry.frames.push(Frame {
            time: resume_ff_time,
            frame: 0,
            frame_data: FrameData::ConsoleCommand(ConsoleCommand {
                command: ByteString::from(ff_cmd.as_str()),
            }),
        });
    }

    // e. Exit On Finish: quit command after the last stop record
    if options.exit_on_finish {
        if let Some(&(_, last_stop)) = merged_intervals.last() {
            let quit_time = last_stop + post_record_buffer + 0.5;
            entry.frames.push(Frame {
                time: quit_time,
                frame: 0,
                frame_data: FrameData::ConsoleCommand(ConsoleCommand {
                    command: ByteString::from("quit"),
                }),
            });
        }
    }

    // Sort frames by time
    entry.frames.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));

    // Re-index frame numbers sequentially
    for (idx, frame) in entry.frames.iter_mut().enumerate() {
        frame.frame = idx as i32;
    }

    // Update entry header values if necessary
    entry.frame_count = entry.frames.len() as i32;

    Ok(demo.write_to_bytes())
}

#[derive(Debug, PartialEq)]
enum PatcherState {
    Seeking,
    PreRoll,
    Recording,
}

fn write_console_cmd(writer: &mut std::io::BufWriter<std::fs::File>, time: f32, tick: i32, cmd: &str) -> std::io::Result<i32> {
    use std::io::Write;
    writer.write_all(&[3_u8])?;
    writer.write_all(&time.to_le_bytes())?;
    writer.write_all(&tick.to_le_bytes())?;
    let mut payload = [0_u8; 64];
    let cmd_bytes = cmd.as_bytes();
    let len = cmd_bytes.len().min(63);
    payload[..len].copy_from_slice(&cmd_bytes[..len]);
    writer.write_all(&payload)?;
    Ok(73)
}

pub struct StreamPatcher {
    pub input_path: std::path::PathBuf,
    pub output_path: std::path::PathBuf,
}

impl StreamPatcher {
    pub fn new(input_path: impl AsRef<std::path::Path>, output_path: impl AsRef<std::path::Path>) -> Self {
        Self {
            input_path: input_path.as_ref().to_path_buf(),
            output_path: output_path.as_ref().to_path_buf(),
        }
    }

    pub fn patch(&self, job: &PatchJob, config: &PatcherConfig, cancel_token: &Arc<AtomicBool>) -> Result<(), std::io::Error> {
        use std::io::{BufReader, BufWriter, Read, Write, Seek, SeekFrom};

        let input_file = std::fs::File::open(&job.source_demo)?;
        let output_file = std::fs::File::create(&job.output_demo)?;

        let mut reader = BufReader::with_capacity(256 * 1024, input_file);
        let mut writer = BufWriter::with_capacity(256 * 1024, output_file);

        // Step 2: Parse and Clone the Header
        let mut header = [0u8; 544];
        reader.read_exact(&mut header)?;

        // Extract directory offset (i32 is at offset 540..544)
        let original_offset = i32::from_le_bytes(header[540..544].try_into().unwrap());

        writer.write_all(&header)?;

        let mut bytes_injected: i32 = 0;
        let mut state = PatcherState::Seeking;
        let mut streak_idx = 0;
        let mut is_first_frame = true;

        // Step 3: Zero-Allocation Copy Loop
        let mut payload_buf = Vec::new();

        loop {
            if cancel_token.load(Ordering::Relaxed) {
                return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Cancelled by user"));
            }

            let mut frame_hdr = [0u8; 9];
            if let Err(e) = reader.read_exact(&mut frame_hdr) {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    break;
                }
                return Err(e);
            }

            let type_byte = frame_hdr[0];
            let time = f32::from_le_bytes(frame_hdr[1..5].try_into().unwrap());
            let tick = i32::from_le_bytes(frame_hdr[5..9].try_into().unwrap());

            if is_first_frame {
                bytes_injected += write_console_cmd(&mut writer, time, tick, "host_framerate 0")?;
                is_first_frame = false;
            }

            if let Some(current_streak) = job.streaks.get(streak_idx) {
                match state {
                    PatcherState::Seeking => {
                        if tick >= current_streak.start_tick - config.pre_roll_ticks {
                            bytes_injected += write_console_cmd(&mut writer, time, tick, &format!("host_framerate {}", config.capture_fps))?;
                            bytes_injected += write_console_cmd(&mut writer, time, tick, "r_decals 0")?;
                            bytes_injected += write_console_cmd(&mut writer, time, tick, "r_decals 5555")?;
                            state = PatcherState::PreRoll;
                        }
                    }
                    PatcherState::PreRoll => {
                        if tick >= current_streak.start_tick {
                            bytes_injected += write_console_cmd(&mut writer, time, tick, &format!("startmovie cap_ {}", config.capture_fps))?;
                            state = PatcherState::Recording;
                        }
                    }
                    PatcherState::Recording => {
                        if tick >= current_streak.end_tick + config.post_roll_ticks {
                            bytes_injected += write_console_cmd(&mut writer, time, tick, "endmovie")?;
                            bytes_injected += write_console_cmd(&mut writer, time, tick, "host_framerate 0")?;
                            state = PatcherState::Seeking;
                            streak_idx += 1;
                        }
                    }
                }
            }

            writer.write_all(&frame_hdr)?;

            // Determine payload size to read/write
            match type_byte {
                1 => {
                    // Demo End indicator (sometimes treated as type 1) or break the loop if required
                    // But in GoldSrc, type 1 is NetworkMessage (Normal). Wait, let's follow user rules:
                    // "When the loop hits Type 5 (Next Section) or Type 1 (Demo End), break the loop."
                    break;
                }
                2 => {
                    // DemoStart (0 bytes)
                }
                3 => {
                    // ConsoleCommand (64 bytes)
                    payload_buf.resize(64, 0);
                    reader.read_exact(&mut payload_buf)?;
                    writer.write_all(&payload_buf)?;
                }
                4 => {
                    // ClientData (32 bytes)
                    payload_buf.resize(32, 0);
                    reader.read_exact(&mut payload_buf)?;
                    writer.write_all(&payload_buf)?;
                }
                5 => {
                    // NextSection (0 bytes)
                    break;
                }
                6 => {
                    // Event (84 bytes)
                    payload_buf.resize(84, 0);
                    reader.read_exact(&mut payload_buf)?;
                    writer.write_all(&payload_buf)?;
                }
                7 => {
                    // WeaponAnimation (8 bytes)
                    payload_buf.resize(8, 0);
                    reader.read_exact(&mut payload_buf)?;
                    writer.write_all(&payload_buf)?;
                }
                8 => {
                    // Sound (24 bytes + sample_length)
                    let mut prefix = [0u8; 8];
                    reader.read_exact(&mut prefix)?;
                    writer.write_all(&prefix)?;
                    let sample_length = u32::from_le_bytes(prefix[4..8].try_into().unwrap()) as usize;

                    payload_buf.resize(sample_length + 16, 0);
                    reader.read_exact(&mut payload_buf)?;
                    writer.write_all(&payload_buf)?;
                }
                9 => {
                    // DemoBuffer (4 bytes + buffer_length)
                    let mut prefix = [0u8; 4];
                    reader.read_exact(&mut prefix)?;
                    writer.write_all(&prefix)?;
                    let buffer_length = u32::from_le_bytes(prefix[0..4].try_into().unwrap()) as usize;

                    payload_buf.resize(buffer_length, 0);
                    reader.read_exact(&mut payload_buf)?;
                    writer.write_all(&buffer_length.to_le_bytes())?;
                    writer.write_all(&payload_buf)?;
                }
                _ => {
                    // NetworkMessage (464 bytes + message_length)
                    let mut info_buf = [0u8; 460];
                    reader.read_exact(&mut info_buf)?;
                    writer.write_all(&info_buf)?;

                    let mut len_buf = [0u8; 4];
                    reader.read_exact(&mut len_buf)?;
                    writer.write_all(&len_buf)?;
                    let msg_len = u32::from_le_bytes(len_buf) as usize;

                    payload_buf.resize(msg_len, 0);
                    reader.read_exact(&mut payload_buf)?;
                    writer.write_all(&payload_buf)?;
                }
            }
        }

        // [STEP 4] Directory Offset Rewrite (EOF Handling)
        // 4b: Copy the remaining directory entries from the input to the output.
        std::io::copy(&mut reader, &mut writer)?;

        // 4c: Flush BufWriter.
        writer.flush()?;

        // 4d: Seek back to the exact byte position of the directory_offset in the output file's header (usually offset 540).
        let mut out_file = writer.into_inner()?;
        out_file.seek(SeekFrom::Start(540))?;

        // 4e: Calculate new offset.
        let new_offset = original_offset + bytes_injected;

        // 4f: Write the new_offset (as Little Endian bytes).
        out_file.write_all(&new_offset.to_le_bytes())?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CaptureStreak {
    pub start_tick: i32,
    pub end_tick: i32,
    pub source_demo: String,
}

#[derive(Debug, Clone)]
pub struct PatchJob {
    pub source_demo: String,
    pub output_demo: std::path::PathBuf,
    pub streaks: Vec<CaptureStreak>,
}

#[derive(Debug, Clone)]
pub struct PatcherConfig {
    pub pre_roll_ticks: i32,
    pub post_roll_ticks: i32,
    pub capture_fps: i32,
}

impl Default for PatcherConfig {
    fn default() -> Self {
        Self {
            pre_roll_ticks: 200,
            post_roll_ticks: 60,
            capture_fps: 300,
        }
    }
}

pub fn build_batch_queue(raw_streaks: Vec<CaptureStreak>, config: &PatcherConfig) -> Vec<PatchJob> {
    let mut grouped: std::collections::HashMap<String, Vec<CaptureStreak>> = std::collections::HashMap::new();
    for streak in raw_streaks {
        grouped.entry(streak.source_demo.clone()).or_default().push(streak);
    }

    let mut jobs = Vec::new();

    for (source_demo, mut streaks) in grouped {
        // Sort by start_tick in ascending order
        streaks.sort_by_key(|s| s.start_tick);

        // Overlap Merge Logic
        let mut merged_streaks: Vec<CaptureStreak> = Vec::new();
        for current in streaks {
            if merged_streaks.is_empty() {
                merged_streaks.push(current);
            } else {
                let adjusted_start = (current.start_tick - config.pre_roll_ticks).max(0);
                let last = merged_streaks.last_mut().unwrap();
                if adjusted_start <= last.end_tick + config.post_roll_ticks {
                    last.end_tick = last.end_tick.max(current.end_tick);
                } else {
                    merged_streaks.push(current);
                }
            }
        }

        // Safe output path manipulation
        let path = std::path::Path::new(&source_demo);
        let output_demo = path.with_extension("").with_extension("patched.dem");

        jobs.push(PatchJob {
            source_demo,
            output_demo,
            streaks: merged_streaks,
        });
    }

    jobs
}

#[derive(Debug, Clone)]
pub enum PatchEvent {
    Starting(usize),
    Progress(String, f32),
    Completed,
    Cancelled,
    Error(String),
}

pub struct CaptureWorker {
    pub receiver: std::sync::mpsc::Receiver<PatchEvent>,
    pub is_running: bool,
    pub cancel_token: Arc<AtomicBool>,
    pub handle: Option<std::thread::JoinHandle<()>>,
}

pub fn spawn_patch_batch(
    jobs: Vec<PatchJob>,
    config: PatcherConfig,
    cancel_token: Arc<AtomicBool>,
) -> CaptureWorker {
    let (tx, rx) = std::sync::mpsc::channel();
    let cancel_token_clone = cancel_token.clone();
    
    let handle = std::thread::spawn(move || {
        if tx.send(PatchEvent::Starting(jobs.len())).is_err() {
            return;
        }

        let mut cancelled = false;
        for job in &jobs {
            if tx.send(PatchEvent::Progress(job.source_demo.clone(), 0.0)).is_err() {
                return;
            }

            let patcher = StreamPatcher::new(&job.source_demo, &job.output_demo);
            match patcher.patch(job, &config, &cancel_token_clone) {
                Ok(()) => {
                    if tx.send(PatchEvent::Progress(job.source_demo.clone(), 100.0)).is_err() {
                        return;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    std::fs::remove_file(&job.output_demo).ok();
                    let _ = tx.send(PatchEvent::Cancelled);
                    cancelled = true;
                    break;
                }
                Err(e) => {
                    if tx.send(PatchEvent::Error(format!("Failed to patch {}: {}", job.source_demo, e))).is_err() {
                        return;
                    }
                }
            }
        }

        if !cancelled {
            let _ = tx.send(PatchEvent::Completed);
        }
    });

    CaptureWorker {
        receiver: rx,
        is_running: true,
        cancel_token,
        handle: Some(handle),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_batch_queue_merging() {
        let config = PatcherConfig::default(); // pre = 200, post = 60
        let raw_streaks = vec![
            CaptureStreak {
                start_tick: 1000,
                end_tick: 1200,
                source_demo: "demo1.dem".to_string(),
            },
            CaptureStreak {
                start_tick: 1300, // starts at 1300. Pre-roll 200 makes adjusted_start = 1100.
                // Since 1100 <= last.end_tick (1200) + 60 (1260), this overlaps!
                end_tick: 1500,
                source_demo: "demo1.dem".to_string(),
            },
            CaptureStreak {
                start_tick: 2000, // adjusted_start = 1800. 1800 > 1560, so this does not overlap.
                end_tick: 2200,
                source_demo: "demo1.dem".to_string(),
            },
        ];

        let jobs = build_batch_queue(raw_streaks, &config);
        assert_eq!(jobs.len(), 1);
        let job = &jobs[0];
        assert_eq!(job.source_demo, "demo1.dem");
        assert_eq!(job.output_demo, std::path::PathBuf::from("demo1.patched.dem"));
        assert_eq!(job.streaks.len(), 2);
        assert_eq!(job.streaks[0].start_tick, 1000);
        assert_eq!(job.streaks[0].end_tick, 1500); // Merged 1000-1200 and 1300-1500
        assert_eq!(job.streaks[1].start_tick, 2000);
        assert_eq!(job.streaks[1].end_tick, 2200);
    }

    #[test]
    fn test_stream_patcher_end_to_end() {
        // Create a temporary scratch folder if it doesn't exist
        let scratch_dir = std::path::Path::new("scratch");
        if !scratch_dir.exists() {
            let _ = std::fs::create_dir_all(scratch_dir);
        }

        let input_path = scratch_dir.join("test_input.dem");
        let output_path = scratch_dir.join("test_output.dem");

        // Synthesize a valid minimal GoldSrc demo file
        let mut header = [0u8; 544];
        header[0..8].copy_from_slice(b"HLDEMO\0\0");
        // directory offset is at 540..544.
        // File structure:
        // Header: 544 bytes
        // Frame 1 (DemoStart, type 2): 9 bytes (offset 544..553)
        // Frame 2 (NextSection, type 5): 9 bytes (offset 553..562)
        // Directory count (i32): 4 bytes (offset 562..566)
        // Directory entry: 92 bytes (offset 566..658)
        // Total size = 658 bytes. Directory starts at offset 562.
        let directory_offset: i32 = 562;
        header[540..544].copy_from_slice(&directory_offset.to_le_bytes());

        let mut file_data = Vec::new();
        file_data.extend_from_slice(&header);

        // Frame 1: DemoStart (type 2), time = 0.0, tick = 0
        file_data.push(2);
        file_data.extend_from_slice(&0.0_f32.to_le_bytes());
        file_data.extend_from_slice(&0_i32.to_le_bytes());

        // Frame 2: NextSection (type 5), time = 0.0, tick = 0
        file_data.push(5);
        file_data.extend_from_slice(&0.0_f32.to_le_bytes());
        file_data.extend_from_slice(&0_i32.to_le_bytes());

        // Directory Block
        // Entry count: 1
        file_data.extend_from_slice(&1_i32.to_le_bytes());
        // Entry type: 1 (Playback)
        file_data.extend_from_slice(&1_i32.to_le_bytes());
        // Description: 64 bytes
        let mut desc = [0u8; 64];
        desc[..8].copy_from_slice(b"Playback");
        file_data.extend_from_slice(&desc);
        // flags (4), cd_track (4), track_time (4)
        file_data.extend_from_slice(&0_i32.to_le_bytes());
        file_data.extend_from_slice(&0_i32.to_le_bytes());
        file_data.extend_from_slice(&0.0_f32.to_le_bytes());
        // frame_count (4)
        file_data.extend_from_slice(&2_i32.to_le_bytes());
        // frame_offset (4) = 544
        file_data.extend_from_slice(&544_i32.to_le_bytes());
        // file_length (4) = 18
        file_data.extend_from_slice(&18_i32.to_le_bytes());

        std::fs::write(&input_path, &file_data).unwrap();

        // Instantiate patch job and config
        let job = PatchJob {
            source_demo: input_path.to_string_lossy().to_string(),
            output_demo: output_path.clone(),
            streaks: vec![
                CaptureStreak {
                    start_tick: 50,
                    end_tick: 100,
                    source_demo: input_path.to_string_lossy().to_string(),
                }
            ],
        };

        let config = PatcherConfig {
            pre_roll_ticks: 10,
            post_roll_ticks: 10,
            capture_fps: 300,
        };

        let patcher = StreamPatcher::new(&input_path, &output_path);
        let cancel_token = Arc::new(AtomicBool::new(false));
        patcher.patch(&job, &config, &cancel_token).unwrap();

        // Check if output file was created and is larger than original (due to injected command)
        assert!(output_path.exists());
        let output_data = std::fs::read(&output_path).unwrap();
        assert!(output_data.len() > file_data.len());

        // Cleanup
        let _ = std::fs::remove_file(input_path);
        let _ = std::fs::remove_file(output_path);
    }

    #[test]
    fn test_stream_patcher_cancellation_and_cleanup() {
        let scratch_dir = std::path::Path::new("scratch");
        if !scratch_dir.exists() {
            let _ = std::fs::create_dir_all(scratch_dir);
        }

        let input_path = scratch_dir.join("test_cancel_input.dem");
        let output_path = scratch_dir.join("test_cancel_output.dem");

        // Synthesize valid minimal header
        let mut header = [0u8; 544];
        header[0..8].copy_from_slice(b"HLDEMO\0\0");
        let directory_offset: i32 = 562;
        header[540..544].copy_from_slice(&directory_offset.to_le_bytes());

        let mut file_data = Vec::new();
        file_data.extend_from_slice(&header);

        // Frame 1: DemoStart (type 2), time = 0.0, tick = 0
        file_data.push(2);
        file_data.extend_from_slice(&0.0_f32.to_le_bytes());
        file_data.extend_from_slice(&0_i32.to_le_bytes());

        // Frame 2: NextSection (type 5), time = 0.0, tick = 0
        file_data.push(5);
        file_data.extend_from_slice(&0.0_f32.to_le_bytes());
        file_data.extend_from_slice(&0_i32.to_le_bytes());

        // Directory Block
        file_data.extend_from_slice(&1_i32.to_le_bytes());
        file_data.extend_from_slice(&1_i32.to_le_bytes());
        let mut desc = [0u8; 64];
        desc[..8].copy_from_slice(b"Playback");
        file_data.extend_from_slice(&desc);
        file_data.extend_from_slice(&0_i32.to_le_bytes());
        file_data.extend_from_slice(&0_i32.to_le_bytes());
        file_data.extend_from_slice(&0.0_f32.to_le_bytes());
        file_data.extend_from_slice(&2_i32.to_le_bytes());
        file_data.extend_from_slice(&544_i32.to_le_bytes());
        file_data.extend_from_slice(&18_i32.to_le_bytes());

        std::fs::write(&input_path, &file_data).unwrap();

        let job = PatchJob {
            source_demo: input_path.to_string_lossy().to_string(),
            output_demo: output_path.clone(),
            streaks: vec![
                CaptureStreak {
                    start_tick: 50,
                    end_tick: 100,
                    source_demo: input_path.to_string_lossy().to_string(),
                }
            ],
        };

        let config = PatcherConfig {
            pre_roll_ticks: 10,
            post_roll_ticks: 10,
            capture_fps: 300,
        };

        // Create a token that starts out as cancelled (true)
        let cancel_token = Arc::new(AtomicBool::new(true));

        let patcher = StreamPatcher::new(&input_path, &output_path);
        let res = patcher.patch(&job, &config, &cancel_token);

        // Should return interrupted error
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), std::io::ErrorKind::Interrupted);

        // Check spawn_patch_batch clean-up
        let cancel_token = Arc::new(AtomicBool::new(true));
        let worker = spawn_patch_batch(vec![job], config, cancel_token);
        let rx = worker.receiver;

        // Wait for worker completion events
        let mut got_cancelled = false;
        while let Ok(event) = rx.recv() {
            if let PatchEvent::Cancelled = event {
                got_cancelled = true;
            }
        }

        assert!(got_cancelled);
        // The output file should have been cleaned up and not exist
        assert!(!output_path.exists());

        // Cleanup input file
        let _ = std::fs::remove_file(input_path);
    }
}



