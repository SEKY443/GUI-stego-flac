//! Tauri desktop backend for stego-flac.
//!
//! Every command below calls `audio-modem-core`/`audio-modem-io` directly, in
//! this process — never a spawned copy of the `stego-flac` CLI binary. That
//! matters for the same reason the CLI itself has no `--passphrase` flag: a
//! secret handed to a subprocess as an argument or environment variable is
//! visible to `ps`/`/proc` on the machine. An in-process Tauri IPC call never
//! crosses that boundary in the first place.

mod commands;
mod error;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::plan::plan_preview,
            commands::info::inspect,
            commands::decode::decode,
            commands::encode::encode,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the stego-flac desktop app");
}
