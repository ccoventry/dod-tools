use std::collections::HashMap;
use std::sync::OnceLock;

static LOCALIZATIONS: OnceLock<HashMap<String, String>> = OnceLock::new();

pub fn get_localizations() -> &'static HashMap<String, String> {
    LOCALIZATIONS.get_or_init(|| {
        load_localizations_from_disk()
    })
}

fn parse_kv_line(line: &str) -> Option<(String, String)> {
    let chars: Vec<char> = line.chars().collect();
    let mut quotes_indices = Vec::new();
    let mut escaped = false;
    
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && !escaped {
            escaped = true;
        } else {
            if c == '"' && !escaped {
                quotes_indices.push(i);
            }
            escaped = false;
        }
        i += 1;
    }
    
    if quotes_indices.len() >= 4 {
        let key: String = chars[quotes_indices[0] + 1..quotes_indices[1]].iter().collect();
        let val: String = chars[quotes_indices[2] + 1..quotes_indices[3]].iter().collect();
        Some((key, val))
    } else {
        None
    }
}

fn parse_localization_content(content: &str, map: &mut HashMap<String, String>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('{') || trimmed.starts_with('}') {
            continue;
        }
        if let Some((key, val)) = parse_kv_line(trimmed) {
            let mut key_clean = key.trim().to_lowercase();
            if !key_clean.starts_with('#') {
                key_clean.insert(0, '#');
            }
            map.insert(key_clean, val);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load_localizations_from_disk() -> HashMap<String, String> {
    let mut map = HashMap::new();
    
    // 1. Scan "./localizations" and executable folder's "./localizations"
    let paths = vec![
        std::path::PathBuf::from("localizations"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("localizations")))
            .unwrap_or_default(),
    ];
    
    for base_path in paths {
        if base_path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(base_path) {
                for entry in entries.filter_map(Result::ok) {
                    let path = entry.path();
                    if path.is_file() && path.extension().map(|s| s == "txt").unwrap_or(false) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            parse_localization_content(&content, &mut map);
                        }
                    }
                }
            }
        }
    }

    // 2. Scan current working directory for files matching "<mod>_<language>.txt" (any .txt containing an underscore)
    if let Ok(entries) = std::fs::read_dir(".") {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_lowercase();
                if name.contains('_') && name.ends_with(".txt") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        parse_localization_content(&content, &mut map);
                    }
                }
            }
        }
    }
    
    map
}

#[cfg(target_arch = "wasm32")]
fn load_localizations_from_disk() -> HashMap<String, String> {
    HashMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_kv_line() {
        let line = "\"#Game_joined_team\"\t\t\"%s1 joined team %s2\"";
        let parsed = parse_kv_line(line);
        assert_eq!(
            parsed,
            Some(("#Game_joined_team".to_string(), "%s1 joined team %s2".to_string()))
        );
        
        let line_escaped = "\"#Game_test\"\t\t\"This has a \\\"quote\\\" inside\"";
        let parsed_escaped = parse_kv_line(line_escaped);
        assert_eq!(
            parsed_escaped,
            Some(("#Game_test".to_string(), "This has a \\\"quote\\\" inside".to_string()))
        );
    }

    #[test]
    fn test_parse_localization_content() {
        let content = r##"
            "lang"
            {
                "Language" "English"
                "Tokens"
                {
                    "#Game_joined_team"        "%s1 joined team %s2"
                    "Game_join"                "%s1 joined the game"
                    // this is a comment
                    "#Game_disconnected"       "%s1 disconnected"
                }
            }
        "##;
        
        let mut map = HashMap::new();
        parse_localization_content(content, &mut map);
        
        assert_eq!(map.get("#game_joined_team"), Some(&"%s1 joined team %s2".to_string()));
        assert_eq!(map.get("#game_join"), Some(&"%s1 joined the game".to_string()));
        assert_eq!(map.get("#game_disconnected"), Some(&"%s1 disconnected".to_string()));
    }
}
