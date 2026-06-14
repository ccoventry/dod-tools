#[cfg(not(target_arch = "wasm32"))]
use analysis::Analysis;
#[cfg(not(target_arch = "wasm32"))]
use filetime::FileTime;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Read;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
use web_time::SystemTime;

pub struct FileInfo {
    pub created_at: SystemTime,
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
}

impl Default for FileInfo {
    fn default() -> Self {
        Self {
            created_at: SystemTime::UNIX_EPOCH,
            name: String::new(),
            path: String::new(),
            size_bytes: 0,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_analyzer(demo_path: &PathBuf) -> Result<(FileInfo, Analysis), String> {
    run_analyzer_with_progress(demo_path, |_, _| {})
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_analyzer_with_progress<F>(
    demo_path: &PathBuf,
    progress_cb: F,
) -> Result<(FileInfo, Analysis), String>
where
    F: FnMut(usize, usize),
{
    let mut file = fs::OpenOptions::new()
        .read(true)
        .open(demo_path)
        .map_err(|e| format!("Could not open the file: {}", e))?;

    let mut bytes: Vec<u8> = vec![];

    file.read_to_end(&mut bytes)
        .map_err(|e| format!("Could not read the file: {}", e))?;

    let analysis = Analysis::try_from_bytes_with_progress(bytes.as_slice(), progress_cb)?;

    let metadata =
        fs::metadata(demo_path).map_err(|e| format!("Could not read metadata: {}", e))?;
    let size_bytes = metadata.len();
    let created_at = FileTime::from_last_modification_time(&metadata);
    let creation_offset = Duration::new(created_at.unix_seconds() as u64, created_at.nanoseconds());
    let created_at_system = SystemTime::UNIX_EPOCH + creation_offset;

    let file_info = FileInfo {
        created_at: created_at_system,
        name: demo_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(String::from)
            .unwrap_or_default(),

        path: demo_path.to_str().map(String::from).unwrap_or_default(),
        size_bytes,
    };

    Ok((file_info, analysis))
}
