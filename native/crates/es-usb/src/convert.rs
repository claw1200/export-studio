//! Converting tracks with ffmpeg on their way to the stick.

use std::io;
use std::path::Path;

use es_convert::Preset;
use rb_usb::{AudioConverter, ConvertedAudio};

/// Raised when what an encode produces changes, so sticks converted before are converted again.
const ENCODER_VERSION: u32 = 1;

pub struct Ffmpeg {
    preset: Preset,
    profile: String,
}

impl Ffmpeg {
    pub fn new(preset: Preset) -> Self {
        let quality = match preset.bitrate {
            Some(kbps) => format!("{kbps}k"),
            None => format!("{}-{}", preset.bit_depth.unwrap_or(16), preset.sample_rate.unwrap_or(48_000)),
        };
        Self { profile: format!("es-ffmpeg-v{ENCODER_VERSION}:{}-{quality}", preset.format), preset }
    }
}

impl AudioConverter for Ffmpeg {
    fn profile(&self) -> &str {
        &self.profile
    }

    fn extension(&self) -> &str {
        self.preset.extension.trim_start_matches('.')
    }

    fn wants(&self, source: &Path) -> io::Result<bool> {
        Ok(!es_convert::matches_format(source, &self.preset))
    }

    fn convert(&self, source: &Path, destination: &Path) -> io::Result<ConvertedAudio> {
        es_convert::encode(source, destination, &self.preset).map_err(io::Error::other)?;
        let info = es_tags::read(destination).map_err(io::Error::other)?;
        let narrow = |value: i64| u32::try_from(value).unwrap_or(0);
        let requested = narrow(self.preset.bitrate.unwrap_or(0));
        // Only MP3 is encoded at a constant bitrate; AAC's is an average the encoder may undershoot.
        let bitrate = if self.preset.format == "mp3" { requested } else { Some(narrow(info.bitrate)).filter(|b| *b > 0).unwrap_or(requested) };
        Ok(ConvertedAudio {
            offset_ms: es_convert::delay_offset_ms(source, destination).unwrap_or(0),
            bitrate,
            sample_rate: narrow(info.sample_rate),
            bit_depth: u16::try_from(info.bit_depth).unwrap_or(0),
        })
    }

    fn shift_analysis(&self, _extension: &str, bytes: &[u8], offset_ms: i64) -> io::Result<Vec<u8>> {
        rb_anlz::write::shift_timeline(bytes, offset_ms)
    }

    fn estimate_bytes(&self, duration_sec: u64) -> u64 {
        let seconds = duration_sec.saturating_add(1);
        let bytes_per_second = match (self.preset.bitrate, self.preset.format) {
            (Some(kbps), _) => u64::try_from(kbps).unwrap_or(320) * 1000 / 8,
            (None, format) => {
                let depth = u64::try_from(self.preset.bit_depth.unwrap_or(16)).unwrap_or(16);
                let rate = u64::try_from(self.preset.sample_rate.unwrap_or(48_000)).unwrap_or(48_000);
                let pcm = rate * depth / 8 * 2;
                // Lossless compression roughly halves music; an estimate only.
                if matches!(format, "flac" | "alac") { pcm * 3 / 5 } else { pcm }
            }
        };
        seconds.saturating_mul(bytes_per_second).saturating_add(64 * 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(raw: &serde_json::Value) -> Option<Preset> {
        es_convert::clean_preset(raw).ok()
    }

    #[test]
    fn profiles_name_the_format_and_its_quality() {
        let mp3 = preset(&serde_json::json!({ "format": "mp3", "bitrate": 320 })).map(Ffmpeg::new);
        assert_eq!(mp3.as_ref().map(|c| (c.profile(), c.extension())), Some(("es-ffmpeg-v1:mp3-320k", "mp3")));
        let alac = preset(&serde_json::json!({ "format": "alac", "bitDepth": 24, "sampleRate": 96000 })).map(Ffmpeg::new);
        assert_eq!(alac.as_ref().map(|c| (c.profile(), c.extension())), Some(("es-ffmpeg-v1:alac-24-96000", "m4a")));
    }

    #[test]
    fn estimates_follow_the_quality() {
        let mp3 = preset(&serde_json::json!({ "format": "mp3", "bitrate": 320 })).map(Ffmpeg::new);
        let wav = preset(&serde_json::json!({ "format": "wav", "bitDepth": 16, "sampleRate": 44100 })).map(Ffmpeg::new);
        let size = |c: Option<Ffmpeg>| c.map_or(0, |c| c.estimate_bytes(59));
        assert_eq!(size(mp3), 60 * 40_000 + 65_536);
        assert_eq!(size(wav), 60 * 176_400 + 65_536);
    }
}
