//! `encode`, mirroring `stego-flac encode`.

use std::path::{Path, PathBuf};

use audio_modem_core::frame::volume;
use audio_modem_core::modem::ofdm::{COVER_FULL_HZ, COVER_TELEPHONE_HZ, COVER_WIDE_HZ};
use audio_modem_core::{encode_frame, format, to_i16, Carrier, EncodeParams, FecParams, KdfParams, Plan};
use audio_modem_io::flac_tags::{PLAN_TAG, PROFILE_TAG, VOLUME_TAG};
use audio_modem_io::{cover, flac_io, volume_path, ProgressSink};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use zeroize::Zeroizing;

use crate::commands::decode::FormatDto;
use crate::commands::plan::PlanArgsDto;
use crate::error::{CmdResult, CommandError};
use crate::progress::TauriProgress;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverOptions {
    pub path: String,
    /// `auto` | `telephone` | `wide` | `full`.
    pub quality: String,
    /// `cut` | `spread`.
    pub mode: String,
    pub attenuation_db: f32,
    pub keep_metadata: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodeRequest {
    pub input_path: String,
    pub output_path: String,
    /// `None` or empty writes an unencrypted carrier (the CLI's `--no-encrypt`).
    pub passphrase: Option<String>,
    pub name: Option<String>,
    pub no_store_name: bool,
    pub level: i32,
    pub fec_overhead: u8,
    pub fec_symbol_size: u16,
    /// `"auto"` or `"1"`..`"8"`.
    pub channels: String,
    pub cover: Option<CoverOptions>,
    /// Split the carrier across several smaller FLAC files instead of one.
    /// `None` or a size at or above the finished frame writes a single file.
    /// Mutually exclusive with `cover`, mirroring the CLI's `--split-size`.
    pub split_size_bytes: Option<u64>,
    #[serde(default)]
    pub plan: PlanArgsDto,
    pub force: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressedDto {
    pub bytes: usize,
    pub ratio: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodeReportDto {
    /// The single carrier's path, or the first volume's path when split into
    /// several — see `volumes` for the rest.
    pub output_path: String,
    pub plaintext_bytes: usize,
    pub compressed: Option<CompressedDto>,
    pub encrypted: bool,
    pub stored_name: Option<String>,
    pub detected_format: Option<FormatDto>,
    pub fec_packets: usize,
    pub fec_repair_percent: u8,
    pub frame_bytes: usize,
    pub expansion_ratio: f64,
    pub waveform_description: String,
    pub bit_rate: f64,
    pub band_hz: (f64, f64),
    pub cover_band_hz: Option<(f64, f64)>,
    pub channels: usize,
    pub channels_auto: bool,
    pub duration_secs: f64,
    pub carrier_bytes: u64,
    pub carrier_ratio: f64,
    /// One entry per part when `split_size_bytes` produced more than one
    /// volume; empty for an ordinary single-file carrier, in which case the
    /// top-level fields above already describe the whole result.
    pub volumes: Vec<EncodedVolumeDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodedVolumeDto {
    pub part: u32,
    pub of: u32,
    pub path: String,
    pub channels: usize,
    pub duration_secs: f64,
    pub carrier_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelChoice {
    Auto,
    Fixed(usize),
}

fn parse_channels(text: &str) -> CmdResult<ChannelChoice> {
    if text.eq_ignore_ascii_case("auto") {
        return Ok(ChannelChoice::Auto);
    }
    match text.parse::<usize>() {
        Ok(n) if (1..=8).contains(&n) => Ok(ChannelChoice::Fixed(n)),
        _ => Err(CommandError::from(format!(
            "expected 1-8 or \"auto\" for channels, got {text:?}"
        ))),
    }
}

#[tauri::command]
pub async fn encode(app: AppHandle, request: EncodeRequest) -> CmdResult<EncodeReportDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let progress = TauriProgress::new(&app, "encode://stage");
        encode_blocking(&progress, request)
    })
    .await
    .map_err(|error| CommandError::from(error.to_string()))?
}

pub(crate) fn encode_blocking(
    progress: &dyn ProgressSink,
    request: EncodeRequest,
) -> CmdResult<EncodeReportDto> {
    let input = PathBuf::from(&request.input_path);
    let output = PathBuf::from(&request.output_path);
    if output.exists() && !request.force {
        return Err(CommandError::from(format!(
            "{} already exists",
            output.display()
        )));
    }

    if request.cover.is_some() && request.split_size_bytes.is_some() {
        return Err(CommandError::from(
            "splitting into volumes cannot be combined with cover audio: a split's parts \
             would each need their own disguise. Drop one or the other."
                .to_string(),
        ));
    }

    let mut plan = request.plan.resolve(None)?;
    let channels_choice = parse_channels(&request.channels)?;

    // Fail before any expensive work — compression, encryption, a multi-minute
    // encode — the same "check the waveform can carry a cover at all first"
    // ordering the CLI uses.
    if request.cover.is_some() && plan.set_cover_ceiling(COVER_TELEPHONE_HZ, 0.0).is_none() {
        return Err(CommandError::from(format!(
            "cover audio needs a waveform with a spectrum to partition; {} lights one tone \
             at a time. Choose the dense or compact profile.",
            plan.describe()
        )));
    }

    let plaintext = std::fs::read(&input)
        .map_err(|error| CommandError::from(format!("reading {}: {error}", input.display())))?;

    let secret: Option<Zeroizing<Vec<u8>>> = match &request.passphrase {
        Some(passphrase) if !passphrase.is_empty() => {
            Some(Zeroizing::new(passphrase.as_bytes().to_vec()))
        }
        _ => None,
    };

    let params = EncodeParams {
        compression_level: request.level,
        passphrase: secret.as_ref().map(|s| s.as_slice()),
        kdf: KdfParams::default(),
        fec: FecParams {
            symbol_size: request.fec_symbol_size,
            repair_overhead_percent: request.fec_overhead,
        },
        store_format: !request.no_store_name,
        store_timestamp: !request.no_store_name,
    };

    let detected = format::detect(&plaintext);
    let stored_name = if request.no_store_name {
        None
    } else if let Some(name) = &request.name {
        Some(name.clone())
    } else {
        input
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    };

    progress.stage("compressing and encrypting");
    let (frame, report) = encode_frame(&plaintext, stored_name.as_deref(), &params)
        .map_err(|error| CommandError::from(error.to_string()))?;

    // The band is settled here, after the frame exists, because `auto` keys
    // off the post-compression/encryption/FEC size, not the raw input.
    let cover_audio = match &request.cover {
        Some(opts) => {
            let ceiling = match opts.quality.as_str() {
                "auto" => plan.set_auto_cover(frame.len(), f64::from(opts.attenuation_db)),
                "telephone" => {
                    plan.set_cover_ceiling(COVER_TELEPHONE_HZ, f64::from(opts.attenuation_db))
                }
                "wide" => plan.set_cover_ceiling(COVER_WIDE_HZ, f64::from(opts.attenuation_db)),
                "full" => plan.set_cover_ceiling(COVER_FULL_HZ, f64::from(opts.attenuation_db)),
                other => return Err(CommandError::from(format!("unknown cover quality {other:?}"))),
            }
            .ok_or_else(|| CommandError::from("cover band could not be reserved".to_string()))?;

            plan.validate().map_err(|error| {
                CommandError::from(format!("cover band does not fit this plan: {error}"))
            })?;

            progress.stage("loading cover audio");
            Some(
                cover::load(Path::new(&opts.path), plan.sample_rate(), ceiling as f32)
                    .map_err(CommandError::from)?,
            )
        }
        None => None,
    };

    let cover_tags = match &request.cover {
        Some(opts) if opts.keep_metadata => {
            cover::read_tags(Path::new(&opts.path)).map_err(CommandError::from)?
        }
        _ => Vec::new(),
    };

    // The stride needs both lengths, so it is decided after the frame exists
    // and the cover is loaded, and before the modem is built (the stride is
    // part of the plan the metadata records).
    if let (Some(opts), Some(audio)) = (&request.cover, &cover_audio) {
        if opts.mode == "spread" {
            let symbols = plan.symbols_for(frame.len());
            let cover_symbols = audio.len() / 512;
            if symbols > 0 && cover_symbols > symbols {
                plan.set_spread(cover_symbols / symbols);
            }
        }
    }

    let plan = plan;
    let modem = Carrier::new(plan).map_err(|error| CommandError::from(error.to_string()))?;

    // Splitting only ever applies to the plain, uncovered path: `cover` and
    // `split_size_bytes` are mutually exclusive (checked above), so
    // `cover_audio` is always `None` whenever more than one part is written.
    // A requested size at or above the finished frame collapses to a single
    // part, written under the ordinary `output` path with no `.partI-of-N`
    // suffix.
    let volume_size = request.split_size_bytes.map(|n| n as usize).filter(|&n| n < frame.len());
    let parts: Vec<(PathBuf, Vec<u8>)> = match volume_size {
        None => vec![(output.clone(), frame.clone())],
        Some(size) => {
            let slices = volume::split(&frame, size).map_err(|error| CommandError::from(error.to_string()))?;
            let count = slices.len() as u32;
            slices
                .into_iter()
                .enumerate()
                .map(|(i, bytes)| (volume_path(&output, i as u32 + 1, count), bytes))
                .collect()
        }
    };
    let volume_count = parts.len() as u32;

    // Guard every part's path before writing any of them, so a name
    // collision discovered on a later part doesn't leave earlier ones
    // already on disk.
    for (path, _) in &parts {
        if path.exists() && !request.force {
            return Err(CommandError::from(format!("{} already exists", path.display())));
        }
    }

    progress.stage("modulating");

    // Cover mode is single-channel: the cover is meant to be heard as one
    // ordinary recording, and channel-splitting produces one independent
    // carrier per channel, with no single audible signal to spread across
    // them.
    if cover_audio.is_some() {
        if let ChannelChoice::Fixed(requested) = channels_choice {
            if requested > 1 {
                return Err(CommandError::from(format!(
                    "cover audio produces a single audible carrier, so it cannot also spread \
                     the payload across {requested} channels. Drop the channel override, or \
                     drop the cover."
                )));
            }
        }
    }

    let mut written = Vec::with_capacity(parts.len());
    for (index, (path, part_frame)) in parts.iter().enumerate() {
        let channels = match (channels_choice, cover_audio.is_some()) {
            (_, true) => 1,
            (ChannelChoice::Fixed(n), false) => n,
            (ChannelChoice::Auto, false) => plan.auto_channels(part_frame.len()),
        };

        let samples = match &cover_audio {
            Some(audio) => {
                let spread = request
                    .cover
                    .as_ref()
                    .is_some_and(|opts| opts.mode == "spread");
                modem
                    .modulate_with_cover(part_frame, audio, spread)
                    .ok_or_else(|| CommandError::from("cover support was already checked".to_string()))?
            }
            None => modem.modulate_interleaved(part_frame, channels),
        };
        let pcm = to_i16(&samples);

        let volume_label = (volume_count > 1).then_some((index as u32 + 1, volume_count));
        flac_io::write_flac(
            path,
            &pcm,
            plan.sample_rate(),
            channels,
            &tags(plan, &report, &cover_tags, volume_label),
        )
        .map_err(CommandError::from)?;

        written.push(EncodedVolume {
            path: path.clone(),
            channels,
            duration_secs: (samples.len() / channels) as f64 / f64::from(plan.sample_rate()),
            carrier_bytes: 0,
        });
    }

    for volume in &mut written {
        volume.carrier_bytes = std::fs::metadata(&volume.path).map(|m| m.len()).unwrap_or(0);
    }

    let (low_hz, high_hz) = plan.band_hz();
    let total_carrier_bytes: u64 = written.iter().map(|v| v.carrier_bytes).sum();
    let total_duration: f64 = written.iter().map(|v| v.duration_secs).sum();
    let carrier_bytes = if volume_count > 1 { total_carrier_bytes } else { written[0].carrier_bytes };
    let duration = if volume_count > 1 { total_duration } else { written[0].duration_secs };

    Ok(EncodeReportDto {
        output_path: written[0].path.display().to_string(),
        plaintext_bytes: report.plaintext_len,
        compressed: report.compressed.then(|| CompressedDto {
            bytes: report.compressed_len,
            ratio: report.compression_ratio(),
        }),
        encrypted: report.encrypted,
        stored_name,
        detected_format: detected.map(FormatDto::from),
        fec_packets: report.fec_packets,
        fec_repair_percent: request.fec_overhead,
        frame_bytes: report.frame_len,
        expansion_ratio: report.expansion_ratio(),
        waveform_description: plan.describe(),
        bit_rate: plan.bit_rate(),
        band_hz: (low_hz, high_hz),
        cover_band_hz: plan.cover_band_hz(),
        channels: written[0].channels,
        channels_auto: volume_count == 1 && channels_choice == ChannelChoice::Auto && cover_audio.is_none(),
        duration_secs: duration,
        carrier_bytes,
        carrier_ratio: carrier_bytes as f64 / report.plaintext_len.max(1) as f64,
        volumes: if volume_count > 1 {
            written
                .iter()
                .enumerate()
                .map(|(i, v)| EncodedVolumeDto {
                    part: i as u32 + 1,
                    of: volume_count,
                    path: v.path.display().to_string(),
                    channels: v.channels,
                    duration_secs: v.duration_secs,
                    carrier_bytes: v.carrier_bytes,
                })
                .collect()
        } else {
            Vec::new()
        },
    })
}

/// One file this run produced, and what it cost to write.
struct EncodedVolume {
    path: PathBuf,
    channels: usize,
    duration_secs: f64,
    carrier_bytes: u64,
}

/// Metadata written into the carrier — see `audio-modem-cli`'s `encode.rs` for
/// why `AUDIOMODEM_PLAN`/`AUDIOMODEM_PROFILE` are load-bearing and therefore
/// excluded from the cover's own tags overriding anything.
fn tags(
    plan: Plan,
    report: &audio_modem_core::EncodeReport,
    cover_tags: &[(String, String)],
    volume_label: Option<(u32, u32)>,
) -> Vec<(String, String)> {
    let mut tags = vec![
        ("TITLE".to_string(), "stego-flac carrier".to_string()),
        (
            "DESCRIPTION".to_string(),
            format!(
                "stego-flac carrier, {} bytes payload, {}",
                report.plaintext_len,
                if report.encrypted {
                    "encrypted"
                } else {
                    "not encrypted"
                }
            ),
        ),
        (
            "ENCODER".to_string(),
            format!("stego-flac {}", env!("CARGO_PKG_VERSION")),
        ),
    ];

    for (key, value) in cover_tags {
        if key == PLAN_TAG || key == PROFILE_TAG {
            continue;
        }
        match tags.iter_mut().find(|(existing, _)| existing == key) {
            Some((_, existing_value)) => existing_value.clone_from(value),
            None => tags.push((key.clone(), value.clone())),
        }
    }

    tags.push((PLAN_TAG.to_string(), plan.to_plan_string()));
    for profile in audio_modem_core::Profile::ALL {
        if profile.plan() == plan {
            tags.push((PROFILE_TAG.to_string(), profile.name().to_string()));
            break;
        }
    }

    if let Some((index, count)) = volume_label {
        tags.push((VOLUME_TAG.to_string(), format!("{index}/{count}")));
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio_modem_core::{EncodeReport, Profile};

    #[test]
    fn parse_channels_accepts_auto_case_insensitively() {
        assert_eq!(parse_channels("auto").unwrap(), ChannelChoice::Auto);
        assert_eq!(parse_channels("AUTO").unwrap(), ChannelChoice::Auto);
        assert_eq!(parse_channels("Auto").unwrap(), ChannelChoice::Auto);
    }

    #[test]
    fn parse_channels_accepts_one_through_eight() {
        for n in 1..=8 {
            assert_eq!(
                parse_channels(&n.to_string()).unwrap(),
                ChannelChoice::Fixed(n)
            );
        }
    }

    #[test]
    fn parse_channels_rejects_zero_nine_and_garbage() {
        for text in ["0", "9", "many", "", "-1", "1.5"] {
            assert!(parse_channels(text).is_err(), "expected {text:?} to be rejected");
        }
    }

    fn dummy_report(encrypted: bool) -> EncodeReport {
        EncodeReport {
            plaintext_len: 100,
            compressed_len: 100,
            ciphertext_len: 100,
            fec_packets: 1,
            frame_len: 200,
            compressed: false,
            encrypted,
        }
    }

    #[test]
    fn tags_always_carries_the_load_bearing_plan_tag() {
        let plan = Profile::Dense.plan();
        let tags = tags(plan, &dummy_report(true), &[], None);
        let plan_value = tags.iter().find(|(k, _)| k == PLAN_TAG).map(|(_, v)| v.clone());
        assert_eq!(plan_value.as_deref(), Some(plan.to_plan_string().as_str()));
    }

    #[test]
    fn a_recognised_profile_gets_a_profile_tag_too() {
        let plan = Profile::Fast.plan();
        let tags = tags(plan, &dummy_report(true), &[], None);
        assert!(tags.iter().any(|(k, v)| k == PROFILE_TAG && v == "fast"));
    }

    #[test]
    fn cover_tags_cannot_override_the_load_bearing_plan_or_profile_tags() {
        let plan = Profile::Dense.plan();
        let cover_tags = vec![
            (PLAN_TAG.to_string(), "mode=fsk;forged".to_string()),
            (PROFILE_TAG.to_string(), "forged".to_string()),
            ("ARTIST".to_string(), "Someone".to_string()),
        ];
        let tags = tags(plan, &dummy_report(true), &cover_tags, None);

        let plan_value = tags.iter().find(|(k, _)| k == PLAN_TAG).unwrap().1.clone();
        assert_eq!(plan_value, plan.to_plan_string());
        let profile_value = tags.iter().find(|(k, _)| k == PROFILE_TAG).unwrap().1.clone();
        assert_eq!(profile_value, "dense");
        // A non-load-bearing cover tag still comes through.
        assert!(tags.iter().any(|(k, v)| k == "ARTIST" && v == "Someone"));
    }

    #[test]
    fn cover_tags_override_the_default_title_and_description() {
        let plan = Profile::Dense.plan();
        let cover_tags = vec![("TITLE".to_string(), "My Recording".to_string())];
        let tags = tags(plan, &dummy_report(false), &cover_tags, None);
        let title = tags.iter().find(|(k, _)| k == "TITLE").unwrap().1.clone();
        assert_eq!(title, "My Recording");
        // Only one TITLE entry, not a duplicate.
        assert_eq!(tags.iter().filter(|(k, _)| k == "TITLE").count(), 1);
    }

    #[test]
    fn a_volume_label_adds_the_volume_tag() {
        let plan = Profile::Dense.plan();
        let tags = tags(plan, &dummy_report(true), &[], Some((2, 5)));
        assert!(tags.iter().any(|(k, v)| k == VOLUME_TAG && v == "2/5"));
    }
}
