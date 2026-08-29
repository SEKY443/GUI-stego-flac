//! A front-end-agnostic sink for "what stage is the pipeline in".
//!
//! Encoding or decoding a large payload can take from seconds to hours, with
//! long stretches inside a single call (FLAC compression, RaptorQ) that
//! produce no output of their own. Something has to tell the user it is
//! moving rather than hung, but *how* differs entirely by front end: a
//! terminal wants a self-erasing status line, a JSON consumer wants one
//! `{"stage": ...}` line per stage, and a GUI wants an event it can bind a
//! progress bar to. This trait is the seam between the pipeline and whichever
//! of those a caller is running.
pub trait ProgressSink {
    /// Announce that the next named stage has begun.
    fn stage(&self, what: &str);
}

/// A sink that does nothing, for callers that do not care.
pub struct NoopProgress;

impl ProgressSink for NoopProgress {
    fn stage(&self, _what: &str) {}
}
