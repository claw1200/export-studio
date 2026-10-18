//! Primitives shared across every rbxport crate.
//!
//! Shared types and durable filesystem publication; no network or Tauri.

pub use rb_storage::durable;
pub mod ids;
pub mod mpeg;
pub mod musickey;
pub mod time;
pub mod xml;

pub use rb_model::{ContentId, ExportMetadata, FourCc, PlaylistId, RowIndex, TIMESTAMP_FORMAT};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fourcc_roundtrips_as_text() {
        assert_eq!(FourCc::new(b"PQTZ").to_string(), "PQTZ");
    }

    #[test]
    fn fourcc_does_not_panic_on_non_utf8() {
        assert_eq!(FourCc([0xff, 0xfe, 0xfd, 0xfc]).as_str(), "????");
    }
}
pub mod paths;
