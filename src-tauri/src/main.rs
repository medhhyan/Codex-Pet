#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let codex_home = std::env::var_os("CODEX_HOME").map(std::path::PathBuf::from);
    codex_pet_island::run_with_codex_home(codex_home);
}
