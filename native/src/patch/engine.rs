// patch/engine.rs
// Binary stream manipulation against the GoldSrc .dem wire format.
// Reads/writes to disk — native-only.
// Strict 468-byte NetworkMessage alignment must be preserved; see ai_project_context.md.

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use crate::patch::types::{PatchJob, PatcherConfig};

// ── Internal helper ───────────────────────────────────────────────────────────

fn write_console_cmd(writer: &mut std::io::BufWriter<std::fs::File>, time: f32, tick: i32, cmd: &str) -> std::io::Result<i32> {
    use std::io::Write;
    writer.write_all(&[3_u8])?;
    writer.write_all(&time.to_le_bytes())?;
    writer.write_all(&tick.to_le_bytes())?;
    let mut payload = vec![0_u8; 64];
    let cmd_bytes = cmd.as_bytes();
    let len = cmd_bytes.len().min(63);
    payload[..len].copy_from_slice(&cmd_bytes[..len]);
    writer.write_all(&payload)?;
    Ok(73)
}

// ── Stream patcher ────────────────────────────────────────────────────────────

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

    pub fn patch(&self, job: &PatchJob, _config: &PatcherConfig, cancel_token: &Arc<AtomicBool>) -> Result<(), std::io::Error> {
        use std::io::{BufReader, BufWriter, Read, Write, Seek, SeekFrom};

        let input_file = std::fs::File::open(&job.source_demo)?;
        let output_file = std::fs::File::create(&job.output_demo)?;

        let mut reader = BufReader::with_capacity(256 * 1024, input_file);
        let mut writer = BufWriter::with_capacity(256 * 1024, output_file);

        let read_exact = |reader: &mut BufReader<std::fs::File>, buf: &mut [u8], label: &str| -> Result<(), std::io::Error> {
            let expected_size = buf.len();
            let file_cursor_position = reader.stream_position().unwrap_or(0);
            match reader.read_exact(buf) {
                Ok(()) => Ok(()),
                Err(e) => {
                    if !(e.kind() == std::io::ErrorKind::UnexpectedEof && label == "Frame Header") {
                        crate::log_markdown(&format!(
                            "Buffer Error [{}]: Expected {} bytes at offset {}, but hit EOF.",
                            label, expected_size, file_cursor_position
                        ));
                    }
                    Err(e)
                }
            }
        };

        // Step 2: Parse and Clone the Header
        let mut header = vec![0u8; 544];
        read_exact(&mut reader, &mut header, "Header")?;

        // Extract directory offset (i32 is at offset 540..544)
        let original_offset = i32::from_le_bytes(header[540..544].try_into().unwrap());

        writer.write_all(&header)?;

        let mut bytes_injected: i32 = 0;
        let mut is_first_frame = true;
        let mut scheduled_queue: std::collections::VecDeque<(i32, String)> = job.scheduled_commands.iter().cloned().collect();

        // Step 3: Zero-Allocation Copy Loop
        let mut payload_buf = Vec::new();

        loop {
            if cancel_token.load(Ordering::Relaxed) {
                return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Cancelled by user"));
            }

            let pos = reader.stream_position().unwrap_or(0);
            if original_offset > 0 && pos >= original_offset as u64 {
                break;
            }

            let mut frame_hdr = vec![0u8; 9];
            if let Err(e) = read_exact(&mut reader, &mut frame_hdr, "Frame Header") {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    break;
                }
                return Err(e);
            }

            let type_byte = frame_hdr[0];
            let time = f32::from_le_bytes(frame_hdr[1..5].try_into().unwrap());
            let tick = i32::from_le_bytes(frame_hdr[5..9].try_into().unwrap());

            if is_first_frame {
                for cmd in &job.init_commands {
                    bytes_injected += write_console_cmd(&mut writer, time, tick, cmd)?;
                }
                is_first_frame = false;
            }

            while let Some((target_tick, _cmd)) = scheduled_queue.front() {
                if tick >= *target_tick {
                    let (_, cmd) = scheduled_queue.pop_front().unwrap();
                    bytes_injected += write_console_cmd(&mut writer, time, tick, &cmd)?;
                } else {
                    break;
                }
            }

            writer.write_all(&frame_hdr)?;

            // Determine payload size to read/write
            match type_byte {
                2 => {
                    // DemoStart (0 bytes)
                }
                3 => {
                    // ConsoleCommand (64 bytes)
                    payload_buf.resize(64, 0);
                    read_exact(&mut reader, &mut payload_buf, "ConsoleCommand")?;
                    writer.write_all(&payload_buf)?;
                }
                4 => {
                    // ClientData (32 bytes)
                    payload_buf.resize(32, 0);
                    read_exact(&mut reader, &mut payload_buf, "ClientData")?;
                    writer.write_all(&payload_buf)?;
                }
                5 => {
                    // NextSection (0 bytes)
                }
                6 => {
                    // Event (84 bytes)
                    payload_buf.resize(84, 0);
                    read_exact(&mut reader, &mut payload_buf, "Event")?;
                    writer.write_all(&payload_buf)?;
                }
                7 => {
                    // WeaponAnimation (8 bytes)
                    payload_buf.resize(8, 0);
                    read_exact(&mut reader, &mut payload_buf, "WeaponAnimation")?;
                    writer.write_all(&payload_buf)?;
                }
                8 => {
                    // Sound (24 bytes + sample_length)
                    let mut prefix = vec![0u8; 8];
                    read_exact(&mut reader, &mut prefix, "Sound Prefix")?;
                    writer.write_all(&prefix)?;
                    let sample_length = u32::from_le_bytes(prefix[4..8].try_into().unwrap()) as usize;

                    if sample_length > 2_000_000 {
                        crate::log_markdown(&format!(
                            "Alignment lost! Read impossible size: {} at offset {}",
                            sample_length,
                            reader.stream_position().unwrap_or(0)
                        ));
                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Parser alignment lost"));
                    }

                    payload_buf.resize(sample_length + 16, 0);
                    read_exact(&mut reader, &mut payload_buf, "Sound Payload")?;
                    writer.write_all(&payload_buf)?;
                }
                9 => {
                    // DemoBuffer (4 bytes + buffer_length)
                    let mut prefix = vec![0u8; 4];
                    read_exact(&mut reader, &mut prefix, "DemoBuffer Prefix")?;
                    writer.write_all(&prefix)?;
                    let buffer_length = u32::from_le_bytes(prefix[0..4].try_into().unwrap()) as usize;

                    if buffer_length > 2_000_000 {
                        crate::log_markdown(&format!(
                            "Alignment lost! Read impossible size: {} at offset {}",
                            buffer_length,
                            reader.stream_position().unwrap_or(0)
                        ));
                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Parser alignment lost"));
                    }

                    payload_buf.resize(buffer_length, 0);
                    read_exact(&mut reader, &mut payload_buf, "DemoBuffer Payload")?;
                    writer.write_all(&payload_buf)?;
                }
                _ => {
                    // NetworkMessage (468 bytes + message_length)
                    let mut info_buf = vec![0u8; 464];
                    read_exact(&mut reader, &mut info_buf, "NetworkMessage Info")?;
                    writer.write_all(&info_buf)?;

                    let mut len_buf = vec![0u8; 4];
                    read_exact(&mut reader, &mut len_buf, "NetworkMessage Length")?;
                    writer.write_all(&len_buf)?;
                    let msg_len = u32::from_le_bytes(len_buf.try_into().unwrap()) as usize;

                    if msg_len > 2_000_000 {
                        crate::log_markdown(&format!(
                            "Alignment lost! Read impossible size: {} at offset {}",
                            msg_len,
                            reader.stream_position().unwrap_or(0)
                        ));
                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Parser alignment lost"));
                    }

                    payload_buf.resize(msg_len, 0);
                    read_exact(&mut reader, &mut payload_buf, "NetworkMessage Payload")?;
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

        out_file.sync_all().unwrap_or_default();
        drop(out_file);
        drop(reader);

        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::types::{CaptureStreak, PatcherConfig, PatchJob};
    use crate::patch::builder::spawn_patch_batch;
    use crate::patch::types::PatchEvent;

    #[test]
    fn test_stream_patcher_end_to_end() {
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

        let job = PatchJob {
            source_demo: input_path.to_string_lossy().to_string(),
            output_demo: output_path.clone(),
            streaks: vec![
                CaptureStreak {
                    start_tick: 50,
                    end_tick: 100,
                    source_demo: input_path.to_string_lossy().to_string(),
                    target_player: None,
                    kill_count: 0,
                    timeline_string: String::new(),
                    duration_string: String::new(),
                    player_index: 0,
                    kills: Vec::new(),
                    start_index: 0,
                    end_index: 0,
                }
            ],
            target_player: None,
            init_commands: vec!["host_framerate 0".to_string()],
            scheduled_commands: vec![(10, "some_command".to_string())],
        };

        let config = PatcherConfig {
            pre_roll_ticks: 10,
            post_roll_ticks: 10,
            capture_fps: 300,
            exit_on_finish: true,
            init_commands: vec!["host_framerate 0".to_string()],
            custom_commands: Vec::new(),
            pre_roll_seconds: 2.0,
            post_roll_seconds: 0.6,
            record_start_lead: 0.0,
            record_stop_trail: 0.0,
            initial_delay: 3.0,
            ..PatcherConfig::default()
        };

        let patcher = StreamPatcher::new(&input_path, &output_path);
        let cancel_token = Arc::new(AtomicBool::new(false));
        patcher.patch(&job, &config, &cancel_token).unwrap();

        assert!(output_path.exists());
        let output_data = std::fs::read(&output_path).unwrap();
        assert!(output_data.len() > file_data.len());

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

        let mut header = [0u8; 544];
        header[0..8].copy_from_slice(b"HLDEMO\0\0");
        let directory_offset: i32 = 562;
        header[540..544].copy_from_slice(&directory_offset.to_le_bytes());

        let mut file_data = Vec::new();
        file_data.extend_from_slice(&header);

        file_data.push(2);
        file_data.extend_from_slice(&0.0_f32.to_le_bytes());
        file_data.extend_from_slice(&0_i32.to_le_bytes());

        file_data.push(5);
        file_data.extend_from_slice(&0.0_f32.to_le_bytes());
        file_data.extend_from_slice(&0_i32.to_le_bytes());

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
                    target_player: None,
                    kill_count: 0,
                    timeline_string: String::new(),
                    duration_string: String::new(),
                    player_index: 0,
                    kills: Vec::new(),
                    start_index: 0,
                    end_index: 0,
                }
            ],
            target_player: None,
            init_commands: vec!["host_framerate 0".to_string()],
            scheduled_commands: vec![(10, "some_command".to_string())],
        };

        let config = PatcherConfig {
            pre_roll_ticks: 10,
            post_roll_ticks: 10,
            capture_fps: 300,
            exit_on_finish: true,
            init_commands: vec!["host_framerate 0".to_string()],
            custom_commands: Vec::new(),
            pre_roll_seconds: 2.0,
            post_roll_seconds: 0.6,
            record_start_lead: 0.0,
            record_stop_trail: 0.0,
            initial_delay: 3.0,
            ..PatcherConfig::default()
        };

        // Create a token that starts out as cancelled (true)
        let cancel_token = Arc::new(AtomicBool::new(true));

        let patcher = StreamPatcher::new(&input_path, &output_path);
        let res = patcher.patch(&job, &config, &cancel_token);

        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), std::io::ErrorKind::Interrupted);

        // Check spawn_patch_batch clean-up
        let cancel_token = Arc::new(AtomicBool::new(true));
        let worker = spawn_patch_batch(vec![job], config, cancel_token);
        let rx = worker.receiver;

        let mut got_cancelled = false;
        while let Ok(event) = rx.recv() {
            if let PatchEvent::Cancelled = event {
                got_cancelled = true;
            }
        }

        assert!(got_cancelled);
        assert!(!output_path.exists());

        let _ = std::fs::remove_file(input_path);
    }
}
