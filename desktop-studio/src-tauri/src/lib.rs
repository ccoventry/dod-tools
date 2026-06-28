// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn test_bridge(path: String) -> String {
    format!("Tauri Backend received target: {}. Engine ready.", path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init()) // ADD THIS EXACT LINE
        .invoke_handler(tauri::generate_handler![test_bridge])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
