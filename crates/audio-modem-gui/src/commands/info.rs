//! `inspect`, mirroring `stego-flac info`. Also doubles as the decode view's
//! "is this carrier encrypted" probe, since it already parses the header
//! without touching the passphrase.

use std::path::PathBuf;

use audio_modem_core::{from_i16, Carrier, Header, HEADER_LEN};
use audio_modem_io::flac_io;
use audio_modem_io::flac_tags::{self, PLAN_TAG, PROFILE_TAG};
use serde::Serialize;

use crate::commands::plan::PlanArgsDto;
use crate::error::{CmdResult, CommandError};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Argon2Dto {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoDto {
    pub path: String,
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub samples: usize,
    pub duration_secs: f64,
    pub profile_label: String,
    pub plan_in_metadata: bool,
    pub waveform_description: String,
    pub bit_rate: f64,
    pub band_hz: (f64, f64),
    pub format_version: u8,
    pub payload_bytes: u64,
    pub compressed: bool,
    pub encrypted: bool,
    pub argon2id: Option<Argon2Dto>,
    pub name_stored: bool,
    pub format_stored: bool,
    pub fec: bool,
    pub fec_symbol_size_bytes: u16,
    pub frame_bytes: u64,
    pub carried_bytes: u64,
    pub short_by_bytes: Option<u64>,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub fn inspect(path: String, plan: PlanArgsDto) -> CmdResult<InfoDto> {
    let path = PathBuf::from(path);
    let raw = std::fs::read(&path)
        .map_err(|error| CommandError::from(format!("reading {}: {error}", path.display())))?;

    let mut warnings = Vec::new();
    let recorded = audio_modem_io::plan_from_tags(&raw, plan.is_explicit()?, |msg| {
        warnings.push(msg.to_string())
    });
    let resolved = plan.resolve(recorded)?;
    let modem = Carrier::new(resolved).map_err(|error| CommandError::from(error.to_string()))?;

    let tags = flac_tags::read_tags(&raw).unwrap_or_default();
    let audio = flac_io::read_flac(&path).map_err(CommandError::from)?;
    warnings.extend(audio.warnings.iter().cloned());

    if audio.sample_rate != resolved.sample_rate() {
        return Err(CommandError::from(format!(
            "{} is {} Hz but the tone plan expects {} Hz",
            path.display(),
            audio.sample_rate,
            resolved.sample_rate()
        )));
    }

    let duration = audio.samples.len() as f64 / f64::from(audio.sample_rate);
    let (low_hz, high_hz) = resolved.band_hz();
    let profile_label = match tags.get(PROFILE_TAG) {
        Some(name) => name.clone(),
        None if recorded.is_some() => "custom (from metadata)".to_string(),
        None => "assumed default".to_string(),
    };
    let plan_in_metadata = recorded.is_some() || tags.contains_key(PLAN_TAG);

    // Only the header needs to be demodulated — at the default plan that is
    // 92 bytes, so this stays fast even for a multi-hour carrier.
    let needed = modem
        .modulated_len(HEADER_LEN)
        .next_multiple_of(modem.alignment_samples())
        * audio.channels;
    if audio.samples.len() < needed {
        return Err(CommandError::from(format!(
            "{} holds {} samples, fewer than the {needed} needed for a header",
            path.display(),
            audio.samples.len()
        )));
    }

    let header_samples = from_i16(&audio.samples[..needed]);
    let header_bytes = modem
        .demodulate_interleaved(&header_samples, audio.channels)
        .map_err(|error| CommandError::from(error.to_string()))?;
    let header = Header::parse(&header_bytes).map_err(|error| CommandError::from(error.to_string()))?;

    let declared = header.frame_len();
    let carried = ((audio.samples.len() / modem.alignment_samples()) as f64
        * modem.alignment_samples() as f64
        * resolved.bit_rate()
        / (8.0 * f64::from(resolved.sample_rate()))) as u64;

    Ok(InfoDto {
        path: path.display().to_string(),
        sample_rate_hz: audio.sample_rate,
        channels: audio.channels,
        samples: audio.samples.len(),
        duration_secs: duration,
        profile_label,
        plan_in_metadata,
        waveform_description: resolved.describe(),
        bit_rate: resolved.bit_rate(),
        band_hz: (low_hz, high_hz),
        format_version: header.version,
        payload_bytes: header.original_len,
        compressed: header.is_compressed(),
        encrypted: header.is_encrypted(),
        argon2id: header.is_encrypted().then_some(Argon2Dto {
            m_cost_kib: header.kdf.m_cost,
            t_cost: header.kdf.t_cost,
            p_cost: header.kdf.p_cost,
        }),
        name_stored: header.is_named(),
        format_stored: header.has_format(),
        fec: header.is_fec(),
        fec_symbol_size_bytes: header.fec_symbol_size,
        frame_bytes: declared,
        carried_bytes: carried,
        short_by_bytes: (carried < declared).then(|| declared - carried),
        warnings,
    })
}
