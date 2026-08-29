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
    use super::sanitize_stored_name;

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
