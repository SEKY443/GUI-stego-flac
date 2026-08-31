//! End-to-end tests through the same functions the frontend calls over IPC
//! (`encode_blocking`/`decode_blocking`/`inspect`), rather than through
//! `audio-modem-core` directly. `audio-modem-core` and the CLI already prove
//! the pipeline itself is correct; what only exists in this crate is the
//! DTO <-> `Plan`/`EncodeParams` mapping, cover/channel option wiring, and
//! output-path resolution, so that is what these exercise.

use std::path::PathBuf;

use audio_modem_core::codec::compress::DEFAULT_LEVEL;
use audio_modem_core::codec::fec::{DEFAULT_REPAIR_PERCENT, DEFAULT_SYMBOL_SIZE};
use audio_modem_io::NoopProgress;

use super::decode::{decode_blocking, DecodeRequest};
use super::encode::{encode_blocking, CoverOptions, EncodeRequest};
use super::info::inspect;
use super::plan::PlanArgsDto;
use super::test_support::{incompressible, tone_wav, TempDir};

fn base_encode_request(input: PathBuf, output: PathBuf) -> EncodeRequest {
    EncodeRequest {
        input_path: input.display().to_string(),
        output_path: output.display().to_string(),
        passphrase: None,
        name: None,
        no_store_name: false,
        level: DEFAULT_LEVEL,
        fec_overhead: DEFAULT_REPAIR_PERCENT,
        fec_symbol_size: DEFAULT_SYMBOL_SIZE,
        channels: "auto".to_string(),
        cover: None,
        split_size_bytes: None,
        plan: PlanArgsDto::default(),
        force: false,
    }
}

fn base_decode_request(input: PathBuf, output: Option<PathBuf>) -> DecodeRequest {
    DecodeRequest {
        input_path: input.display().to_string(),
        output_path: output.map(|p| p.display().to_string()),
        passphrase: None,
        force: false,
        plan: PlanArgsDto::default(),
    }
}

#[test]
fn plain_round_trip_without_encryption() {
    let dir = TempDir::new("plain");
    let input = dir.join("secret.txt");
    let carrier = dir.join("carrier.flac");
    let landing = dir.join("out.txt");
    std::fs::write(&input, b"the quick brown fox jumps over the lazy dog").unwrap();

    let report = encode_blocking(&NoopProgress, base_encode_request(input.clone(), carrier.clone()))
        .expect("encode");
    assert!(!report.encrypted);
    assert_eq!(report.stored_name.as_deref(), Some("secret.txt"));

    let decoded = decode_blocking(
        &NoopProgress,
        base_decode_request(carrier, Some(landing.clone())),
    )
    .expect("decode");
    assert_eq!(decoded.name.as_deref(), Some("secret.txt"));
    assert_eq!(
        std::fs::read(&landing).unwrap(),
        b"the quick brown fox jumps over the lazy dog"
    );
}

#[test]
fn round_trip_with_a_passphrase() {
    let dir = TempDir::new("passphrase");
    let input = dir.join("secret.txt");
    let carrier = dir.join("carrier.flac");
    let landing = dir.join("out.txt");
    std::fs::write(&input, b"only for those who know the words").unwrap();

    let mut encode_request = base_encode_request(input, carrier.clone());
    encode_request.passphrase = Some("correct horse battery staple".to_string());
    let report = encode_blocking(&NoopProgress, encode_request).expect("encode");
    assert!(report.encrypted);

    let mut decode_request = base_decode_request(carrier.clone(), Some(landing.clone()));
    decode_request.passphrase = Some("correct horse battery staple".to_string());
    decode_blocking(&NoopProgress, decode_request).expect("decode with the right passphrase");
    assert_eq!(
        std::fs::read(&landing).unwrap(),
        b"only for those who know the words"
    );

    let mut wrong = base_decode_request(carrier.clone(), Some(dir.join("wrong.txt")));
    wrong.passphrase = Some("not the right passphrase".to_string());
    let error = decode_blocking(&NoopProgress, wrong).unwrap_err();
    assert!(!error.message.is_empty());

    let missing = base_decode_request(carrier, Some(dir.join("missing.txt")));
    let error = decode_blocking(&NoopProgress, missing).unwrap_err();
    assert!(
        error.message.contains("passphrase"),
        "got: {}",
        error.message
    );
}

