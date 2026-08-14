//! GoldSrc HLDEMO → Xash3D IDEM transcoder.
//!
//! # Why this exists
//!
//! Xash3D cannot open a DoD demo. The two engines use different demo
//! *containers*:
//!
//! | | GoldSrc (`.dem` from `hl.exe`) | Xash3D |
//! |---|---|---|
//! | magic | `"HLDEMO\0\0"` (8 bytes) | `"IDEM"` (i32) |
//! | demo protocol | 5 | 3 |
//! | path fields | `[260]` | `[64]` |
//!
//! `CL_ParseDemoHeader` bails on `hdr->id != IDEMOHEADER` before it looks at
//! anything else.
//!
//! However, Xash *does* accept one foreign value in `net_protocol`:
//! [`idem::PROTOCOL_GOLDSRC_VERSION_DEMO`] (176). With that set,
//! `CL_GetProtocolFromDemo` returns `PROTO_GOLDSRC` and the engine decodes the
//! contained network messages using GoldSrc protocol-48 rules. So the payload
//! a DoD demo already carries is understood — only the wrapper is wrong.
//!
//! This crate rewrites the wrapper.
//!
//! # What is lost
//!
//! GoldSrc has nine frame types; Xash has six commands. The mapping:
//!
//! | GoldSrc | Xash | note |
//! |---|---|---|
//! | `NetworkMessage(Start)` | `dem_norewind` | signon traffic |
//! | `NetworkMessage(Normal)` | `dem_read` | the actual gameplay stream |
//! | `DemoStart` | `dem_jumptime` | resets the section clock |
//! | `NextSection` | `dem_stop` | section terminator |
//! | `DemoBuffer` | `dem_userdata` | optional, off by default |
//! | `ConsoleCommand` | — | **dropped** — your injected director commands live here |
//! | `ClientData` | — | dropped |
//! | `Event` / `WeaponAnimation` / `Sound` | — | dropped |
//!
//! Dropping the client-side frames is why output lands around 25% of input
//! size. For a triage preview that is a feature, not a loss.
//!
//! # No I/O
//!
//! Everything here operates on `&Demo` and returns `Vec<u8>`, so it compiles
//! to `wasm32-unknown-unknown` unchanged. The CLI in `main.rs` owns all file
//! access.

use std::collections::BTreeMap;

use dem::types::{Demo, DirectoryEntry, FrameData, MessageData, NetworkMessageType};

pub mod idem;
pub mod resources;
pub mod writer;

use writer::ByteWriter;

/// How a source frame was handled.
pub type Stats = BTreeMap<String, usize>;

