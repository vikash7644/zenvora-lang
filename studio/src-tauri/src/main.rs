#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[tauri::command]
fn compile_qv(source: String) -> Result<String, String> {
    qorvynt::compile_source_to_bytecode_v2(&source)
}

#[tauri::command]
fn run_qv(source: String) -> Result<Vec<String>, String> {
    qorvynt::run_source(&source)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![compile_qv, run_qv])
        .run(tauri::generate_context!())
        .expect("failed to launch QORVYNT Studio");
}