#[test]
fn round_trip_with_cover_audio() {
    let dir = TempDir::new("cover");
    let input = dir.join("secret.txt");
    let cover_path = dir.join("cover.wav");
    let carrier = dir.join("carrier.flac");
    let landing = dir.join("out.txt");
    std::fs::write(&input, b"hidden beneath a pleasant little tune").unwrap();
    std::fs::write(&cover_path, tone_wav(1)).unwrap();

    let mut encode_request = base_encode_request(input, carrier.clone());
    encode_request.cover = Some(CoverOptions {
        path: cover_path.display().to_string(),
        quality: "auto".to_string(),
        mode: "cut".to_string(),
        attenuation_db: 25.0,
        keep_metadata: false,
    });
    let report = encode_blocking(&NoopProgress, encode_request).expect("encode with cover");
    assert!(report.cover_band_hz.is_some());
    assert_eq!(report.channels, 1, "cover mode is always single-channel");

    let decoded = decode_blocking(
        &NoopProgress,
        base_decode_request(carrier, Some(landing.clone())),
    )
    .expect("decode a covered carrier");
    assert_eq!(decoded.recovered_bytes, "hidden beneath a pleasant little tune".len());
    assert_eq!(
        std::fs::read(&landing).unwrap(),
        b"hidden beneath a pleasant little tune"
    );
}

#[test]
fn cover_audio_is_refused_on_an_fsk_profile() {
    let dir = TempDir::new("cover-fsk");
    let input = dir.join("secret.txt");
    let cover_path = dir.join("cover.wav");
    let carrier = dir.join("carrier.flac");
    std::fs::write(&input, b"data").unwrap();
    std::fs::write(&cover_path, tone_wav(1)).unwrap();

    let mut request = base_encode_request(input, carrier);
    request.plan = PlanArgsDto {
        profile: Some("standard".to_string()),
        ..Default::default()
    };
    request.cover = Some(CoverOptions {
        path: cover_path.display().to_string(),
        quality: "auto".to_string(),
        mode: "cut".to_string(),
        attenuation_db: 25.0,
        keep_metadata: false,
    });

    let error = encode_blocking(&NoopProgress, request).unwrap_err();
    assert!(error.message.contains("spectrum"), "got: {}", error.message);
}

#[test]
fn fixed_channels_are_rejected_alongside_cover_audio() {
    let dir = TempDir::new("cover-channels");
    let input = dir.join("secret.txt");
    let cover_path = dir.join("cover.wav");
    let carrier = dir.join("carrier.flac");
    std::fs::write(&input, b"data").unwrap();
    std::fs::write(&cover_path, tone_wav(1)).unwrap();

    let mut request = base_encode_request(input, carrier);
    request.channels = "4".to_string();
    request.cover = Some(CoverOptions {
        path: cover_path.display().to_string(),
        quality: "auto".to_string(),
        mode: "cut".to_string(),
        attenuation_db: 25.0,
        keep_metadata: false,
    });

    let error = encode_blocking(&NoopProgress, request).unwrap_err();
    assert!(error.message.contains("channels"), "got: {}", error.message);
}

#[test]
fn multiple_channels_round_trip() {
    let dir = TempDir::new("channels");
    let input = dir.join("secret.txt");
    let carrier = dir.join("carrier.flac");
    let landing = dir.join("out.txt");
    std::fs::write(&input, b"split across several lanes and reassembled").unwrap();

    let mut request = base_encode_request(input, carrier.clone());
    request.channels = "4".to_string();
    let report = encode_blocking(&NoopProgress, request).expect("encode");
    assert_eq!(report.channels, 4);
    assert!(!report.channels_auto);

    decode_blocking(
        &NoopProgress,
        base_decode_request(carrier, Some(landing.clone())),
    )
    .expect("decode");
    assert_eq!(
        std::fs::read(&landing).unwrap(),
        b"split across several lanes and reassembled"
    );
}

