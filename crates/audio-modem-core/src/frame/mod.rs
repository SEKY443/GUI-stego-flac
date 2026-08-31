//! Frame structure carried by the physical layer.

pub mod header;
pub mod volume;

pub use header::{Header, HEADER_LEN, MAGIC, VERSION};
pub use volume::{VolumeHeader, VOLUME_HEADER_LEN, VOLUME_MAGIC, VOLUME_VERSION};
