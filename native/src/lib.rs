pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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

pub mod patch;

#[cfg(not(target_arch = "wasm32"))]
pub mod hlcr;

pub mod sys;
pub mod utils;
pub mod shared;

#[cfg(not(target_arch = "wasm32"))]
pub mod capture_engine;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
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
    let file_info = build_file_info(demo_path)?;

    Ok((file_info, analysis))
}

#[cfg(not(target_arch = "wasm32"))]
fn build_file_info(demo_path: &PathBuf) -> Result<FileInfo, String> {
    let metadata =
        fs::metadata(demo_path).map_err(|e| format!("Could not read metadata: {}", e))?;
    let size_bytes = metadata.len();
    let created_at = FileTime::from_last_modification_time(&metadata);
    let creation_offset = Duration::new(created_at.unix_seconds() as u64, created_at.nanoseconds());
    let created_at_system = SystemTime::UNIX_EPOCH + creation_offset;

    Ok(FileInfo {
        created_at: created_at_system,
        name: demo_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(String::from)
            .unwrap_or_default(),

        path: demo_path.to_str().map(String::from).unwrap_or_default(),
        size_bytes,
    })
}

// Bump whenever `AnalyzerState`/`Player`/related computed fields change, so
// caches written by an older schema are treated as a miss instead of
// silently deserializing with new fields missing/defaulted.
#[cfg(not(target_arch = "wasm32"))]
const ANALYZER_CACHE_SCHEMA_VERSION: u32 = 1;

#[cfg(not(target_arch = "wasm32"))]
#[derive(serde::Deserialize)]
struct AnalyzerCacheEntry {
    size_bytes: u64,
    modified_unix_secs: u64,
    file_info: FileInfo,
    analysis: Analysis,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(serde::Serialize)]
struct AnalyzerCacheEntryRef<'a> {
    size_bytes: u64,
    modified_unix_secs: u64,
    file_info: &'a FileInfo,
    analysis: &'a Analysis,
}

#[cfg(not(target_arch = "wasm32"))]
fn analyzer_cache_path(demo_path: &PathBuf) -> Option<PathBuf> {
    let canonical = fs::canonicalize(demo_path).ok()?;
    let key = canonical.to_string_lossy();
    let hash = crate::utils::demo_hasher::fnv1a_hash(key.as_bytes());
    Some(
        crate::shared::paths::get_appdata_dir()
            .join("analyzer_cache")
            .join(format!("v{}", ANALYZER_CACHE_SCHEMA_VERSION))
            .join(format!("{:016x}.json", hash)),
    )
}

/// Same as `run_analyzer_with_progress`, but backed by an on-disk JSON cache
/// keyed on the demo's canonicalized path, size, and mtime. A cache hit skips
/// straight to a ~10-15ms file read + deserialize instead of the full ~1.3s
/// parse; `progress_cb` is not invoked on the cache-hit path. Returns whether
/// the result came from cache as the third tuple element.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_analyzer_cached<F>(
    demo_path: &PathBuf,
    progress_cb: F,
) -> Result<(FileInfo, Analysis, bool), String>
where
    F: FnMut(usize, usize),
{
    let metadata =
        fs::metadata(demo_path).map_err(|e| format!("Could not read metadata: {}", e))?;
    let size_bytes = metadata.len();
    let modified_unix_secs = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let cache_path = analyzer_cache_path(demo_path);

    if let Some(cache_path) = &cache_path {
        if let Ok(bytes) = fs::read(cache_path) {
            if let Ok(entry) = serde_json::from_slice::<AnalyzerCacheEntry>(&bytes) {
                if entry.size_bytes == size_bytes && entry.modified_unix_secs == modified_unix_secs {
                    return Ok((entry.file_info, entry.analysis, true));
                }
            }
        }
    }

    let (file_info, analysis) = run_analyzer_with_progress(demo_path, progress_cb)?;

    if let Some(cache_path) = &cache_path {
        write_analyzer_cache_entry(cache_path, size_bytes, modified_unix_secs, &file_info, &analysis);
    }

    Ok((file_info, analysis, false))
}

#[cfg(not(target_arch = "wasm32"))]
fn write_analyzer_cache_entry(
    cache_path: &PathBuf,
    size_bytes: u64,
    modified_unix_secs: u64,
    file_info: &FileInfo,
    analysis: &Analysis,
) {
    if let Some(parent) = cache_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let entry_ref = AnalyzerCacheEntryRef {
        size_bytes,
        modified_unix_secs,
        file_info,
        analysis,
    };
    // Best-effort: a cache write failure must never fail the caller.
    if let Ok(json) = serde_json::to_vec(&entry_ref) {
        let _ = fs::write(cache_path, json);
    }
}

/// Cache-only read: returns `Some` on a fresh cache hit, `None` on a miss or
/// stale entry — never falls back to parsing. Used by callers that need to
/// know "is this already cheap to show" without paying the ~1.3s cold-parse
/// cost for every demo up front (e.g. the Analyzer's recursive demo browser
/// listing many files across watched folders at once).
#[cfg(not(target_arch = "wasm32"))]
pub fn peek_analyzer_cache(demo_path: &PathBuf) -> Option<(FileInfo, Analysis)> {
    let metadata = fs::metadata(demo_path).ok()?;
    let size_bytes = metadata.len();
    let modified_unix_secs = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let cache_path = analyzer_cache_path(demo_path)?;
    let bytes = fs::read(&cache_path).ok()?;
    let entry: AnalyzerCacheEntry = serde_json::from_slice(&bytes).ok()?;
    if entry.size_bytes == size_bytes && entry.modified_unix_secs == modified_unix_secs {
        Some((entry.file_info, entry.analysis))
    } else {
        None
    }
}

/// Writes `analysis` (already computed by a folder scan, e.g.
/// `scan_demo_for_highlights_with_analysis`) straight into the analyzer
/// cache, so a later `run_analyzer_cached` call for the same demo hits the
/// ~10-15ms cache path instead of re-parsing. Best-effort and silent on any
/// failure — cache warming must never affect the caller's own result.
#[cfg(not(target_arch = "wasm32"))]
pub fn warm_analyzer_cache(demo_path: &PathBuf, analysis: &Analysis) {
    let Ok(metadata) = fs::metadata(demo_path) else { return };
    let size_bytes = metadata.len();
    let modified_unix_secs = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let Some(cache_path) = analyzer_cache_path(demo_path) else { return };
    let Ok(file_info) = build_file_info(demo_path) else { return };

    write_analyzer_cache_entry(&cache_path, size_bytes, modified_unix_secs, &file_info, analysis);
}

#[cfg(not(target_arch = "wasm32"))]
static SESSION_INITIALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn log_markdown(msg: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use chrono::Local;
        use std::io::Write;
        let log_dir = crate::shared::paths::get_appdata_dir().join("logs");
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path_md = log_dir.join("crash_log.md");

        static LOG_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOG_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

        // Write to md
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&log_path_md)
        {
            if !SESSION_INITIALIZED.load(std::sync::atomic::Ordering::SeqCst) {
                SESSION_INITIALIZED.store(true, std::sync::atomic::Ordering::SeqCst);
                let time_str = Local::now().format("%Y-%m-%d @ %H:%M %Z").to_string();
                let _ = writeln!(f, "\n\n========== New Session: {} ====================\n", time_str);
            }
            let time_str = Local::now().format("%H:%M:%S").to_string();
            let _ = writeln!(f, "* [{}] {}", time_str, msg);
            let _ = f.sync_all();
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        log::info!("{}", msg);
    }
}