#[test]
fn encoding_over_an_existing_file_requires_force() {
    let dir = TempDir::new("exists");
    let input = dir.join("secret.txt");
    let carrier = dir.join("carrier.flac");
    std::fs::write(&input, b"data").unwrap();
    std::fs::write(&carrier, b"already here").unwrap();

    let request = base_encode_request(input.clone(), carrier.clone());
    let error = encode_blocking(&NoopProgress, request).unwrap_err();
    assert!(error.message.contains("already exists"), "got: {}", error.message);

    let mut forced = base_encode_request(input, carrier);
    forced.force = true;
    encode_blocking(&NoopProgress, forced).expect("force should overwrite");
}

#[test]
fn decoding_over_an_existing_file_requires_force() {
    let dir = TempDir::new("decode-exists");
    let input = dir.join("secret.txt");
    let carrier = dir.join("carrier.flac");
    let landing = dir.join("out.txt");
    std::fs::write(&input, b"data").unwrap();
    std::fs::write(&landing, b"already here").unwrap();

    encode_blocking(&NoopProgress, base_encode_request(input, carrier.clone())).expect("encode");

    let request = base_decode_request(carrier.clone(), Some(landing.clone()));
    let error = decode_blocking(&NoopProgress, request).unwrap_err();
    assert!(error.message.contains("already exists"), "got: {}", error.message);

    let mut forced = base_decode_request(carrier, Some(landing));
    forced.force = true;
    decode_blocking(&NoopProgress, forced).expect("force should overwrite");
}

#[test]
fn decode_with_no_output_path_saves_next_to_the_carrier() {
    let dir = TempDir::new("default-output");
    let input = dir.join("secret.txt");
    let carrier = dir.join("carrier.flac");
    std::fs::write(&input, b"data").unwrap();

    encode_blocking(&NoopProgress, base_encode_request(input, carrier.clone())).expect("encode");

    // The stored name matches the input's own filename, so decoding back to
    // the default location (next to the carrier) would collide with the
    // still-present input; force it, since this test is about *where* the
    // file lands, not overwrite semantics (covered separately).
    let mut request = base_decode_request(carrier, None);
    request.force = true;
    let report = decode_blocking(&NoopProgress, request).expect("decode");
    let expected = dir.join("secret.txt");
    assert_eq!(report.output_path, expected.display().to_string());
    assert_eq!(std::fs::read(&expected).unwrap(), b"data");
}

#[test]
fn inspect_reports_the_headers_of_a_real_carrier() {
    let dir = TempDir::new("inspect");
    let input = dir.join("secret.txt");
    let carrier = dir.join("carrier.flac");
    std::fs::write(&input, b"twelve bytes").unwrap();

    let mut request = base_encode_request(input, carrier.clone());
    request.passphrase = Some("hunter2".to_string());
    encode_blocking(&NoopProgress, request).expect("encode");

    let info = inspect(carrier.display().to_string(), PlanArgsDto::default()).expect("inspect");
    assert_eq!(info.encrypted, Some(true));
    assert_eq!(info.fec, Some(true));
    assert_eq!(info.name_stored, Some(true));
    assert!(info.argon2id.is_some());
    assert_eq!(info.short_by_bytes, None, "a freshly written carrier is never short");
    assert_eq!(info.volume, None, "a plain carrier is not a split volume");
}

#[test]
fn no_store_name_leaves_the_carrier_anonymous() {
    let dir = TempDir::new("anonymous");
    let input = dir.join("secret.txt");
    let carrier = dir.join("carrier.flac");
    std::fs::write(&input, b"data").unwrap();

    let mut request = base_encode_request(input, carrier.clone());
    request.no_store_name = true;
    let report = encode_blocking(&NoopProgress, request).expect("encode");
    assert_eq!(report.stored_name, None);

    let info = inspect(carrier.display().to_string(), PlanArgsDto::default()).expect("inspect");
    assert_eq!(info.name_stored, Some(false));
    assert_eq!(info.format_stored, Some(false));
}

