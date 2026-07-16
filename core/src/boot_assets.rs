//! Embedded Phase 6 boot identity assets.
#![cfg_attr(test, allow(dead_code))]

use crate::audio;
use crate::serial;

pub const WAKE_PHRASE: &str = "PythOS [HISS] We Are Woken";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VisualFrame {
    pub at_ms: u16,
    pub text: &'static str,
    pub scale: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcmAsset {
    pub sample_rate_hz: u32,
    pub channel_count: u8,
    pub source_count: u8,
    pub frame_count: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncPoint {
    pub visual_index: u8,
    pub audio_frame: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootAssets {
    pub wake_phrase: &'static str,
    pub visual_frames: &'static [VisualFrame],
    pub pcm: PcmAsset,
    pub sync_points: &'static [SyncPoint],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootAssetError {
    EmptyVisualSequence,
    SyncLengthMismatch,
    NonMonotonicSync,
    SyncOutsideAudio,
}

pub const VISUAL_FRAMES: [VisualFrame; 3] = [
    VisualFrame {
        at_ms: 0,
        text: "PythOS",
        scale: 4,
    },
    VisualFrame {
        at_ms: 180,
        text: "[HISS]",
        scale: 3,
    },
    VisualFrame {
        at_ms: 420,
        text: "We Are Woken",
        scale: 3,
    },
];

pub const PCM_ASSET: PcmAsset = PcmAsset {
    sample_rate_hz: audio::BOOT_AUDIO_SAMPLE_RATE_HZ,
    channel_count: audio::BOOT_AUDIO_CHANNELS,
    source_count: audio::BOOT_AUDIO_SOURCE_COUNT,
    frame_count: audio::BOOT_AUDIO_FRAME_COUNT,
};

pub const SYNC_POINTS: [SyncPoint; 3] = [
    SyncPoint {
        visual_index: 0,
        audio_frame: audio_frame_at_ms(VISUAL_FRAMES[0].at_ms),
    },
    SyncPoint {
        visual_index: 1,
        audio_frame: audio_frame_at_ms(VISUAL_FRAMES[1].at_ms),
    },
    SyncPoint {
        visual_index: 2,
        audio_frame: audio_frame_at_ms(VISUAL_FRAMES[2].at_ms),
    },
];

pub static BOOT_ASSETS: BootAssets = BootAssets {
    wake_phrase: WAKE_PHRASE,
    visual_frames: &VISUAL_FRAMES,
    pcm: PCM_ASSET,
    sync_points: &SYNC_POINTS,
};

pub fn load_assets() -> Result<&'static BootAssets, BootAssetError> {
    validate_assets(&BOOT_ASSETS)?;
    serial::write_line("PYTHOS:CORE:BOOT_ASSET:VISUAL");
    serial::write_line("PYTHOS:CORE:BOOT_ASSET:PCM");
    serial::write_line("PYTHOS:CORE:BOOT_ASSET:SYNC");
    Ok(&BOOT_ASSETS)
}

pub fn validate_assets(assets: &BootAssets) -> Result<(), BootAssetError> {
    if assets.visual_frames.is_empty() {
        return Err(BootAssetError::EmptyVisualSequence);
    }
    if assets.visual_frames.len() != assets.sync_points.len() {
        return Err(BootAssetError::SyncLengthMismatch);
    }

    let mut previous_frame = 0u16;
    for (index, sync) in assets.sync_points.iter().enumerate() {
        if usize::from(sync.visual_index) != index {
            return Err(BootAssetError::SyncLengthMismatch);
        }
        if index > 0 && sync.audio_frame <= previous_frame {
            return Err(BootAssetError::NonMonotonicSync);
        }
        if sync.audio_frame >= assets.pcm.frame_count {
            return Err(BootAssetError::SyncOutsideAudio);
        }
        previous_frame = sync.audio_frame;
    }
    Ok(())
}

const fn audio_frame_at_ms(ms: u16) -> u16 {
    ((ms as u32 * audio::BOOT_AUDIO_SAMPLE_RATE_HZ) / 1000) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_assets_validate() {
        assert_eq!(validate_assets(&BOOT_ASSETS), Ok(()));
    }

    #[test]
    fn wake_phrase_matches_phase_6_identity() {
        assert_eq!(BOOT_ASSETS.wake_phrase, "PythOS [HISS] We Are Woken");
    }

    #[test]
    fn sync_points_fit_inside_embedded_audio_asset() {
        let last = BOOT_ASSETS.sync_points.last().unwrap();

        assert!(last.audio_frame < BOOT_ASSETS.pcm.frame_count);
        assert_eq!(BOOT_ASSETS.pcm.frame_count, 32_768);
    }
}
