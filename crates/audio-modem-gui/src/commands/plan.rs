//! Tone-plan resolution and the standalone plan explorer, mirroring
//! `stego-flac plan`.

use audio_modem_core::{Carrier, Plan, Profile};
use audio_modem_io::PlanOverrides;
use serde::{Deserialize, Serialize};

use crate::error::{CmdResult, CommandError};

/// Every tone-plan field the frontend can override, gathered from whichever
/// view is asking (the encode form's Advanced panel, the plan explorer, or a
/// decode's plan-mismatch recovery).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanArgsDto {
    pub profile: Option<String>,
    pub sample_rate: Option<u32>,
    pub amplitude: Option<f32>,
    pub samples_per_symbol: Option<usize>,
    pub bits_per_symbol: Option<u32>,
    pub bin_spacing: Option<usize>,
    pub fft_size: Option<usize>,
    pub qam_bits: Option<u32>,
    pub top_bin: Option<usize>,
    pub base_bin: Option<usize>,
}

impl PlanArgsDto {
    fn profile(&self) -> CmdResult<Option<Profile>> {
        match self.profile.as_deref() {
            None => Ok(None),
            Some("dense") => Ok(Some(Profile::Dense)),
            Some("compact") => Ok(Some(Profile::Compact)),
            Some("standard") => Ok(Some(Profile::Standard)),
            Some("fast") => Ok(Some(Profile::Fast)),
            Some(other) => Err(CommandError::from(format!("unknown profile {other:?}"))),
        }
    }

    pub fn to_overrides(&self) -> CmdResult<PlanOverrides> {
        Ok(PlanOverrides {
            profile: self.profile()?,
            sample_rate: self.sample_rate,
            amplitude: self.amplitude,
            samples_per_symbol: self.samples_per_symbol,
            bits_per_symbol: self.bits_per_symbol,
            bin_spacing: self.bin_spacing,
            fft_size: self.fft_size,
            qam_bits: self.qam_bits,
            top_bin: self.top_bin,
            base_bin: self.base_bin,
        })
    }

    pub fn is_explicit(&self) -> CmdResult<bool> {
        Ok(self.to_overrides()?.is_explicit())
    }

    pub fn resolve(&self, recorded: Option<Plan>) -> CmdResult<Plan> {
        self.to_overrides()?
            .resolve(recorded)
            .map_err(CommandError::from)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanInfoDto {
    pub description: String,
    pub sample_rate_hz: u32,
    pub band_hz: (f64, f64),
    pub amplitude: f32,
    pub mode: &'static str,
    pub bit_rate: f64,
    pub carrier_expansion_ratio: f64,
    pub duration_for_payload: Vec<DurationEntry>,
    pub presets: Vec<PresetEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DurationEntry {
    pub payload_bytes: u64,
    pub duration_secs: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetEntry {
    pub name: String,
    pub bit_rate: f64,
    pub description: String,
}

/// Live preview for the encode form and the standalone plan explorer.
///
/// Cheap (no I/O), so the frontend calls it on every field change rather than
/// only just before an encode — the same live-preview idea `stego-flac plan`
/// gives a terminal, just recomputed continuously.
#[tauri::command]
pub fn plan_preview(args: PlanArgsDto) -> CmdResult<PlanInfoDto> {
    let plan = args.resolve(None)?;
    let modem = Carrier::new(plan).map_err(|error| CommandError::from(error.to_string()))?;
    let (low_hz, high_hz) = plan.band_hz();

    let duration_for_payload = [1_024u64, 262_144, 20_000_000]
        .into_iter()
        .map(|size| DurationEntry {
            payload_bytes: size,
            duration_secs: modem.duration_secs(size as usize),
        })
        .collect();

    let presets = Profile::ALL
        .into_iter()
        .map(|profile| {
            let other = profile.plan();
            PresetEntry {
                name: profile.name().to_string(),
                bit_rate: other.bit_rate(),
                description: other.describe(),
            }
        })
        .collect();

    Ok(PlanInfoDto {
        description: plan.describe(),
        sample_rate_hz: plan.sample_rate(),
        band_hz: (low_hz, high_hz),
        amplitude: plan.amplitude(),
        mode: match plan {
            Plan::Fsk(_) => "fsk",
            Plan::Ofdm(_) => "ofdm",
        },
        bit_rate: plan.bit_rate(),
        carrier_expansion_ratio: plan.carrier_expansion_ratio(),
        duration_for_payload,
        presets,
    })
}
