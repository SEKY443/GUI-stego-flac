//! Multi-volume carrier splitting: filename convention, and the shared
//! demodulate/join path used by both the CLI's `--split-size` and the GUI's
//! equivalent.
//!
//! `audio_modem_core::frame::volume` only knows about byte slices — it splits
//! and joins a `Vec<u8>` frame. Turning that into files on disk needs a naming
//! convention two independent runs can agree on (`decode` must find the files
//! `encode` wrote), and needs to read and validate each sibling file the same
//! way a plain carrier is read. That plumbing lives here once, rather than
//! once per frontend.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use audio_modem_core::frame::volume::{self, VolumeHeader};
use audio_modem_core::{from_i16, Carrier};

use crate::flac_io::{self, FlacAudio};

/// Derive the path for volume `index` (1-based) of `count`, inserting
/// `.partI-of-N` before `base`'s extension.
///
/// `count` is zero-padded to its own digit width, so parts sort correctly in
/// a directory listing past nine volumes. The marker this produces is exact
/// and reversed by [`strip_volume_suffix`] rather than re-derived by parsing,
/// so the two stay in lockstep by construction.
pub fn volume_path(base: &Path, index: u32, count: u32) -> PathBuf {
    let width = count.to_string().len();
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let suffix = base
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let name = format!("{stem}.part{index:0width$}-of-{count}{suffix}");
    match base.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
        _ => PathBuf::from(name),
    }
}

/// Reverse [`volume_path`]: given a path known to be volume `index` of
/// `count`, recover the base path it was derived from.
///
/// Returns `None` if the filename does not contain the exact `.partI-of-N`
/// marker those two numbers imply — the file was renamed after encoding, or
/// never belonged to this archive.
pub fn strip_volume_suffix(path: &Path, index: u32, count: u32) -> Option<PathBuf> {
    let width = count.to_string().len();
    let marker = format!(".part{index:0width$}-of-{count}");
    let name = path.file_name()?.to_str()?;
    let pos = name.rfind(&marker)?;
    let mut base_name = String::with_capacity(name.len() - marker.len());
    base_name.push_str(&name[..pos]);
    base_name.push_str(&name[pos + marker.len()..]);
    Some(path.with_file_name(base_name))
}

/// Validate a decoded FLAC container against the tone plan and trim to a
/// decodable length.
///
/// Shared by every reader of a stego-flac carrier — a plain single-file
/// carrier or one volume of a split archive — so the channel/sample-rate
/// checks and the alignment trim only exist in one place.
pub fn prepare_samples(audio: &FlacAudio, modem: &Carrier, path: &Path) -> Result<Vec<f32>> {
    let plan = modem.plan();
    if audio.channels == 0 || audio.channels > 8 {
        bail!(
            "{} declares {} channels; stego-flac carriers use 1 to 8",
            path.display(),
            audio.channels
        );
    }

    if audio.sample_rate != plan.sample_rate() {
        bail!(
            "{} is {} Hz but the tone plan expects {} Hz; pass the matching sample rate \
             (every tone frequency is defined relative to the sample rate, so a \
             mismatch shifts the whole plan)",
            path.display(),
            audio.sample_rate,
            plan.sample_rate()
        );
    }

    // Trim to a whole number of bytes' worth of symbols. A FLAC encoder is free
    // to pad its final block, and a truncated carrier is exactly the case FEC
    // exists to survive, so a ragged tail is discarded rather than rejected.
    // Interleaved samples must divide evenly into whole frames *and* leave each
    // lane a whole number of symbols.
    let group = modem.alignment_samples() * audio.channels;
    let usable = audio.samples.len() - audio.samples.len() % group;

    if usable == 0 {
        bail!(
            "{} holds {} samples, fewer than one symbol group",
            path.display(),
            audio.samples.len()
        );
    }

    // The remainder is discarded silently. It is always benign: the encoder
    // pads the carrier to a whole number of FLAC blocks, and the frame header
    // declares the payload length, so trailing samples are never part of the
    // message. Warning about them would fire on almost every successful decode.
    Ok(from_i16(&audio.samples[..usable]))
}

/// Demodulate one FLAC file's stego-flac frame in isolation.
///
/// Used both for a plain single-file carrier and for each sibling of a split
/// archive, which is why it re-reads and re-validates the container from
/// scratch rather than reusing anything from the caller.
pub fn demodulate_file(path: &Path, modem: &Carrier) -> Result<Vec<u8>> {
    let audio = flac_io::read_flac(path)?;
    let samples = prepare_samples(&audio, modem, path)?;
    Ok(modem.demodulate_interleaved(&samples, audio.channels)?)
}

