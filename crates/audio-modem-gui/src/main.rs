// Windows: suppress the console window in release builds, matching the
// standard Tauri app template.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    audio_modem_gui::run();
}
