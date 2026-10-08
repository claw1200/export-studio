//! Decoded audio on its way from a deck's decode thread to the audio callback.

/// Frames per block: about 11.6 ms at 44.1 kHz.
pub const BLOCK_FRAMES: usize = 512;

/// Blocks a deck keeps queued during playback: about 186 ms, enough to ride out a slow read.
pub const RING_BLOCKS: usize = 16;

/// A block of interleaved stereo at the device rate.
///
/// Stored inline so moving one through the ring never allocates.
pub struct Block {
    /// The deck generation it was decoded under. The callback drops blocks from before a seek.
    pub generation: u32,
    /// Device-rate frame of the block's first frame.
    pub position: u64,
    pub frames: u16,
    pub samples: [f32; BLOCK_FRAMES * 2],
}

impl Block {
    pub fn empty(generation: u32, position: u64) -> Self {
        Self {
            generation,
            position,
            frames: 0,
            samples: [0.0; BLOCK_FRAMES * 2],
        }
    }
}
