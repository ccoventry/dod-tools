pub fn calculate_raw_sequence_bytes(w: i32, h: i32, fps: i32, duration_secs: f32) -> u64 {
    (w * h * 3) as u64 * (fps as u64) * (duration_secs as f64).ceil() as u64
}

// Disk query results are cached for TTL_MS milliseconds to avoid
// issuing blocking kernel disk-enumeration syscalls on every frame.
const TTL_MS: u128 = 2_000;

#[cfg(not(target_arch = "wasm32"))]
pub fn get_available_bytes(path: &std::path::Path) -> u64 {
    use sysinfo::{System, SystemExt, DiskExt};
    use std::sync::{OnceLock, Mutex};
    use std::collections::HashMap;
    use std::time::Instant;

    // Shared sysinfo System handle — only refreshed when cache is stale.
    static SYSTEM: OnceLock<Mutex<System>> = OnceLock::new();

    // Per-path TTL cache: path_key → (last_refresh, available_bytes)
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, u64)>>> = OnceLock::new();

    // Normalise path to a lowercase forward-slash key in a single pass.
    let path_key: String = path
        .to_string_lossy()
        .chars()
        .map(|c| if c == '\\' { '/' } else { c.to_ascii_lowercase() })
        .collect();

    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    // Fast path: return cached value if within TTL.
    if let Ok(guard) = cache.lock() {
        if let Some(&(last, bytes)) = guard.get(&path_key) {
            if last.elapsed().as_millis() < TTL_MS {
                return bytes;
            }
        }
    }

    // Slow path: refresh disk list and update cache.
    let sys_mutex = SYSTEM.get_or_init(|| {
        let mut sys = System::new();
        sys.refresh_disks_list();
        Mutex::new(sys)
    });

    let result = if let Ok(mut sys) = sys_mutex.lock() {
        sys.refresh_disks_list();
        sys.refresh_disks();

        let mut best_bytes = u64::MAX;
        let mut best_len = 0usize;

        for disk in sys.disks() {
            // Normalise mount point in-place without an extra owned String.
            let mount_cow = disk.mount_point().to_string_lossy();
            let mount_key: String = mount_cow
                .chars()
                .map(|c| if c == '\\' { '/' } else { c.to_ascii_lowercase() })
                .collect();

            if path_key.starts_with(&mount_key) && mount_key.len() > best_len {
                best_len = mount_key.len();
                best_bytes = disk.available_space();
            }
        }
        best_bytes
    } else {
        u64::MAX
    };

    // Store result in cache.
    if let Ok(mut guard) = cache.lock() {
        guard.insert(path_key, (Instant::now(), result));
    }

    result
}

#[cfg(target_arch = "wasm32")]
pub fn get_available_bytes(_path: &std::path::Path) -> u64 {
    u64::MAX
}
