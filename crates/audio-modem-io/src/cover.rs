//! Loading cover audio for the radio-camouflage mode.
//!
//! The cover is whatever audio the user supplies — FLAC, WAV, MP3, or the audio
//! track of an MP4/M4A — reduced to a
//! single mono channel at the carrier's sample rate. It is deliberately *not*
//! made high fidelity: the modulator band-limits it to the plan's lowest
//! subcarrier up to whatever ceiling the plan reserved, so anything outside
//! that is thrown away regardless.
//!
//! The ceiling is a parameter rather than a constant because it moves: a small
//! payload can afford to hand the cover more spectrum than a large one. The
//! filter below has to track it, or a widened band would be fed audio whose top
//! octave had already been discarded.
//!
//! # Why the source is low-passed before resampling
//!
//! Rate conversion without a filter folds everything above the new Nyquist back
//! down into the audible range, and those aliases would land squarely in the
//! band the cover occupies. Low-passing near the cover's own ceiling first
//! makes the decimation harmless: there is nothing left up there to fold.
//!
//! The filter is a cascaded one-pole, which is gentle — but it only has to stop
//! aliasing being audible, since the modulator's bin copy enforces the exact
//! band edges afterwards.

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use symphonia::core::audio::GenericAudioBufferRef;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::{MetadataOptions, StandardTag};

/// Decode `path` to mono `f32` at `target_rate`, low-passed at `ceiling_hz`.
pub fn load(path: &Path, target_rate: u32, ceiling_hz: f32) -> Result<Vec<f32>> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("opening cover audio {}", path.display()))?;
    let stream = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }

    // The probe is used here, unlike the carrier reader, because the cover can
    // be any format the user happens to have.
    let mut reader = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .with_context(|| {
            format!(
                "{} is not audio this build can read (FLAC, WAV, MP3 and MP4/AAC \
                 are supported)",
                path.display()
            )
        })?;

    let track = reader
        .default_track(TrackType::Audio)
        .ok_or_else(|| anyhow!("{} has no audio track", path.display()))?
        .clone();
    let params = match track.codec_params.as_ref() {
        Some(CodecParameters::Audio(params)) => params.clone(),
        _ => bail!("{} has no usable audio parameters", path.display()),
    };

    let source_rate = params
        .sample_rate
        .ok_or_else(|| anyhow!("{} does not declare a sample rate", path.display()))?;
    let channels = params
        .channels
        .as_ref()
        .map_or(1, symphonia::core::audio::Channels::count)
        .max(1);

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|error| anyhow!("no decoder for {}: {error}", path.display()))?;

    let mut interleaved: Vec<f32> = Vec::new();
    let mut scratch: Vec<f32> = Vec::new();
    loop {
        match reader.next_packet() {
            Ok(Some(packet)) => {
                if packet.track_id != track.id {
                    continue;
                }
                match decoder.decode(&packet) {
                    Ok(decoded) => {
                        scratch.clear();
                        copy_f32(&decoded, &mut scratch);
                        interleaved.extend_from_slice(&scratch);
                    }
                    // A damaged tail is not worth failing over; the cover is
                    // decoration, and whatever decoded is enough to loop.
                    Err(_) if !interleaved.is_empty() => break,
                    Err(error) => bail!("decoding {}: {error}", path.display()),
                }
            }
            Ok(None) => break,
            Err(_) if !interleaved.is_empty() => break,
            Err(error) => bail!("reading {}: {error}", path.display()),
        }
    }

    if interleaved.is_empty() {
        bail!("{} decoded to no audio", path.display());
    }

    let mono = downmix(&interleaved, channels);
    Ok(resample(&mono, source_rate, target_rate, ceiling_hz))
}

/// Read the cover file's own tags (title, artist, album, ...) for
/// `--keep-cover-metadata`, as `KEY=value` pairs in Vorbis-comment
/// convention.
///
/// A separate probe pass rather than folding into [`load`]: tag reading never
/// touches the audio decoder, so keeping it independent means the flag this
/// exists for costs nothing when it isn't given.
pub fn read_tags(path: &Path) -> Result<Vec<(String, String)>> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("opening cover audio {}", path.display()))?;
    let stream = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }

    let mut reader = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .with_context(|| {
            format!(
                "{} is not audio this build can read (FLAC, WAV, MP3 and MP4/AAC \
                 are supported)",
                path.display()
            )
        })?;

    let mut metadata = reader.metadata();
    let tags = match metadata.skip_to_latest() {
        Some(revision) => revision
            .media
            .tags
            .iter()
            .map(|tag| (vorbis_key(tag), tag.raw.value.to_string()))
            .collect(),
        None => Vec::new(),
    };

    // symphonia-format-riff 0.6's WAV reader parses the RIFF LIST/INFO chunk
    // but then discards the result — `WavReader::try_new` hands back
    // `opts.external_data.metadata` instead of what it just parsed — so a
    // `.wav` cover never surfaces tags through `reader.metadata()` even when
    // it has them. Walked here by hand as a fallback, the same way
    // `flac_tags::read_tags` walks FLAC's own metadata blocks directly.
    if tags.is_empty() && is_wav(path) {
        return wav_info_tags(path);
    }

    Ok(tags)
}

fn is_wav(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
}

