//! A cover and its rekordbox thumbnails, stored under `share/PIONEER/Artwork`.

use std::io::Cursor;
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat};

use crate::{Error, Result};

const THUMBNAILS: [(&str, u32); 2] = [("m", 240), ("s", 80)];
const JPEG_QUALITY: u8 = 90;

fn other(e: impl std::fmt::Display) -> Error {
    Error::Other(e.to_string())
}

/// Write the cover as given and square JPEG thumbnails beside it. Returns the stored `ImagePath`.
pub fn save(data: &[u8], share: &Path, uuid: &str) -> Result<String> {
    let format = image::guess_format(data).map_err(other)?;
    let ext = match format {
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Png => "png",
        other_format => return Err(Error::Other(format!("Unsupported artwork format: {other_format:?}"))),
    };
    let picture = image::load_from_memory_with_format(data, format).map_err(other)?;
    let shard: String = uuid.chars().take(3).collect();
    let relative = format!("/PIONEER/Artwork/{shard}/{uuid}/artwork.{ext}");
    let folder = share.join("PIONEER").join("Artwork").join(&shard).join(uuid);
    std::fs::create_dir_all(&folder).map_err(other)?;
    std::fs::write(folder.join(format!("artwork.{ext}")), data).map_err(other)?;
    let rgb = DynamicImage::ImageRgb8(picture.to_rgb8());
    for (suffix, size) in THUMBNAILS {
        let thumb = DynamicImage::ImageRgb8(rgb.resize_to_fill(size, size, FilterType::Lanczos3).to_rgb8());
        let mut bytes = Cursor::new(Vec::new());
        JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY).encode_image(&thumb).map_err(other)?;
        std::fs::write(folder.join(format!("artwork_{suffix}.jpg")), bytes.into_inner()).map_err(other)?;
    }
    Ok(relative)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(300, 200).write_to(&mut bytes, ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn writes_the_cover_and_square_thumbnails() {
        let share = tempfile::tempdir().unwrap();
        let relative = save(&png(), share.path(), "abcdef-1234").unwrap();
        assert_eq!(relative, "/PIONEER/Artwork/abc/abcdef-1234/artwork.png");
        let folder = share.path().join("PIONEER/Artwork/abc/abcdef-1234");
        let small = image::open(folder.join("artwork_s.jpg")).unwrap();
        let medium = image::open(folder.join("artwork_m.jpg")).unwrap();
        assert_eq!((small.width(), small.height()), (80, 80));
        assert_eq!((medium.width(), medium.height()), (240, 240));
    }

    #[test]
    fn rejects_other_formats() {
        let share = tempfile::tempdir().unwrap();
        assert!(save(b"GIF89a....", share.path(), "abcdef").is_err());
    }
}
