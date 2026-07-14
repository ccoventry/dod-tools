# Active Runtime Bugs
*Keep entries flat and direct. No paragraphs.*

- [ ] Bug: (Paste your current active code crash or glitch here)

## Completed/Resolved Bugs

### [Fixed/Mitigated] svc_bad (SZ_GetSpace overflow) Crash During Demo Fast-Forward
* **Symptom:** The GoldSrc engine (`hl.exe`) would deterministically crash with a `svc_bad` error when attempting to fast-forward (`host_framerate`) through a demo after executing a `mirv_recordmovie` cycle.
* **Root Cause:** A binary formatting violation in the stream patcher (`native/src/patch/engine.rs`). The patcher was injecting director event `BOOKMARK` frames *interleaved inside* the payload of original `NetworkMessage` frames. The engine read the original frame header, expected gameplay data, but read the injected bookmark bytes instead. It interpreted those bytes as a 32-bit packet length, resulting in a phantom ~2.9 Gigabyte packet that instantly overflowed the `netchan` buffer.
* **Resolution:** Restructured the `engine.rs` patcher loop. It now buffers the `info_block`, injects pending bookmarks as complete, standalone frames *first*, and then writes the original network frame header and payload to keep the binary stream 100% aligned.
* **Resolution Notes:** Hard-clamped `fast_forward_speed` to 0.05 and implemented 3-frame command redundancy with `clear;` prefix to ensure state synchronization during high-density network spikes.