/// Locate this archive's sibling volumes by filename, demodulate each, and
/// reassemble the frame they together carry.
///
/// `primary_bytes` is the already-demodulated content of `primary_path`,
/// confirmed by the caller to start with the volume magic. Siblings are found
/// by inverting the `.partI-of-N` naming [`volume_path`] writes, so a part is
/// only ever missed when it was renamed or moved after encoding — in which
/// case the error explains exactly that, rather than a generic "file not
/// found".
pub fn join_volume_set(primary_path: &Path, primary_bytes: Vec<u8>, modem: &Carrier) -> Result<Vec<u8>> {
    let header = VolumeHeader::parse(&primary_bytes)
        .context("this carrier's demodulated header looks like a volume but failed to parse")?;
    let index = header.volume_index + 1;
    let count = header.volume_count;

    let base = strip_volume_suffix(primary_path, index, count).ok_or_else(|| {
        anyhow!(
            "{} is part {index} of {count} in a split archive, but its filename doesn't \
             follow the `<name>.part{index}-of-{count}<ext>` convention volume splitting \
             writes, so the other parts cannot be located automatically. Restore the original \
             filename, or place all {count} parts together with names that follow it.",
            primary_path.display()
        )
    })?;

    let mut missing = Vec::new();
    let mut volumes = Vec::with_capacity(count as usize);

    for i in 1..=count {
        let (bytes, path) = if i == index {
            (primary_bytes.clone(), primary_path.to_path_buf())
        } else {
            let path = volume_path(&base, i, count);
            if !path.exists() {
                missing.push(path);
                continue;
            }
            let bytes = demodulate_file(&path, modem)
                .with_context(|| format!("reading volume {i}/{count} ({})", path.display()))?;
            (bytes, path)
        };

        let vol_header = VolumeHeader::parse(&bytes)
            .with_context(|| format!("volume {i}/{count} ({})", path.display()))?;
        if vol_header.archive_id != header.archive_id {
            bail!(
                "{} claims to be part of a different archive ({:016x} instead of {:016x}); \
                 these parts do not belong together",
                path.display(),
                vol_header.archive_id,
                header.archive_id
            );
        }
        if vol_header.volume_index + 1 != i {
            bail!(
                "{} is named as part {i} of {count} but its own header says it is part {}; \
                 the files may have been renamed or mixed up",
                path.display(),
                vol_header.volume_index + 1
            );
        }

        let after_header = &bytes[volume::VOLUME_HEADER_LEN..];
        let declared = usize::try_from(vol_header.volume_len).unwrap_or(usize::MAX);
        if after_header.len() < declared {
            bail!(
                "volume {i}/{count} ({}) is truncated: declares {declared} bytes but only {} \
                 were recovered",
                path.display(),
                after_header.len()
            );
        }
        let payload = &after_header[..declared];
        if !vol_header.verify_payload(payload) {
            bail!(
                "volume {i}/{count} ({}) failed its integrity check; this part is corrupt",
                path.display()
            );
        }

        volumes.push((vol_header, payload.to_vec()));
    }

    if !missing.is_empty() {
        bail!(
            "{count}-part split archive is missing {} part(s): {}",
            missing.len(),
            missing
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    Ok(volume::join(volumes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_path_inserts_marker_before_extension() {
        let path = volume_path(Path::new("/tmp/song.flac"), 2, 10);
        assert_eq!(path, PathBuf::from("/tmp/song.part02-of-10.flac"));
    }

    #[test]
    fn volume_path_handles_no_extension_and_no_parent() {
        let path = volume_path(Path::new("song"), 1, 3);
        assert_eq!(path, PathBuf::from("song.part1-of-3"));
    }

    #[test]
    fn strip_volume_suffix_reverses_volume_path() {
        let base = Path::new("/tmp/song.flac");
        let derived = volume_path(base, 3, 12);
        assert_eq!(strip_volume_suffix(&derived, 3, 12), Some(base.to_path_buf()));
    }

    #[test]
    fn strip_volume_suffix_rejects_a_renamed_file() {
        assert_eq!(strip_volume_suffix(Path::new("/tmp/renamed.flac"), 1, 2), None);
    }

    #[test]
    fn strip_volume_suffix_rejects_a_mismatched_index() {
        let derived = volume_path(Path::new("/tmp/song.flac"), 3, 12);
        assert_eq!(strip_volume_suffix(&derived, 4, 12), None);
    }
}
