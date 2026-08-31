//! `stego-flac decode`

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use audio_modem_core::frame::volume::{self, VolumeHeader};
use audio_modem_core::{decode_frame, Carrier, Header};
use audio_modem_io::{flac_io, join_volume_set, prepare_samples};
use serde_json::json;

use crate::cli::{is_stream, DecodeArgs};
use crate::commands::{format_to_json, guard_output, human_bytes, print_warnings, Stage};
use crate::passphrase;

pub fn run(args: &DecodeArgs) -> Result<()> {
    let raw = fs::read(&args.input).with_context(|| format!("reading {}", args.input.display()))?;

    // The tone plan comes from the carrier itself unless the user overrode it.
    let recorded = audio_modem_io::plan_from_tags(&raw, args.plan.is_explicit(), |msg| {
        eprintln!("note: {msg}")
    });
    let plan = args.plan.resolve(recorded)?;
    let modem = Carrier::new(plan).context("building the modem")?;

    let stage = Stage::new(args.output_args.json);
    stage.begin("decoding FLAC");
    let audio = flac_io::read_flac(&args.input)?;
    print_warnings(&audio.warnings);
    let samples = prepare_samples(&audio, &modem, &args.input)?;

    stage.begin("demodulating");
    // The channel count comes from the container's own header, so nothing has
    // to be recorded in the metadata or remembered by the reader.
    let piece = modem.demodulate_interleaved(&samples, audio.channels)?;

    // A plain frame starts "AMDM" and is decoded as-is; a volume starts
    // "AMVL" and means `args.input` is only one part of a split archive. The
    // rest are found by filename, demodulated the same way, and joined back
    // into the single frame `decode_frame` expects -- from here on nothing
    // below has to know splitting happened.
    let (frame, volumes_joined) = if piece.len() >= 4 && piece[0..4] == volume::VOLUME_MAGIC {
        stage.begin("locating and joining volumes");
        let count = VolumeHeader::parse(&piece).map(|h| h.volume_count).ok();
        (join_volume_set(&args.input, piece, &modem)?, count)
    } else {
        (piece, None)
    };
    stage.done();

    // Read the header before asking for a passphrase, so a tone-plan mismatch
    // fails with a clear message rather than after the user has typed a secret.
    let header = Header::parse(&frame).with_context(|| {
        if recorded.is_some() {
            "the carrier's recorded tone plan did not demodulate to a valid frame"
        } else {
            "no tone plan found in the carrier's metadata; if it was encoded with \
             non-default settings, pass the same flags used to encode it"
        }
    })?;

    let secret = if header.is_encrypted() {
        Some(passphrase::acquire(args.passphrase_file.as_deref(), false)?)
    } else {
        if args.passphrase_file.is_some() {
            bail!("this carrier is not encrypted, but --passphrase-file was given");
        }
        None
    };

    let stage = Stage::new(args.output_args.json);
    stage.begin("decrypting and decompressing");
    let payload = decode_frame(&frame, secret.as_ref().map(|s| s.as_slice()))?;
    stage.done();

    // Writing the payload to standard output makes stdout a data channel, so
    // the summary -- JSON or text -- moves to stderr rather than corrupting
    // whatever is piped.
    let to_stdout = args.output.as_deref().is_some_and(is_stream);
    if to_stdout {
        std::io::stdout()
            .write_all(&payload.data)
            .context("writing standard output")?;
        if args.output_args.json {
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&summary(&payload, None, volumes_joined))?
            );
        } else if !args.quiet {
            eprintln!(
                "recovered {}{}",
                human_bytes(payload.data.len() as u64),
                volume_note(volumes_joined)
            );
        }
        return Ok(());
    }

    let output = resolve_output(args, payload.suggested_name())?;
    guard_output(&output, args.force)?;

    fs::write(&output, &payload.data).with_context(|| format!("writing {}", output.display()))?;

    if args.output_args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&summary(&payload, Some(&output), volumes_joined))?
        );
    } else if !args.quiet {
        println!(
            "recovered {} to {}{}{}",
            human_bytes(payload.data.len() as u64),
            output.display(),
            match payload.format {
                Some(format) => format!(" ({})", format.description),
                None => String::new(),
            },
            volume_note(volumes_joined)
        );
    }

    Ok(())
}

/// Text-mode aside noting how many volumes were joined, or nothing at all
/// for an ordinary single-file carrier.
fn volume_note(volumes_joined: Option<u32>) -> String {
    match volumes_joined {
        Some(count) => format!(" (reassembled from {count} volumes)"),
        None => String::new(),
    }
}

/// Build the machine-readable decode summary.
///
/// `output` is `None` when the payload went to standard output instead of a
/// file. `volumes_joined` is the part count when `input` was one part of a
/// split archive that `decode` located and reassembled on its own.
fn summary(
    payload: &audio_modem_core::DecodedPayload,
    output: Option<&Path>,
    volumes_joined: Option<u32>,
) -> serde_json::Value {
    json!({
        "output_path": output.map(|path| path.display().to_string()),
        "recovered_bytes": payload.data.len(),
        "name": payload.name,
        "format": payload.format.map(format_to_json),
        "encoded_at_unix": payload.encoded_at,
        "volumes_joined": volumes_joined,
    })
}

/// Decide where to write the recovered file.
///
/// `-o` wins. Otherwise the name stored inside the encrypted payload is used
/// verbatim, with any directory component stripped. A carrier is untrusted
/// input, and a stored name like `../../.ssh/authorized_keys` must not be able
/// to steer the write anywhere outside the working directory.
fn resolve_output(args: &DecodeArgs, stored: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = &args.output {
        return Ok(path.clone());
    }

    let Some(stored) = stored else {
        bail!("this carrier does not store a filename; pass -o to say where to write the payload");
    };

    let Some(safe) = audio_modem_io::sanitize_stored_name(stored) else {
        bail!("the carrier stores an unusable filename ({stored:?}); pass -o instead");
    };

    if safe != stored {
        eprintln!("note: stored name {stored:?} was reduced to {safe:?}");
    }

    Ok(PathBuf::from(safe))
}
