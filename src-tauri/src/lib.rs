pub mod app;
pub mod codex_adapter;
pub mod model;
pub mod state_machine;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    run_with_codex_home(None);
}

pub fn run_with_codex_home(codex_home: Option<std::path::PathBuf>) {
    app::run(codex_home)
        .expect("error while running Codex Pet Island");
}