#[derive(Debug, Clone)]
pub struct Options {
    /// Written to the IDEM header. Xash clamps this; 0.0 plays back in slow motion.
    pub host_fps: f64,
    /// Forward `DemoBuffer` frames as `dem_userdata`. These are handed to
    /// `pfnDemo_ReadBuffer` on the client library — with a non-DoD client that
    /// is at best ignored, so the default drops them and saves a lot of bytes.
    pub keep_userdata: bool,
    /// Rebase each section's frame times so the first frame sits at t=0.
    pub rebase_time: bool,
    /// Overrides the gamedir field. `None` keeps whatever the source demo had.
    pub gamedir: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            host_fps: idem::DEFAULT_HOST_FPS,
            keep_userdata: false,
            rebase_time: true,
            gamedir: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EntrySummary {
    pub entrytype: i32,
    pub description: String,
    pub frames: i32,
    pub offset: i32,
    pub length: i32,
    pub playback_time: f32,
}

#[derive(Debug, Clone)]
pub struct Output {
    pub bytes: Vec<u8>,
    pub entries: Vec<EntrySummary>,
    pub stats: Stats,
}

#[inline]
fn bump(stats: &mut Stats, key: &str) {
    *stats.entry(key.to_string()).or_insert(0) += 1;
}

/// Resolve a frame's network payload to raw bytes.
///
/// Parse the demo with [`dem::types::MessageDataParseMode::Raw`] and this is a
/// borrow. `Parse` mode works too but costs a re-serialisation round trip
/// through `write_netmsg`, which is both slower and a fidelity risk — prefer
/// `Raw` unless you are also mutating messages.
fn raw_message(md: &MessageData) -> Option<&[u8]> {
    match md {
        MessageData::Raw(bytes) => Some(bytes.as_slice()),
        MessageData::Parsed(_) => None,
        MessageData::None => None,
    }
}

/// Transcode a parsed HLDEMO into an IDEM container.
pub fn transcode(demo: &Demo, opts: &Options) -> eyre::Result<Output> {
    transcode_entries(demo, &demo.directory.entries, opts)
}

fn transcode_entries(
    demo: &Demo,
    entries: &[DirectoryEntry],
    opts: &Options,
) -> eyre::Result<Output> {
    let mut stats = Stats::new();
    let mut w = ByteWriter::with_capacity(8 << 20);

    // ---- header ----------------------------------------------------------
    w.i32(idem::IDEMOHEADER);
    w.i32(idem::DEMO_PROTOCOL);
    w.i32(idem::PROTOCOL_GOLDSRC_VERSION_DEMO);
    w.f64(opts.host_fps);
    w.fixed_str(demo.header.map_name.as_slice(), idem::FIELD_WIDTH);
    w.fixed_str(b"transcoded by dod-tools", idem::FIELD_WIDTH);
    match &opts.gamedir {
        Some(g) => w.fixed_str(g.as_bytes(), idem::FIELD_WIDTH),
        None => w.fixed_str(demo.header.game_directory.as_slice(), idem::FIELD_WIDTH),
    }
    let dir_off_slot = w.offset();
    w.i32(0); // backpatched below

    debug_assert_eq!(w.offset(), idem::HEADER_SIZE);

    // ---- sections --------------------------------------------------------
    let mut summaries: Vec<EntrySummary> = Vec::with_capacity(entries.len());

    for entry in entries {
        let start = w.offset();
        let mut nframes: i32 = 0;

        let base = if opts.rebase_time {
            entry.frames.first().map(|f| f.time).unwrap_or(0.0)
        } else {
            0.0
        };

        for frame in &entry.frames {
            let dt = frame.time - base;

            match &frame.frame_data {
                FrameData::NetworkMessage(boxed) => {
                    let (kind, msg) = boxed.as_ref();
                    let cmd = match kind {
                        NetworkMessageType::Start => idem::DEM_NOREWIND,
                        NetworkMessageType::Normal => idem::DEM_READ,
                        // Types >= 10 are treated as network messages by the
                        // GoldSrc reader; follow suit rather than dropping data.
                        NetworkMessageType::Unknown(_) => idem::DEM_READ,
                    };

                    let payload = raw_message(&msg.messages).ok_or_else(|| {
                        eyre::eyre!(
                            "frame {} has no raw message bytes — reparse the demo with \
                             MessageDataParseMode::Raw",
                            frame.frame
                        )
                    })?;

                    if payload.len() as u32 > idem::MAX_INIT_MSG {
                        bump(&mut stats, "SKIPPED oversize netmsg");
                        continue;
                    }

                    w.u8(cmd);
                    w.f32(dt);

                    // CL_ReadDemoSequence expects exactly these seven, in this
                    // order. GoldSrc's SequenceInfo is field-for-field identical.
                    let s = &msg.sequence_info;
                    w.i32(s.incoming_sequence);
                    w.i32(s.incoming_acknowledged);
                    w.i32(s.incoming_reliable_acknowledged);
                    w.i32(s.incoming_reliable_sequence);
                    w.i32(s.outgoing_sequence);
                    w.i32(s.reliable_sequence);
                    w.i32(s.last_reliable_sequence);

                    w.i32(payload.len() as i32);
                    w.bytes(payload);

                    bump(
                        &mut stats,
                        if cmd == idem::DEM_READ {
                            "NetworkMessage -> dem_read"
                        } else {
                            "NetworkMessage -> dem_norewind"
                        },
                    );
                }

                FrameData::DemoStart => {
                    w.u8(idem::DEM_JUMPTIME);
                    w.f32(dt);
                    bump(&mut stats, "DemoStart -> dem_jumptime");
                }

                FrameData::NextSection => {
                    w.u8(idem::DEM_STOP);
                    w.f32(dt);
                    bump(&mut stats, "NextSection -> dem_stop");
                }

                FrameData::DemoBuffer(b) if opts.keep_userdata => {
                    w.u8(idem::DEM_USERDATA);
                    w.f32(dt);
                    w.i32(b.buffer.len() as i32);
                    w.bytes(&b.buffer);
                    bump(&mut stats, "DemoBuffer -> dem_userdata");
                }

                other => {
                    bump(&mut stats, &format!("dropped {}", frame_kind(other)));
                    continue;
                }
            }

            nframes += 1;
        }

        let length = (w.offset() - start) as i32;
        summaries.push(EntrySummary {
            entrytype: if entry.type_ == 0 {
                idem::DEMO_STARTUP
            } else {
                idem::DEMO_NORMAL
            },
            description: String::from_utf8_lossy(
                entry
                    .description
                    .as_slice()
                    .split(|&b| b == 0)
                    .next()
                    .unwrap_or(&[]),
            )
            .into_owned(),
            frames: nframes,
            offset: start as i32,
            length,
            playback_time: entry.track_time,
        });
    }

    // ---- directory -------------------------------------------------------
    let dir_off = w.offset();
    w.i32(summaries.len() as i32);
    for s in &summaries {
        w.i32(s.entrytype);
        w.f32(s.playback_time);
        w.i32(s.frames);
        w.i32(s.offset);
        w.i32(s.length);
        w.i32(0); // flags
        w.fixed_str(s.description.as_bytes(), idem::FIELD_WIDTH);
    }
    w.patch_i32(dir_off_slot, dir_off as i32);

    Ok(Output {
        bytes: w.into_vec(),
        entries: summaries,
        stats,
    })
}

fn frame_kind(f: &FrameData) -> &'static str {
    match f {
        FrameData::NetworkMessage(_) => "NetworkMessage",
        FrameData::DemoStart => "DemoStart",
        FrameData::ConsoleCommand(_) => "ConsoleCommand",
        FrameData::ClientData(_) => "ClientData",
        FrameData::NextSection => "NextSection",
        FrameData::Event(_) => "Event",
        FrameData::WeaponAnimation(_) => "WeaponAnimation",
        FrameData::Sound(_) => "Sound",
        FrameData::DemoBuffer(_) => "DemoBuffer",
    }
}

