//! `--split-size`: a payload spread across several FLAC carriers.
//!
//! `decode` is only ever handed one part's path in these tests. Finding and
//! joining the rest by filename is the entire point of the feature, so a test
//! that pointed at every part defeats what it is meant to prove.

mod fixtures;

use std::fs;

use fixtures::{decode, encode, stderr, TempDir};

/// List the `.partI-of-N.flac` siblings `encode --split-size` wrote next to
/// `carrier`, sorted by part number.
fn volume_parts(dir: &std::path::Path, stem: &str) -> Vec<std::path::PathBuf> {
    let mut parts: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(stem) && n.contains(".part"))
        })
        .collect();
    parts.sort();
    parts
}

#[test]
fn split_size_produces_volumes_that_decode_finds_and_joins_from_any_part() {
    let dir = TempDir::new("volumes-roundtrip");
    let input = dir.join("secret.bin");
    let carrier = dir.join("carrier.flac");
    let data = fixtures::incompressible(150_000, 11);
    fs::write(&input, &data).unwrap();

    let out = encode(&input, &carrier, &["--split-size", "40K"]);
    assert!(out.status.success(), "encode failed: {}", stderr(&out));

    let parts = volume_parts(&dir.0, "carrier");
    assert!(
        parts.len() >= 2,
        "expected --split-size to produce multiple volumes, got {}",
        parts.len()
    );
    assert!(
        !dir.join("carrier.flac").exists(),
        "the unsplit `carrier.flac` name should not exist once split into parts"
    );

    // Pointed at the *last* part, not the first -- proving discovery walks
    // outward from whichever file it was given, not just forward from part 1.
    let last_part = parts.last().unwrap();
    let landing = dir.join("recovered.bin");
    let out = decode(last_part, &landing, &[]);
    assert!(
        out.status.success(),
        "decode from {} failed: {}",
        last_part.display(),
        stderr(&out)
    );
    assert_eq!(
        fs::read(&landing).unwrap(),
        data,
        "round-tripped bytes differ from the original"
    );
}

#[test]
fn a_volume_size_above_the_frame_falls_back_to_a_single_plain_file() {
    let dir = TempDir::new("volumes-fallback");
    let input = dir.join("small.bin");
    let carrier = dir.join("carrier.flac");
    let data = fixtures::incompressible(2_000, 3);
    fs::write(&input, &data).unwrap();

    let out = encode(&input, &carrier, &["--split-size", "10M"]);
    assert!(out.status.success(), "encode failed: {}", stderr(&out));

    assert!(
        carrier.exists(),
        "a split size larger than the frame should still produce the plain output path"
    );
    assert!(
        volume_parts(&dir.0, "carrier").is_empty(),
        "no .partI-of-N siblings should exist when everything fit in one volume"
    );

    let landing = dir.join("recovered.bin");
    let out = decode(&carrier, &landing, &[]);
    assert!(out.status.success(), "decode failed: {}", stderr(&out));
    assert_eq!(fs::read(&landing).unwrap(), data);
}

#[test]
fn a_missing_volume_is_reported_by_name_not_a_generic_failure() {
    let dir = TempDir::new("volumes-missing");
    let input = dir.join("secret.bin");
    let carrier = dir.join("carrier.flac");
    fs::write(&input, fixtures::incompressible(150_000, 22)).unwrap();

    let out = encode(&input, &carrier, &["--split-size", "40K", "--no-encrypt"]);
    assert!(out.status.success(), "encode failed: {}", stderr(&out));

    let parts = volume_parts(&dir.0, "carrier");
    assert!(parts.len() >= 3, "need at least 3 parts for this test");
    fs::remove_file(&parts[1]).unwrap();

    let landing = dir.join("recovered.bin");
    let out = decode(&parts[0], &landing, &[]);
    assert!(
        !out.status.success(),
        "decode should fail when a volume is missing"
    );
    let message = stderr(&out);
    assert!(
        message.contains("missing"),
        "expected the error to say a part is missing, got: {message}"
    );
    assert!(
        !landing.exists(),
        "no output should be written when the archive is incomplete"
    );
}

#[test]
fn a_corrupted_volume_is_rejected_instead_of_silently_joined() {
    let dir = TempDir::new("volumes-corrupt");
    let input = dir.join("secret.bin");
    let carrier = dir.join("carrier.flac");
    fs::write(&input, fixtures::incompressible(150_000, 33)).unwrap();

    let out = encode(&input, &carrier, &["--split-size", "40K", "--no-encrypt"]);
    assert!(out.status.success(), "encode failed: {}", stderr(&out));

    let parts = volume_parts(&dir.0, "carrier");
    assert!(parts.len() >= 2, "need at least 2 parts for this test");

    // Flip a handful of bytes well past any FLAC container header, deep in
    // the modulated payload, so the corruption lands in the volume's own
    // data rather than being caught earlier by the FLAC reader itself.
    let victim = &parts[1];
    let mut bytes = fs::read(victim).unwrap();
    let start = bytes.len() / 2;
    for byte in &mut bytes[start..start + 8] {
        *byte ^= 0xff;
    }
    fs::write(victim, &bytes).unwrap();

    let landing = dir.join("recovered.bin");
    let out = decode(&parts[0], &landing, &[]);
    assert!(
        !out.status.success(),
        "decode should reject a corrupted volume rather than joining it"
    );
    let message = stderr(&out);
    assert!(
        message.contains(victim.file_name().unwrap().to_str().unwrap()),
        "expected the error to name the corrupted file, got: {message}"
    );
    assert!(!landing.exists());
}

#[test]
fn a_renamed_volume_gets_an_actionable_error_instead_of_a_missing_file_guess() {
    let dir = TempDir::new("volumes-renamed");
    let input = dir.join("secret.bin");
    let carrier = dir.join("carrier.flac");
    fs::write(&input, fixtures::incompressible(150_000, 44)).unwrap();

    let out = encode(&input, &carrier, &["--split-size", "40K", "--no-encrypt"]);
    assert!(out.status.success(), "encode failed: {}", stderr(&out));

    let parts = volume_parts(&dir.0, "carrier");
    assert!(parts.len() >= 2, "need at least 2 parts for this test");

    let renamed = dir.join("renamed.flac");
    fs::rename(&parts[0], &renamed).unwrap();

    let landing = dir.join("recovered.bin");
    let out = decode(&renamed, &landing, &[]);
    assert!(
        !out.status.success(),
        "decode should refuse to guess at a renamed volume's siblings"
    );
    let message = stderr(&out);
    assert!(
        message.contains("part") && message.contains("convention"),
        "expected an actionable naming-convention error, got: {message}"
    );
}
