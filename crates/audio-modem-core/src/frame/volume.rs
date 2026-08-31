//! Multi-volume splitting: one frame carried across several carriers.
//!
//! Encoding already produces a single self-describing `frame` (see
//! [`crate::pipeline`]) — a header followed by RaptorQ packets. Splitting does
//! not touch that layer at all; it slices the *finished* frame into
//! consecutive byte ranges and prefixes each with a small [`VolumeHeader`], so
//! every part independently declares which archive it belongs to, its own
//! position, and how many siblings to expect. `join` reverses this by
//! concatenating the ranges back in order, which reproduces the original frame
//! bit-for-bit — everything below it (RaptorQ, AEAD, zstd) never has to know
//! splitting happened.
//!
//! # Why this is not another FEC layer
//!
//! RaptorQ already makes the frame tolerant of a *truncated* carrier. Volumes
//! solve a different problem: a payload too large to move as one file at all
//! (an upload limit, a chat attachment cap, a slow or metered link where
//! smaller pieces resume better). A volume set is not redundant the way FEC
//! packets are — every part is a disjoint slice, and losing one is fatal to
//! the whole archive, same as a split RAR or 7z. The `archive_id` exists to
//! catch a specific, easy mistake: pointing `decode` at parts from two
//! different splits that happen to share a naming pattern.
//!
//! # Layout (all integers little-endian)
//!
//! ```text
//!  off  len  field
//!    0    4  magic "AMVL"
//!    4    1  version
//!    5    3  reserved
//!    8    8  archive_id       random, shared by every volume in the set
//!   16    4  volume_index     0-based
//!   20    4  volume_count     total volumes in the set
//!   24    8  volume_len       bytes of the frame slice following this header
//!   32    8  total_len        bytes of the full reassembled frame
//!   40    4  volume_crc       CRC-32 over this volume's slice bytes
//!   44    4  header_crc       CRC-32 over bytes 0..44
//! ```
//!
//! Two separate checksums, deliberately. `header_crc` catches a mis-demodulated
//! or wrong-tone-plan read the same way the main frame header's CRC does.
//! `volume_crc` covers the payload slice itself, so a corrupt part is reported
//! by name — "volume 2 of 5 is damaged" — instead of surfacing as an opaque
//! RaptorQ failure only after every other part has already been read.

use crate::error::FrameError;

/// Volume container magic.
pub const VOLUME_MAGIC: [u8; 4] = *b"AMVL";
/// Volume format version this build reads and writes.
pub const VOLUME_VERSION: u8 = 1;
/// Serialised volume header length in bytes.
pub const VOLUME_HEADER_LEN: usize = 48;
/// Byte range covered by the header checksum.
const VOLUME_CRC_COVERAGE: usize = 44;

/// Parsed volume header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeHeader {
    pub version: u8,
    /// Random identifier shared by every volume produced by one `split` call.
    pub archive_id: u64,
    /// This volume's position, 0-based.
    pub volume_index: u32,
    /// Total volumes in the set.
    pub volume_count: u32,
    /// Bytes of the frame slice carried by this volume.
    pub volume_len: u64,
    /// Bytes of the full frame once every volume is joined.
    pub total_len: u64,
    /// CRC-32 over this volume's slice bytes.
    pub volume_crc: u32,
}

impl VolumeHeader {
    /// Serialise to exactly [`VOLUME_HEADER_LEN`] bytes, appending the CRC.
    pub fn to_bytes(&self) -> [u8; VOLUME_HEADER_LEN] {
        let mut out = [0u8; VOLUME_HEADER_LEN];

        out[0..4].copy_from_slice(&VOLUME_MAGIC);
        out[4] = self.version;
        // 5..8 reserved
        out[8..16].copy_from_slice(&self.archive_id.to_le_bytes());
        out[16..20].copy_from_slice(&self.volume_index.to_le_bytes());
        out[20..24].copy_from_slice(&self.volume_count.to_le_bytes());
        out[24..32].copy_from_slice(&self.volume_len.to_le_bytes());
        out[32..40].copy_from_slice(&self.total_len.to_le_bytes());
        out[40..44].copy_from_slice(&self.volume_crc.to_le_bytes());

        let crc = crc32fast::hash(&out[..VOLUME_CRC_COVERAGE]);
        out[44..48].copy_from_slice(&crc.to_le_bytes());

        out
    }

