//! Helpers that sit between a carrier's stored metadata and the tone plan /
//! output filename an application actually uses.

use std::path::Path;

use audio_modem_core::Plan;

use crate::flac_tags;

/// Read the tone plan recorded in a carrier's FLAC metadata.
///
/// Returns `None` when the tags are absent, unparseable, or when the caller
/// already resolved an explicit plan of its own — in which case that choice
/// takes precedence and reading the file's opinion would only muddy the
/// resolution order.
///
/// A malformed plan is reported to `on_warning` and ignored rather than
/// raised: the carrier is still perfectly decodable if the plan is supplied by
/// hand, so refusing to open the file would be a worse outcome than losing the
/// shortcut.
pub fn plan_from_tags(
    raw: &[u8],
    user_was_explicit: bool,
    on_warning: impl FnOnce(&str),
) -> Option<Plan> {
    if user_was_explicit {
        return None;
    }

    let tags = flac_tags::read_tags(raw).ok()?;
    let recorded = tags.get(flac_tags::PLAN_TAG)?;

    match Plan::from_plan_string(recorded) {
        Ok(config) => Some(config),
        Err(error) => {
            on_warning(&format!("ignoring unreadable plan in metadata ({error})"));
            None
        }
    }
}

/// Reduce a stored filename to a single safe path component.
///
/// A carrier is untrusted input, and a stored name such as
/// `../../.ssh/authorized_keys` must not be able to steer a write anywhere
/// outside the intended output directory. This is the one place that decision
/// is made; every caller that writes a decoded payload under its stored name
/// must go through it rather than re-deriving the same rule independently.
///
/// Returns `None` if nothing safe survives (e.g. the stored name was `..` or
/// empty).
pub fn sanitize_stored_name(stored: &str) -> Option<&str> {
    Path::new(stored)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio_modem_core::Profile;

    /// A minimal but genuine FLAC byte stream carrying only the metadata
    /// block `plan_from_tags` needs — no STREAMINFO, no audio frames, since
    /// `flac_tags::read_tags` (and therefore `plan_from_tags`) never looks
    /// past the metadata block chain.
    fn fake_flac_with_tags(tags: &[(&str, &str)]) -> Vec<u8> {
        let owned: Vec<(String, String)> =
            tags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let block = flac_tags::build_block(&owned);

        let mut out = b"fLaC".to_vec();
        out.push(0x80 | flac_tags::VORBIS_COMMENT_BLOCK); // last block, this type
        let len = (block.len() as u32).to_be_bytes();
        out.extend_from_slice(&len[1..]); // 24-bit big-endian length
        out.extend_from_slice(&block);
        out
    }

    #[test]
    fn an_explicit_user_plan_short_circuits_before_reading_tags() {
        let raw = fake_flac_with_tags(&[(flac_tags::PLAN_TAG, "garbage that would warn")]);
        let mut warned = false;
        assert_eq!(plan_from_tags(&raw, true, |_| warned = true), None);
        assert!(!warned, "should never even look at the tags");
    }

    #[test]
    fn a_valid_recorded_plan_round_trips() {
        let plan = Profile::Compact.plan();
        let raw = fake_flac_with_tags(&[(flac_tags::PLAN_TAG, &plan.to_plan_string())]);
        assert_eq!(plan_from_tags(&raw, false, |_| panic!("no warning expected")), Some(plan));
    }

    #[test]
    fn a_malformed_recorded_plan_warns_and_returns_none() {
        let raw = fake_flac_with_tags(&[(flac_tags::PLAN_TAG, "mode=not-a-real-waveform")]);
        let mut warning = None;
        let result = plan_from_tags(&raw, false, |msg| warning = Some(msg.to_string()));
        assert_eq!(result, None);
        assert!(warning.is_some(), "expected a warning about the bad plan");
    }

    #[test]
    fn no_plan_tag_returns_none_without_warning() {
        let raw = fake_flac_with_tags(&[("TITLE", "some carrier")]);
        assert_eq!(plan_from_tags(&raw, false, |_| panic!("no warning expected")), None);
    }

    #[test]
    fn non_flac_input_returns_none() {
        let raw = b"not a flac file at all".to_vec();
        assert_eq!(plan_from_tags(&raw, false, |_| panic!("no warning expected")), None);
    }

    #[test]
    fn strips_directory_traversal() {
        assert_eq!(
            sanitize_stored_name("../../.ssh/authorized_keys"),
            Some("authorized_keys")
        );
    }

    #[test]
    fn rejects_dot_and_dotdot() {
        assert_eq!(sanitize_stored_name(".."), None);
        assert_eq!(sanitize_stored_name("."), None);
        assert_eq!(sanitize_stored_name(""), None);
    }

    #[test]
    fn keeps_plain_names_unchanged() {
        assert_eq!(sanitize_stored_name("report.pdf"), Some("report.pdf"));
    }
}
