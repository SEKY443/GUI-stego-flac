//! Adapts a Tauri event channel to the front-end-agnostic [`ProgressSink`]
//! trait from `audio-modem-io`.
//!
//! `encode_blocking`/`decode_blocking` depend on the trait, not on
//! `tauri::AppHandle` directly — that seam is what lets their business logic
//! run in a plain `#[test]` (with [`audio_modem_io::NoopProgress`] standing
//! in) rather than needing a live Tauri application to construct an
//! `AppHandle`.

use audio_modem_io::ProgressSink;
use tauri::{AppHandle, Emitter};

pub struct TauriProgress<'a> {
    app: &'a AppHandle,
    channel: &'static str,
}

impl<'a> TauriProgress<'a> {
    pub fn new(app: &'a AppHandle, channel: &'static str) -> Self {
        Self { app, channel }
    }
}

impl ProgressSink for TauriProgress<'_> {
    fn stage(&self, what: &str) {
        let _ = self.app.emit(self.channel, what);
    }
}