#[test]
fn split_size_produces_volumes_that_decode_finds_and_joins_from_any_part() {
    let dir = TempDir::new("split-roundtrip");
    let input = dir.join("secret.bin");
    let carrier = dir.join("carrier.flac");
    let data = incompressible(150_000, 11);
    std::fs::write(&input, &data).unwrap();

    let mut request = base_encode_request(input, carrier.clone());
    request.split_size_bytes = Some(40_000);
    let report = encode_blocking(&NoopProgress, request).expect("encode");
    assert!(
        report.volumes.len() >= 2,
        "expected split_size_bytes to produce multiple volumes, got {}",
        report.volumes.len()
    );
    assert!(!carrier.exists(), "the unsplit carrier.flac name should not exist once split");

    // Pointed at the *last* volume, not the first -- proving discovery walks
    // outward from whichever file it was given, not just forward from part 1.
    let last = report.volumes.last().unwrap();
    let landing = dir.join("recovered.bin");
    let decoded = decode_blocking(
        &NoopProgress,
        base_decode_request(PathBuf::from(&last.path), Some(landing.clone())),
    )
    .expect("decode from the last volume");
    assert_eq!(decoded.volumes_joined, Some(report.volumes.len() as u32));
    assert_eq!(std::fs::read(&landing).unwrap(), data);
}

#[test]
fn a_split_size_above_the_frame_produces_a_single_plain_file() {
    let dir = TempDir::new("split-fallback");
    let input = dir.join("small.bin");
    let carrier = dir.join("carrier.flac");
    std::fs::write(&input, incompressible(2_000, 3)).unwrap();

    let mut request = base_encode_request(input, carrier.clone());
    request.split_size_bytes = Some(10 * 1024 * 1024);
    let report = encode_blocking(&NoopProgress, request).expect("encode");
    assert!(report.volumes.is_empty(), "everything fit in one volume");
    assert_eq!(report.output_path, carrier.display().to_string());
    assert!(carrier.exists());
}

#[test]
fn split_size_and_cover_audio_are_mutually_exclusive() {
    let dir = TempDir::new("split-cover-conflict");
    let input = dir.join("secret.bin");
    let cover_path = dir.join("cover.wav");
    let carrier = dir.join("carrier.flac");
    std::fs::write(&input, incompressible(2_000, 5)).unwrap();
    std::fs::write(&cover_path, tone_wav(1)).unwrap();

    let mut request = base_encode_request(input, carrier);
    request.split_size_bytes = Some(1_000);
    request.cover = Some(CoverOptions {
        path: cover_path.display().to_string(),
        quality: "auto".to_string(),
        mode: "cut".to_string(),
        attenuation_db: 25.0,
        keep_metadata: false,
    });
    let error = encode_blocking(&NoopProgress, request).unwrap_err();
    assert!(error.message.contains("split"), "got: {}", error.message);
}

#[test]
fn inspect_reports_a_lone_volume_summary() {
    let dir = TempDir::new("inspect-volume");
    let input = dir.join("secret.bin");
    let carrier = dir.join("carrier.flac");
    std::fs::write(&input, incompressible(150_000, 7)).unwrap();

    let mut request = base_encode_request(input, carrier);
    request.split_size_bytes = Some(40_000);
    let report = encode_blocking(&NoopProgress, request).expect("encode");
    assert!(report.volumes.len() >= 2);

    let info = inspect(report.volumes[0].path.clone(), PlanArgsDto::default()).expect("inspect");
    let volume = info.volume.expect("a split part should report volume info");
    assert_eq!(volume.part, 1);
    assert_eq!(volume.of, report.volumes.len() as u32);
    assert_eq!(info.encrypted, None, "a volume header does not describe the frame header");
    assert_eq!(info.frame_bytes, None);
}
