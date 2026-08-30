//! `decode`, mirroring `stego-flac decode`.
//!
//! There is no separate "probe" command: the frontend calls [`super::info::inspect`]
//! first to learn whether a carrier is encrypted (and everything else about
//! it) before deciding whether to show a passphrase field, then calls
//! [`decode`] once the user has supplied one — the same "read the header
//! before asking for a passphrase" ordering the CLI uses, just split across
//! two IPC calls instead of one process.

use std::path::{Path, PathBuf};

use audio_modem_core::{decode_frame, from_i16, Carrier, DecodedPayload, Header};
use audio_modem_io::{flac_io, ProgressSink};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use zeroize::Zeroizing;

use crate::commands::plan::PlanArgsDto;
use crate::error::{CmdResult, CommandError};
use crate::progress::TauriProgress;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodeRequest {
    pub input_path: String,
    /// `None` derives the name from the carrier's stored (sanitized) filename.
    pub output_path: Option<String>,
    /// `None` or empty for an unencrypted carrier.
    pub passphrase: Option<String>,
    pub force: bool,
    #[serde(default)]
    pub plan: PlanArgsDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatDto {
    pub id: String,
    pub extension: String,
    pub description: String,
}

impl From<audio_modem_core::FileFormat> for FormatDto {
    fn from(format: audio_modem_core::FileFormat) -> Self {
        FormatDto {
            id: format.id.to_string(),
            extension: format.extension.to_string(),
            description: format.description.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodeReportDto {
    pub output_path: String,
    pub recovered_bytes: usize,
    pub name: Option<String>,
    pub format: Option<FormatDto>,
    pub encoded_at_unix: Option<u64>,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn decode(app: AppHandle, request: DecodeRequest) -> CmdResult<DecodeReportDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let progress = TauriProgress::new(&app, "decode://stage");
        decode_blocking(&progress, request)
    })
    .await
    .map_err(|error| CommandError::from(error.to_string()))?
}

pub(crate) fn decode_blocking(
    progress: &dyn ProgressSink,
    request: DecodeRequest,
) -> CmdResult<DecodeReportDto> {
    let input = PathBuf::from(&request.input_path);
    let raw = std::fs::read(&input)
        .map_err(|error| CommandError::from(format!("reading {}: {error}", input.display())))?;

    let mut warnings = Vec::new();
    let recorded = audio_modem_io::plan_from_tags(&raw, request.plan.is_explicit()?, |msg| {
        warnings.push(msg.to_string())
    });
    let plan = request.plan.resolve(recorded)?;
    let modem = Carrier::new(plan).map_err(|error| CommandError::from(error.to_string()))?;

    progress.stage("decoding FLAC");
    let audio = flac_io::read_flac(&input).map_err(CommandError::from)?;
    warnings.extend(audio.warnings.iter().cloned());

    if audio.channels == 0 || audio.channels > 8 {
        return Err(CommandError::from(format!(
            "{} declares {} channels; stego-flac carriers use 1 to 8",
            input.display(),
            audio.channels
        )));
    }
    if audio.sample_rate != plan.sample_rate() {
        return Err(CommandError::from(format!(
            "{} is {} Hz but the tone plan expects {} Hz",
            input.display(),
            audio.sample_rate,
            plan.sample_rate()
        )));
    }

    let group = modem.alignment_samples() * audio.channels;
    let usable = audio.samples.len() - audio.samples.len() % group;
    if usable == 0 {
        return Err(CommandError::from(format!(
            "{} holds {} samples, fewer than one symbol group",
            input.display(),
            audio.samples.len()
        )));
    }
    let samples = from_i16(&audio.samples[..usable]);

    progress.stage("demodulating");
    let frame = modem
        .demodulate_interleaved(&samples, audio.channels)
        .map_err(|error| CommandError::from(error.to_string()))?;

    let header = Header::parse(&frame).map_err(|error| {
        CommandError::from(if recorded.is_some() {
            format!("the carrier's recorded tone plan did not demodulate to a valid frame: {error}")
        } else {
            format!(
                "no tone plan found in the carrier's metadata, and none was demodulatable \
                 with the default plan: {error}. Supply the plan used to encode it."
            )
        })
    })?;

    let secret: Option<Zeroizing<Vec<u8>>> = match (header.is_encrypted(), &request.passphrase) {
        (true, Some(passphrase)) if !passphrase.is_empty() => {
            Some(Zeroizing::new(passphrase.as_bytes().to_vec()))
        }
        (true, _) => {
            return Err(CommandError::from(
                "this carrier is encrypted; a passphrase is required".to_string(),
            ))
        }
        (false, _) => None,
    };

    progress.stage("decrypting and decompressing");
    let payload: DecodedPayload = decode_frame(&frame, secret.as_ref().map(|s| s.as_slice()))
        .map_err(|error| CommandError::from(error.to_string()))?;

    let output_path = resolve_output_path(&input, &request, &payload)?;
    if output_path.exists() && !request.force {
        return Err(CommandError::from(format!(
            "{} already exists",
            output_path.display()
        )));
    }
    std::fs::write(&output_path, &payload.data).map_err(|error| {
        CommandError::from(format!("writing {}: {error}", output_path.display()))
    })?;

    Ok(DecodeReportDto {
        output_path: output_path.display().to_string(),
        recovered_bytes: payload.data.len(),
        name: payload.name.clone(),
        format: payload.format.map(FormatDto::from),
        encoded_at_unix: payload.encoded_at,
        warnings,
    })
}

/// Where to write the recovered file: `-o`-equivalent wins, otherwise the
/// name stored inside the encrypted payload, reduced to a single safe path
/// component by [`audio_modem_io::sanitize_stored_name`] — the same guard the
/// CLI uses, so a malicious carrier cannot steer the write outside the chosen
/// output directory here either.
///
/// Unlike the CLI (a terminal user has a working directory they chose), a
/// GUI process's current directory is an implementation detail the user
/// never sees — writing there by default would land the file somewhere they
/// have to go hunting for it. The carrier's own directory is the one
/// location a GUI user actually associates with "where I opened this from",
/// so that is the default instead.
fn resolve_output_path(
    input: &Path,
    request: &DecodeRequest,
    payload: &DecodedPayload,
) -> CmdResult<PathBuf> {
    if let Some(path) = &request.output_path {
        return Ok(PathBuf::from(path));
    }

    let stored = payload.suggested_name().ok_or_else(|| {
        CommandError::from(
            "this carrier does not store a filename; choose an output location".to_string(),
        )
    })?;

    let safe = audio_modem_io::sanitize_stored_name(stored).ok_or_else(|| {
        CommandError::from(format!("the carrier stores an unusable filename ({stored:?})"))
    })?;

    let dir = input.parent().filter(|p| !p.as_os_str().is_empty());
    Ok(match dir {
        Some(dir) => dir.join(safe),
        None => PathBuf::from(safe),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::plan::PlanArgsDto;

    fn request(output_path: Option<&str>) -> DecodeRequest {
        DecodeRequest {
            input_path: String::new(),
            output_path: output_path.map(str::to_owned),
            passphrase: None,
            force: false,
            plan: PlanArgsDto::default(),
        }
    }

    fn payload_named(name: &str) -> DecodedPayload {
        DecodedPayload {
            name: Some(name.to_string()),
            format: None,
            encoded_at: None,
            data: Vec::new(),
        }
    }

    fn payload_unnamed() -> DecodedPayload {
        DecodedPayload {
            name: None,
            format: None,
            encoded_at: None,
            data: Vec::new(),
        }
    }

    #[test]
    fn an_explicit_output_path_wins_over_the_stored_name() {
        let path = resolve_output_path(
            Path::new("/carriers/song.flac"),
            &request(Some("/elsewhere/chosen.bin")),
            &payload_named("report.pdf"),
        )
        .unwrap();
        assert_eq!(path, PathBuf::from("/elsewhere/chosen.bin"));
    }

    #[test]
    fn no_output_path_defaults_next_to_the_input_carrier() {
        let path = resolve_output_path(
            Path::new("/carriers/song.flac"),
            &request(None),
            &payload_named("report.pdf"),
        )
        .unwrap();
        assert_eq!(path, PathBuf::from("/carriers/report.pdf"));
    }

    #[test]
    fn a_traversal_attempt_in_the_stored_name_is_sanitized_before_joining() {
        let path = resolve_output_path(
            Path::new("/carriers/song.flac"),
            &request(None),
            &payload_named("../../.ssh/authorized_keys"),
        )
        .unwrap();
        // Confined to the carrier's own directory, not escaped from it.
        assert_eq!(path, PathBuf::from("/carriers/authorized_keys"));
    }

    #[test]
    fn an_input_with_no_directory_component_falls_back_to_a_bare_name() {
        let path = resolve_output_path(
            Path::new("song.flac"),
            &request(None),
            &payload_named("report.pdf"),
        )
        .unwrap();
        assert_eq!(path, PathBuf::from("report.pdf"));
    }

    #[test]
    fn no_output_path_and_no_stored_name_is_an_error() {
        assert!(resolve_output_path(Path::new("/carriers/song.flac"), &request(None), &payload_unnamed())
            .is_err());
    }

    #[test]
    fn a_stored_name_that_sanitizes_to_nothing_is_an_error() {
        assert!(resolve_output_path(
            Path::new("/carriers/song.flac"),
            &request(None),
            &payload_named(".."),
        )
        .is_err());
    }
}
