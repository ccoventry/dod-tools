pub fn calculate_raw_sequence_bytes(w: i32, h: i32, fps: i32, duration_secs: f32) -> u64 {
    (w * h * 3) as u64 * (fps as u64) * (duration_secs as f64).ceil() as u64
}

#[cfg(not(target_arch = "wasm32"))]
pub fn get_available_bytes(path: &std::path::Path) -> u64 {
    use sysinfo::{System, SystemExt, DiskExt};
    use std::sync::{OnceLock, Mutex};
    static SYSTEM: OnceLock<Mutex<System>> = OnceLock::new();
    
    let sys_mutex = SYSTEM.get_or_init(|| {
        let mut sys = System::new();
        sys.refresh_disks_list();
        Mutex::new(sys)
    });

    if let Ok(mut sys) = sys_mutex.lock() {
        sys.refresh_disks_list();
        sys.refresh_disks();
        let path_str = path.to_string_lossy().to_string().to_lowercase().replace('\\', "/");
        let mut best_match = None;
        let mut best_len = 0;
        
        for disk in sys.disks() {
            let mount = disk.mount_point().to_string_lossy().to_string().to_lowercase().replace('\\', "/");
            if path_str.starts_with(&mount) && mount.len() > best_len {
                best_len = mount.len();
                best_match = Some(disk);
            }
        }
        
        if let Some(disk) = best_match {
            return disk.available_space();
        }
    }
    
    u64::MAX
}

#[cfg(target_arch = "wasm32")]
pub fn get_available_bytes(_path: &std::path::Path) -> u64 {
    u64::MAX
}