/// Hand-parsed fallback for the RIFF LIST/INFO chunk symphonia discards; see
/// [`read_tags`]. Malformed or absent chunks yield no tags rather than an
/// error, matching [`crate::flac_tags::read_tags`]'s policy for the same
/// reason: a cover's decorative metadata is not worth failing the encode over.
fn wav_info_tags(path: &Path) -> Result<Vec<(String, String)>> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("opening cover audio {}", path.display()))?;

    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Ok(Vec::new());
    }

    let mut tags = Vec::new();
    let mut pos = 12usize;

    while pos + 8 <= bytes.len() {
        let tag = &bytes[pos..pos + 4];
        let len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body_start = pos + 8;
        let Some(body_end) = body_start.checked_add(len).filter(|&end| end <= bytes.len()) else {
            break;
        };

        if tag == b"LIST" && bytes[body_start..body_end].starts_with(b"INFO") {
            parse_riff_info(&bytes[body_start + 4..body_end], &mut tags);
        }

        // Chunks are padded to an even length.
        pos = body_end + (len % 2);
    }

    Ok(tags)
}

/// Walk `iXXX`-tagged sub-chunks inside a RIFF `LIST`/`INFO` chunk's body.
fn parse_riff_info(mut info: &[u8], tags: &mut Vec<(String, String)>) {
    while info.len() >= 8 {
        let tag = &info[0..4];
        let len = u32::from_le_bytes(info[4..8].try_into().unwrap()) as usize;
        let Some(value) = info.get(8..8 + len) else {
            break;
        };

        if let Ok(text) = std::str::from_utf8(value) {
            let text = text.trim_end_matches('\0').trim();
            if !text.is_empty() {
                tags.push((riff_info_key(tag), text.to_string()));
            }
        }

        info = &info[(8 + len + len % 2).min(info.len())..];
    }
}

/// Map a RIFF `INFO` four-character code to a Vorbis-comment field name,
/// covering the common subset also handled by [`vorbis_key`]. An unrecognised
/// code is kept verbatim, uppercased, rather than dropped.
fn riff_info_key(tag: &[u8]) -> String {
    match tag {
        b"INAM" => "TITLE",
        b"IART" => "ARTIST",
        b"IPRD" => "ALBUM",
        b"IGNR" | b"ISGN" => "GENRE",
        b"ICMT" => "COMMENT",
        b"IMUS" => "COMPOSER",
        b"ICRD" | b"IDIT" => "DATE",
        b"ITRK" | b"IPRT" => "TRACKNUMBER",
        _ => return String::from_utf8_lossy(tag).to_ascii_uppercase(),
    }
    .to_string()
}

/// Map a decoded tag to the Vorbis-comment field name a FLAC player expects,
/// for the handful of fields common across ID3, MP4 and Vorbis tagging. An
/// unrecognised field keeps its format-native key rather than being dropped,
/// so `--keep-cover-metadata` still carries it through, just less tidily.
fn vorbis_key(tag: &symphonia::core::meta::Tag) -> String {
    let mapped = match &tag.std {
        Some(StandardTag::TrackTitle(_)) => "TITLE",
        Some(StandardTag::Artist(_)) => "ARTIST",
        Some(StandardTag::Album(_)) => "ALBUM",
        Some(StandardTag::AlbumArtist(_)) => "ALBUMARTIST",
        Some(StandardTag::Genre(_)) => "GENRE",
        Some(StandardTag::Comment(_)) => "COMMENT",
        Some(StandardTag::Composer(_)) => "COMPOSER",
        Some(StandardTag::TrackNumber(_)) => "TRACKNUMBER",
        Some(StandardTag::DiscNumber(_)) => "DISCNUMBER",
        Some(StandardTag::ReleaseDate(_) | StandardTag::RecordingDate(_)) => "DATE",
        _ => return tag.raw.key.to_ascii_uppercase(),
    };
    mapped.to_string()
}

/// Average the channels together.
fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Low-pass near the cover ceiling, then linearly resample.
fn resample(mono: &[f32], from: u32, to: u32, ceiling_hz: f32) -> Vec<f32> {
    let filtered = low_pass(mono, from as f32, ceiling_hz);
    // `from == 0` would otherwise divide the ratio to zero and blow the output
    // length up towards `usize::MAX` below -- guard it here rather than let a
    // malformed source file (a declared-but-bogus sample rate) turn into a
    // multi-exabyte allocation attempt.
    if from == to || from == 0 || filtered.is_empty() {
        return filtered;
    }

    let ratio = from as f64 / to as f64;
    let out_len = (filtered.len() as f64 / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let position = i as f64 * ratio;
            let index = position as usize;
            let fraction = (position - index as f64) as f32;
            let a = filtered[index.min(filtered.len() - 1)];
            let b = filtered[(index + 1).min(filtered.len() - 1)];
            a + (b - a) * fraction
        })
        .collect()
}

/// Two cascaded one-pole low-pass sections.
fn low_pass(samples: &[f32], sample_rate: f32, cutoff_hz: f32) -> Vec<f32> {
    if sample_rate <= 0.0 || cutoff_hz >= sample_rate / 2.0 {
        return samples.to_vec();
    }
    let dt = 1.0 / sample_rate;
    let rc = 1.0 / (std::f32::consts::TAU * cutoff_hz);
    let alpha = dt / (rc + dt);

    let mut out = samples.to_vec();
    for _ in 0..2 {
        let mut state = 0.0f32;
        for value in &mut out {
            state += alpha * (*value - state);
            *value = state;
        }
    }
    out
}

fn copy_f32(buffer: &GenericAudioBufferRef<'_>, dst: &mut Vec<f32>) {
    buffer.copy_to_vec_interleaved(dst);
}