/// Transcode only `[start - preroll, end]` of the playback section.
///
/// The signon section is always kept whole — it carries the map name, resource
/// list and entity baselines, without which nothing loads.
///
/// # Delta-compression caveat
///
/// `svc_deltapacketentities` encodes against an earlier frame. A cut landing
/// mid-stream renders corrupt entities until the next full update arrives.
/// `preroll` is a blunt mitigation that usually works because full updates are
/// frequent.
///
/// The correct fix is to walk forward from `start` to the first frame carrying
/// a non-delta `svc_packetentities` and cut there. That needs message-level
/// parsing — reparse in `MessageDataParseMode::Parse` and match on
/// `EngineMessage::SvcPacketEntities` via `dem::netmsg_doer`. Left as a TODO
/// because for triage the blunt version is usually good enough, and doing it
/// properly costs a second parse pass over the whole demo.
pub fn cut(
    demo: &Demo,
    start: f32,
    end: f32,
    preroll: f32,
    opts: &Options,
) -> eyre::Result<Output> {
    let lo = (start - preroll).max(0.0);

    let mut kept: Vec<DirectoryEntry> = Vec::with_capacity(demo.directory.entries.len());

    for entry in &demo.directory.entries {
        if entry.type_ == 0 {
            kept.push(entry.clone());
            continue;
        }

        let mut e = entry.clone();
        e.frames = entry
            .frames
            .iter()
            .filter(|f| {
                matches!(
                    f.frame_data,
                    FrameData::DemoStart | FrameData::NextSection
                ) || (f.time >= lo && f.time <= end)
            })
            .cloned()
            .collect();

        // Guarantee a terminator; Xash treats a section without dem_stop as corrupt.
        if !e
            .frames
            .iter()
            .any(|f| matches!(f.frame_data, FrameData::NextSection))
        {
            if let Some(last) = entry.frames.last() {
                e.frames.push(last.clone());
            }
        }

        e.track_time = (end - lo).max(0.0);
        kept.push(e);
    }

    transcode_entries(demo, &kept, opts)
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Re-check produced bytes the way `CL_ParseDemoHeader`, `CL_PlayDemo_f` and
/// `CL_DemoReadMessage` would.
///
/// Passing here means the file is structurally acceptable to Xash. It does not
/// prove the *contents* replay correctly — that still needs a real engine.
pub fn validate(data: &[u8]) -> Vec<String> {
    let mut errs = Vec::new();

    macro_rules! rd_i32 {
        ($at:expr) => {{
            let a = $at;
            if a + 4 > data.len() {
                errs.push(format!("truncated read at {}", a));
                return errs;
            }
            i32::from_le_bytes([data[a], data[a + 1], data[a + 2], data[a + 3]])
        }};
    }

    if data.len() < idem::HEADER_SIZE {
        errs.push("shorter than IDEM header".into());
        return errs;
    }

    if rd_i32!(0) != idem::IDEMOHEADER {
        errs.push("id != IDEMOHEADER".into());
    }
    if rd_i32!(4) != idem::DEMO_PROTOCOL {
        errs.push(format!("dem_protocol {} != 3", rd_i32!(4)));
    }
    let np = rd_i32!(8);
    if np != idem::PROTOCOL_GOLDSRC_VERSION_DEMO {
        errs.push(format!("net_protocol {} would be rejected", np));
    }

    let dir_off = rd_i32!(212);
    if dir_off <= 0 || dir_off as usize >= data.len() {
        errs.push(format!("directory_offset {} out of range", dir_off));
        return errs;
    }

    let n = rd_i32!(dir_off as usize);
    if !(1..=1024).contains(&n) {
        errs.push(format!("bogus numentries {}", n));
        return errs;
    }

    for i in 0..n as usize {
        let p = dir_off as usize + 4 + i * idem::ENTRY_SIZE;
        if p + idem::ENTRY_SIZE > data.len() {
            errs.push(format!("entry {} truncated", i));
            return errs;
        }
        let off = rd_i32!(p + 12) as usize;
        let len = rd_i32!(p + 16) as usize;
        if off + len > data.len() {
            errs.push(format!("entry {} extends past EOF", i));
            continue;
        }

        let (mut q, end) = (off, off + len);
        let mut saw_stop = false;
        while q < end {
            let cmd = data[q];
            q += 1;
            if cmd > idem::DEM_STOP {
                errs.push(format!("entry {}: cmd {} > dem_stop", i, cmd));
                break;
            }
            q += 4; // dt
            match cmd {
                idem::DEM_NOREWIND | idem::DEM_READ => {
                    q += 28;
                    let mlen = rd_i32!(q);
                    q += 4;
                    if mlen < 0 || mlen as u32 > idem::MAX_INIT_MSG {
                        errs.push(format!("entry {}: msglen {} rejected", i, mlen));
                        break;
                    }
                    q += mlen as usize;
                }
                idem::DEM_USERDATA => {
                    let sz = rd_i32!(q);
                    q += 4 + sz.max(0) as usize;
                }
                idem::DEM_USERCMD => {
                    q += 8;
                    let nb = u16::from_le_bytes([data[q], data[q + 1]]) as usize;
                    q += 2 + nb;
                }
                idem::DEM_STOP => saw_stop = true,
                _ => {}
            }
        }
        if q != end {
            errs.push(format!(
                "entry {}: frame walk ended at {}, expected {} (drift {})",
                i,
                q,
                end,
                q as i64 - end as i64
            ));
        }
        if !saw_stop {
            errs.push(format!("entry {}: no terminating dem_stop", i));
        }
    }

    errs
}