    /// Parse and validate a volume header from the front of `bytes`.
    ///
    /// Only checks the header's own integrity — magic, version, CRC. Whether
    /// the payload that follows matches `volume_crc` is a separate question,
    /// answered by [`VolumeHeader::verify_payload`], because a caller often
    /// wants to report "which volume" before it has read the whole thing.
    pub fn parse(bytes: &[u8]) -> Result<Self, FrameError> {
        if bytes.len() < VOLUME_HEADER_LEN {
            return Err(FrameError::VolumeTooShort {
                len: bytes.len(),
                needed: VOLUME_HEADER_LEN,
            });
        }

        let magic: [u8; 4] = bytes[0..4].try_into().expect("slice is 4 bytes");
        if magic != VOLUME_MAGIC {
            return Err(FrameError::VolumeBadMagic { got: magic });
        }

        let version = bytes[4];
        if version != VOLUME_VERSION {
            return Err(FrameError::VolumeUnsupportedVersion {
                got: version,
                supported: VOLUME_VERSION,
            });
        }

        let stored = u32::from_le_bytes(bytes[44..48].try_into().expect("4 bytes"));
        let computed = crc32fast::hash(&bytes[..VOLUME_CRC_COVERAGE]);
        if stored != computed {
            return Err(FrameError::VolumeHeaderCrcMismatch { stored, computed });
        }

        Ok(Self {
            version,
            archive_id: u64::from_le_bytes(bytes[8..16].try_into().expect("8 bytes")),
            volume_index: u32::from_le_bytes(bytes[16..20].try_into().expect("4 bytes")),
            volume_count: u32::from_le_bytes(bytes[20..24].try_into().expect("4 bytes")),
            volume_len: u64::from_le_bytes(bytes[24..32].try_into().expect("8 bytes")),
            total_len: u64::from_le_bytes(bytes[32..40].try_into().expect("8 bytes")),
            volume_crc: u32::from_le_bytes(bytes[40..44].try_into().expect("4 bytes")),
        })
    }

    /// Check `payload` — the bytes that followed this header — against
    /// `volume_crc`.
    pub fn verify_payload(&self, payload: &[u8]) -> bool {
        crc32fast::hash(payload) == self.volume_crc
    }
}

/// Split `frame` into volumes of at most `volume_size` payload bytes each.
///
/// Returns one `VolumeHeader || slice` per volume, ready to be modulated and
/// written out independently. A `volume_size` at or above `frame.len()` yields
/// a single volume — callers that treat one volume as "no split needed" get
/// that for free.
pub fn split(frame: &[u8], volume_size: usize) -> Result<Vec<Vec<u8>>, FrameError> {
    if volume_size == 0 {
        return Err(FrameError::ZeroVolumeSize);
    }

    let total_len = frame.len() as u64;
    let volume_count = frame.len().div_ceil(volume_size).max(1) as u32;

    let mut id_bytes = [0u8; 8];
    getrandom::fill(&mut id_bytes).map_err(|error| FrameError::VolumeRng(error.to_string()))?;
    let archive_id = u64::from_le_bytes(id_bytes);

    let volumes = frame
        .chunks(volume_size)
        .enumerate()
        .map(|(index, slice)| {
            let header = VolumeHeader {
                version: VOLUME_VERSION,
                archive_id,
                volume_index: index as u32,
                volume_count,
                volume_len: slice.len() as u64,
                total_len,
                volume_crc: crc32fast::hash(slice),
            };
            let mut out = Vec::with_capacity(VOLUME_HEADER_LEN + slice.len());
            out.extend_from_slice(&header.to_bytes());
            out.extend_from_slice(slice);
            out
        })
        .collect();

    Ok(volumes)
}

/// Reassemble a frame from its volumes.
///
/// `volumes` need not arrive in order, but must together hold exactly one
/// entry per index `0..volume_count` from a single `archive_id`, and each
/// payload must pass its own [`VolumeHeader::verify_payload`] check — this
/// function trusts the caller to have already checked that, since the error
/// needs to name a specific volume the caller can identify by file path,
/// which this function cannot see.
pub fn join(mut volumes: Vec<(VolumeHeader, Vec<u8>)>) -> Result<Vec<u8>, FrameError> {
    let first = volumes.first().map(|(header, _)| *header);

    if let Some(first) = first {
        for (header, _) in &volumes[1..] {
            if header.archive_id != first.archive_id {
                return Err(FrameError::VolumeArchiveMismatch {
                    first: first.archive_id,
                    other: header.archive_id,
                });
            }
            if header.volume_count != first.volume_count {
                return Err(FrameError::VolumeCountMismatch {
                    count_a: first.volume_count,
                    count_b: header.volume_count,
                });
            }
        }
    }

    let count = first.map(|header| header.volume_count).unwrap_or(0);

    volumes.sort_by_key(|(header, _)| header.volume_index);

    let mut expected = 0u32;
    for (header, _) in &volumes {
        match header.volume_index.cmp(&expected) {
            std::cmp::Ordering::Less => {
                return Err(FrameError::VolumeDuplicate {
                    index: header.volume_index,
                })
            }
            std::cmp::Ordering::Greater => {
                return Err(FrameError::VolumeMissing {
                    index: expected,
                    count,
                })
            }
            std::cmp::Ordering::Equal => expected += 1,
        }
    }
    if expected != count {
        return Err(FrameError::VolumeMissing {
            index: expected,
            count,
        });
    }

    let total_len = first.map(|header| header.total_len).unwrap_or(0);
    let mut frame = Vec::with_capacity(total_len as usize);
    for (_, payload) in volumes {
        frame.extend_from_slice(&payload);
    }

    if frame.len() as u64 != total_len {
        return Err(FrameError::VolumeLengthMismatch {
            expected: total_len,
            got: frame.len() as u64,
        });
    }

    Ok(frame)
}
