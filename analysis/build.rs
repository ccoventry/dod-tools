use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Tell Cargo to re-run if any files in localizations change
    println!("cargo:rerun-if-changed=../localizations");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("embedded_localizations.rs");

    let mut entries = Vec::new();
    let localizations_dir = Path::new("../localizations");
    if localizations_dir.exists() {
        scan_dir_recursive(localizations_dir, localizations_dir, &mut entries);
    }

    let mut content = String::new();
    content.push_str("pub static EMBEDDED_LOCALIZATIONS: &[(&str, &str)] = &[\n");
    for (rel_path, file_content) in entries {
        // Output each file name and content as a raw string literal with 5 hashes.
        // We replace backslashes with forward slashes for cross-platform consistency.
        let path_str = rel_path.replace('\\', "/");
        content.push_str(&format!(
            "    (r#####\"{}\"#####, r#####\"{}\"#####),\n",
            path_str, file_content
        ));
    }
    content.push_str("];\n");

    fs::write(dest_path, content).unwrap();
}

fn scan_dir_recursive(dir: &Path, base_dir: &Path, entries: &mut Vec<(String, String)>) {
    if let Ok(read_dir) = fs::read_dir(dir) {
        for entry in read_dir.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                scan_dir_recursive(&path, base_dir, entries);
            } else if path.is_file() && path.extension().map_or(false, |ext| ext == "txt") {
                if let Ok(content) = read_to_string_lossy_utf16_or_utf8(&path) {
                    if let Ok(rel_path) = path.strip_prefix(base_dir) {
                        entries.push((rel_path.to_string_lossy().into_owned(), content));
                    }
                }
            }
        }
    }
}

fn read_to_string_lossy_utf16_or_utf8(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    if bytes.len() >= 2 {
        if bytes[0] == 0xFF && bytes[1] == 0xFE {
            // UTF-16 LE
            let u16_chars: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();
            return Ok(String::from_utf16_lossy(&u16_chars));
        } else if bytes[0] == 0xFE && bytes[1] == 0xFF {
            // UTF-16 BE
            let u16_chars: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
                .collect();
            return Ok(String::from_utf16_lossy(&u16_chars));
        }
    }

    match String::from_utf8(bytes.clone()) {
        Ok(s) => Ok(s),
        Err(_) => {
            let has_nulls = bytes.iter().enumerate().any(|(i, &b)| b == 0 && i % 2 == 1);
            if has_nulls && bytes.len() % 2 == 0 {
                let u16_chars: Vec<u16> = bytes
                    .chunks_exact(2)
                    .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();
                Ok(String::from_utf16_lossy(&u16_chars))
            } else {
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            }
        }
    }
}
