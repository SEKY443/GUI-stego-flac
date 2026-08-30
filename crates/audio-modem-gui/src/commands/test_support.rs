//! Test-only fixtures shared by the command test modules.

#![cfg(test)]

use std::path::PathBuf;

/// A scratch directory that removes itself, mirroring the equivalent helper
/// in `audio-modem-cli`'s integration tests — small enough that duplicating
/// it here is cheaper than sharing a crate for it across two test suites.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        let unique = format!(
            "audio-modem-gui-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).expect("creating the scratch directory");
        Self(path)
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A short 44.1 kHz mono WAV of a pure tone, just to exercise cover-audio
/// loading (downmix + resample) without needing a real recording on disk.
pub fn tone_wav(seconds: usize) -> Vec<u8> {
    let rate = 44_100u32;
    let frames = rate as usize * seconds;
    let mut pcm = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let t = i as f32 / rate as f32;
        let sample = (0.5 * (std::f32::consts::TAU * 440.0 * t).sin() * 32767.0) as i16;
        pcm.extend_from_slice(&sample.to_le_bytes());
    }

    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(&pcm);
    wav
}
