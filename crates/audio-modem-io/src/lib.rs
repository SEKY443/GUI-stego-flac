//! Shared FLAC/tag/cover I/O for the stego-flac CLI and desktop GUI.
//!
//! `audio-modem-core` is deliberately container-independent: it maps
//! `&[u8]` to normalised PCM and back and knows nothing about files, FLAC, or
//! progress reporting. Every front end that reads or writes an actual `.flac`
//! carrier needs the same handful of things on top of that — container I/O,
//! the Vorbis-comment tags the tone plan travels in, cover-audio loading, and
//! a path-traversal-safe way to name a recovered file — so that logic lives
//! here once rather than once per front end.

pub mod cover;
pub mod flac_io;
pub mod flac_tags;
pub mod plan;
pub mod plan_resolve;
pub mod progress;

pub use plan::{plan_from_tags, sanitize_stored_name};
pub use plan_resolve::PlanOverrides;
pub use progress::{NoopProgress, ProgressSink};
