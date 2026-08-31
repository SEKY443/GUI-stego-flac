//! `stego-flac encode`

use std::fs;
use std::io::Read;
use std::path::PathBuf;

use anyhow::bail;
use anyhow::{Context, Result};
use audio_modem_core::frame::volume;
use audio_modem_core::modem::ofdm::COVER_TELEPHONE_HZ;
use audio_modem_core::{encode_frame, format, to_i16, Carrier, EncodeParams, KdfParams, Plan};
use audio_modem_io::flac_tags::{PLAN_TAG, PROFILE_TAG, VOLUME_TAG};
use audio_modem_io::{cover, flac_io, volume_path};
use serde_json::json;

use crate::cli::{ChannelChoice, CoverMode, EncodeArgs};
use crate::commands::{guard_output, human_bytes, human_duration, Stage};
use crate::passphrase;

pub fn run(args: &EncodeArgs) -> Result<()> {
    let mut plan = args.plan.resolve(None)?;

    // Check the waveform can carry a cover at all before anything expensive
    // happens — a passphrase prompt and a multi-minute encode are both worse
    // places to discover an FSK plan has no spectrum to partition. The band
    // itself is chosen later, once the frame size is known.
    if args.cover.is_some() && plan.set_cover_ceiling(COVER_TELEPHONE_HZ, 0.0).is_none() {
        bail!(
            "--cover needs a waveform with a spectrum to partition; {} lights one \
             tone at a time. Use an OFDM profile (dense or compact).",
            plan.describe()
        );
    }
    let output = args.output_path()?;
    guard_output(&output, args.force)?;

    let plaintext = if args.reads_stdin() {
        let mut buffer = Vec::new();
        std::io::stdin()
            .read_to_end(&mut buffer)
            .context("reading standard input")?;
        buffer
    } else {
        fs::read(&args.input).with_context(|| format!("reading {}", args.input.display()))?
    };

    // Warn before the passphrase prompt, not after: this is the last moment the
    // user can cancel without having typed a secret, and a multi-hour encode is
    // worth interrupting early.
    let estimated = plaintext.len() as f64 * 8.0 / plan.bit_rate();
    if estimated > 600.0 {
        eprintln!(
            "note: {} uncompressed would be ~{} of audio at {:.0} bit/s.",
            human_bytes(plaintext.len() as u64),
            human_duration(estimated),
            plan.bit_rate()
        );
        eprintln!("      compression usually cuts this a lot; `--profile fast` halves it.");
    }

    // Confirm on encode: a mistyped write-side passphrase produces a carrier
    // that nobody, including its author, can ever open.
    let secret = if args.no_encrypt {
        None
    } else {
        Some(passphrase::acquire(args.passphrase_file.as_deref(), true)?)
    };

    let params = EncodeParams {
        compression_level: args.level as i32,
        passphrase: secret.as_ref().map(|s| s.as_slice()),
        kdf: KdfParams::default(),
        fec: args.fec_params(),
        store_format: !args.no_store_name,
        store_timestamp: !args.no_store_name,
    };

    // A payload piped in has no filename of its own, so one is synthesised from
    // whatever the content turns out to be: `payload.pdf` rather than nothing.
    let detected = format::detect(&plaintext);
    let stored_name = args.stored_name().or_else(|| {
        (!args.no_store_name && args.reads_stdin()).then(|| format::default_name(detected))
    });

    let stage = Stage::new(args.output_args.json);
    stage.begin("compressing and encrypting");
    let (frame, report) = encode_frame(&plaintext, stored_name.as_deref(), &params)?;

    // The band is settled here rather than up front because `auto` keys off the
    // frame — post-compression, post-encryption, post-FEC — which is what
    // actually becomes audio. A 40 MB text file that zstd takes down to 6 MB
    // should get the wide band its carrier can afford, not the one its
    // uncompressed size suggests.
    let cover_audio = match &args.cover {
        Some(path) => {
            let ceiling = match args.cover_quality.ceiling_hz() {
                Some(fixed) => plan.set_cover_ceiling(fixed, f64::from(args.cover_attenuation)),
                None => plan.set_auto_cover(frame.len(), f64::from(args.cover_attenuation)),
            }
            .expect("cover support was checked before the frame was built");
            plan.validate()
                .map_err(|error| anyhow::anyhow!("cover band does not fit this plan: {error}"))?;

            stage.begin("loading cover audio");
            // The loader's anti-alias filter has to track the band, otherwise a
            // wider band would be handed audio that was already thrown away and
            // the extra subcarriers would carry nothing but silence.
            Some(cover::load(path, plan.sample_rate(), ceiling as f32)?)
        }
        None => None,
    };

    let cover_tags = if args.keep_cover_metadata {
        let path = args
            .cover
            .as_deref()
            .expect("--keep-cover-metadata requires --cover");
        cover::read_tags(path)?
    } else {
        Vec::new()
    };

    // Deciding the stride needs both lengths, so it happens after the frame
    // exists and the cover is loaded — and before the modem is built, because
    // the stride is part of the plan the metadata records.
    if let (CoverMode::Spread, Some(audio)) = (args.cover_mode, &cover_audio) {
        let symbols = plan.symbols_for(frame.len());
        let cover_symbols = audio.len() / 512;
        if symbols > 0 && cover_symbols > symbols {
            plan.set_spread(cover_symbols / symbols);
        }
    }
    let plan = plan;
    let modem = Carrier::new(plan).context("building the modem")?;

    // Splitting only ever applies to the plain, uncovered path: --split-size
    // and --cover are mutually exclusive (see `EncodeArgs::split_size`), so
    // `cover_audio` is always `None` whenever more than one part is written.
    // A requested size at or above the finished frame collapses to a single
    // part, written under the ordinary `output` path with no `.partI-of-N`
    // suffix -- indistinguishable from never having passed --split-size.
    let volume_size = args.split_size.filter(|size| size.0 < frame.len());
    let parts: Vec<(PathBuf, Vec<u8>)> = match volume_size {
        None => vec![(output.clone(), frame.clone())],
        Some(size) => {
            let slices = volume::split(&frame, size.0)?;
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
        guard_output(path, args.force)?;
    }

    stage.begin("modulating");
    // Cover mode is single-channel. The cover is meant to be heard as an
    // ordinary recording, and the lane splitting used for extra channels
    // produces one independent carrier per channel -- there is no sensible
    // single audible signal to spread across them.
    if cover_audio.is_some() {
        if let ChannelChoice::Fixed(requested) = args.channels {
            if requested > 1 {
                bail!(
                    "--cover produces a single audible carrier, so it cannot also spread \
                     the payload across {requested} channels. Drop --channels, or drop \
                     --cover."
                );
            }
        }
    }

    let mut written = Vec::with_capacity(parts.len());
    for (index, (path, part_frame)) in parts.iter().enumerate() {
        let channels = match (args.channels, cover_audio.is_some()) {
            (_, true) => 1,
            (ChannelChoice::Fixed(n), false) => n,
            (ChannelChoice::Auto, false) => plan.auto_channels(part_frame.len()),
        };
        let samples = match &cover_audio {
            Some(audio) => modem
                .modulate_with_cover(part_frame, audio, args.cover_mode == CoverMode::Spread)
                .expect("cover support was checked when the plan was resolved"),
            None => modem.modulate_interleaved(part_frame, channels),
        };
        let pcm = to_i16(&samples);

        let volume_label = (volume_count > 1).then_some((index as u32 + 1, volume_count));
        flac_io::write_flac(
            path,
            &pcm,
            plan.sample_rate(),
            channels,
            &tags(plan, &report_title(&report, volume_label), &cover_tags, volume_label),
        )?;

        written.push(WrittenVolume {
            path: path.clone(),
            channels,
            duration_secs: (samples.len() / channels) as f64 / f64::from(plan.sample_rate()),
            carrier_bytes: 0,
        });
    }
    stage.done();

    for volume in &mut written {
        volume.carrier_bytes = fs::metadata(&volume.path).map(|m| m.len()).unwrap_or(0);
    }
    let total_carrier_bytes: u64 = written.iter().map(|v| v.carrier_bytes).sum();

    if args.output_args.json {
        let (low_hz, high_hz) = plan.band_hz();
        let out = json!({
            "output_path": (volume_count == 1).then(|| written[0].path.display().to_string()),
            "plaintext_bytes": report.plaintext_len,
            "compressed": report.compressed.then(|| json!({
                "bytes": report.compressed_len,
                "ratio": report.compression_ratio(),
            })),
            "encrypted": report.encrypted,
            "stored_name": stored_name,
            "detected_format": detected.map(crate::commands::format_to_json),
            "fec": {
                "packets": report.fec_packets,
                "repair_percent": args.fec_overhead,
            },
            "frame_bytes": report.frame_len,
            "expansion_ratio": report.expansion_ratio(),
            "waveform": {
                "description": plan.describe(),
                "bit_rate": plan.bit_rate(),
                "band_hz": [low_hz, high_hz],
            },
            "cover": plan.cover_band_hz().map(|(low, high)| json!({
                "band_hz": [low, high],
                "attenuation_db": args.cover_attenuation,
                "metadata_kept": args.keep_cover_metadata,
            })),
            "channels": (volume_count == 1).then_some(written[0].channels),
            "channels_auto": volume_count == 1
                && args.channels == ChannelChoice::Auto
                && cover_audio.is_none(),
            "duration_secs": (volume_count == 1).then_some(written[0].duration_secs),
            "carrier_bytes": (volume_count == 1).then_some(written[0].carrier_bytes),
            "carrier_ratio": (volume_count == 1)
                .then(|| written[0].carrier_bytes as f64 / report.plaintext_len.max(1) as f64),
            "split": (volume_count > 1).then(|| json!({
                "volume_count": volume_count,
                "requested_volume_size_bytes": volume_size.map(|s| s.0),
                "total_carrier_bytes": total_carrier_bytes,
                "total_carrier_ratio":
                    total_carrier_bytes as f64 / report.plaintext_len.max(1) as f64,
                "volumes": written.iter().enumerate().map(|(i, v)| json!({
                    "part": i as u32 + 1,
                    "of": volume_count,
                    "path": v.path.display().to_string(),
                    "channels": v.channels,
                    "duration_secs": v.duration_secs,
                    "carrier_bytes": v.carrier_bytes,
                })).collect::<Vec<_>>(),
            })),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        if !report.encrypted {
            eprintln!("warning: payload is not encrypted; anyone with this file can read it");
        }
        return Ok(());
    }

    if args.quiet {
        if !report.encrypted {
            eprintln!("warning: payload is not encrypted; anyone with this file can read it");
        }
        return Ok(());
    }

    if volume_count == 1 {
        println!("wrote {}", written[0].path.display());
    } else {
        println!("wrote {volume_count} volumes:");
        for (i, volume) in written.iter().enumerate() {
            println!(
                "  {}/{volume_count}  {}  ({}, {}, {} ch)",
                i + 1,
                volume.path.display(),
                human_bytes(volume.carrier_bytes),
                human_duration(volume.duration_secs),
                volume.channels
            );
        }
    }
    println!();
    println!(
        "  input              {}",
        human_bytes(report.plaintext_len as u64)
    );
    if report.compressed {
        println!(
            "  compressed         {} ({:.1}% of original)",
            human_bytes(report.compressed_len as u64),
            report.compression_ratio() * 100.0
        );
    } else {
        println!("  compressed         skipped (input is incompressible)");
    }
    println!(
        "  encrypted          {}",
        if report.encrypted {
            "AES-256-GCM, Argon2id key"
        } else {
            "no (--no-encrypt)"
        }
    );
    println!(
        "  filename stored    {}",
        match &stored_name {
            Some(name) => name.as_str(),
            None => "no",
        }
    );
    println!(
        "  format detected    {}",
        match (detected, args.no_store_name) {
            (_, true) => "not stored".to_string(),
            (Some(format), _) => format.description.to_string(),
            (None, _) => "unrecognised".to_string(),
        }
    );
    println!(
        "  fec                {} RaptorQ packets ({}% repair)",
        report.fec_packets, args.fec_overhead
    );
    println!(
        "  frame              {} ({:.2}x plaintext)",
        human_bytes(report.frame_len as u64),
        report.expansion_ratio()
    );
    let (low_hz, high_hz) = plan.band_hz();
    println!(
        "  waveform           {} ({:.0} bit/s, {:.0}-{:.0} Hz)",
        plan.describe(),
        plan.bit_rate(),
        low_hz,
        high_hz
    );
    if let Some((low, high)) = plan.cover_band_hz() {
        println!(
            "  cover audio        {:.0}-{:.0} Hz, data {:.0} dB below{}",
            low,
            high,
            args.cover_attenuation,
            if args.keep_cover_metadata {
                ", tags kept"
            } else {
                ""
            }
        );
    }
    if volume_count > 1 {
        println!(
            "  split              {volume_count} volumes, {} requested per part",
            human_bytes(volume_size.map(|s| s.0 as u64).unwrap_or(0))
        );
    }
    let total_duration: f64 = written.iter().map(|v| v.duration_secs).sum();
    println!(
        "  carrier            {}{}",
        human_duration(total_duration),
        if volume_count == 1 && written[0].channels > 1 {
            format!(
                " across {} channels{}",
                written[0].channels,
                if args.channels == ChannelChoice::Auto {
                    " (auto)"
                } else {
                    ""
                }
            )
        } else {
            String::new()
        }
    );
    if volume_count > 1 {
        println!(
            "  flac files (total) {} ({:.2}x plaintext)",
            human_bytes(total_carrier_bytes),
            total_carrier_bytes as f64 / report.plaintext_len.max(1) as f64
        );
    } else {
        println!(
            "  flac file          {} ({:.2}x plaintext)",
            human_bytes(total_carrier_bytes),
            total_carrier_bytes as f64 / report.plaintext_len.max(1) as f64
        );
    }

    if !report.encrypted {
        eprintln!();
        eprintln!("warning: payload is not encrypted; anyone with this file can read it");
    }

    Ok(())
}

/// One file this run produced, and what it cost to write.
struct WrittenVolume {
    path: PathBuf,
    channels: usize,
    duration_secs: f64,
    carrier_bytes: u64,
}

/// Human-readable one-liner for the DESCRIPTION tag.
///
/// `volume_label`, when this file is one part of a split archive, appends its
/// position so the tag alone identifies the part even if the filename is
/// later changed.
fn report_title(report: &audio_modem_core::EncodeReport, volume_label: Option<(u32, u32)>) -> String {
    let base = format!(
        "stego-flac carrier, {} payload, {}",
        human_bytes(report.plaintext_len as u64),
        if report.encrypted {
            "encrypted"
        } else {
            "not encrypted"
        }
    );
    match volume_label {
        Some((index, count)) => format!("{base}, part {index}/{count}"),
        None => base,
    }
}

/// Metadata written into the carrier.
///
/// `AUDIOMODEM_PLAN` is the load-bearing one: it is what lets `decode` and
/// `info` configure themselves. The rest exist so the file looks like a real
/// tagged audio file in a player or library scanner instead of an untitled
/// blob. `cover_tags` — the cover audio's own tags, when
/// `--keep-cover-metadata` was given — override the TITLE/DESCRIPTION
/// placeholders on a matching key, since a carrier that inherits the cover's
/// real metadata is a better disguise than one stamped "stego-flac carrier".
/// `AUDIOMODEM_PLAN`/`AUDIOMODEM_PROFILE` are excluded from that override:
/// they are load-bearing, and a cover file has no business setting them,
/// whatever it happens to be tagged with.
fn tags(
    plan: Plan,
    description: &str,
    cover_tags: &[(String, String)],
    volume_label: Option<(u32, u32)>,
) -> Vec<(String, String)> {
    let mut tags = vec![
        ("TITLE".to_string(), "stego-flac carrier".to_string()),
        ("DESCRIPTION".to_string(), description.to_string()),
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

    // Record the preset name when the plan matches one exactly, purely so
    // `info` can say "fast" rather than reciting five numbers.
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
