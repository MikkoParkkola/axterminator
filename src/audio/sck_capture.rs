//! ScreenCaptureKit audio-only capture (macOS 14+).
//!
//! On macOS 14+, `SCStream` with `capturesAudio=true` and `width=0, height=0`
//! captures system audio output **without** requiring Screen Recording TCC
//! permission. This is a significant UX improvement over the AVAudioEngine
//! fallback path.
//!
//! The actual ScreenCaptureKit interaction is in `sck_audio_objc.m` (Objective-C).
//! This module provides the Rust FFI bridge and data conversion.
//!
//! Credit: Matthew Diakonov (@m13v) for the width=0/height=0 technique.

use tracing::{debug, info};

use super::{AudioData, AudioError, CHANNELS, SAMPLE_RATE};

// ---------------------------------------------------------------------------
// FFI declarations (defined in sck_audio_objc.m, compiled by build.rs)
// ---------------------------------------------------------------------------

/// Result struct from the Objective-C capture function. Must match the C layout
/// in `sck_audio_objc.m` exactly.
#[repr(C)]
struct AXTSCKCaptureResult {
    /// Heap-allocated float samples (caller must free via `axt_sck_free_result`).
    samples: *mut f32,
    sample_count: i32,
    sample_rate: f32,
    channels: i32,
    /// 0=ok, 1=unavailable, 2=no display, 3=capture failed.
    error_code: i32,
    /// Null-terminated error message (256 bytes).
    error_msg: [u8; 256],
}

extern "C" {
    fn axt_sck_is_available() -> bool;
    fn axt_sck_capture_system_audio(duration_secs: f32) -> AXTSCKCaptureResult;
    fn axt_sck_free_result(result: *mut AXTSCKCaptureResult);
}

// ---------------------------------------------------------------------------
// Public API (crate-internal)
// ---------------------------------------------------------------------------

/// Check if ScreenCaptureKit audio-only capture is available (macOS 14+).
pub(super) fn sck_available() -> bool {
    // SAFETY: C function with no side effects; just checks OS version + class availability.
    unsafe { axt_sck_is_available() }
}

/// Capture system audio via ScreenCaptureKit (macOS 14+, no Screen Recording permission).
///
/// Falls back to `AudioError::Framework` if SCK is not available or capture fails.
/// The caller should fall back to the AVAudioEngine path on error.
pub(super) fn capture_system_audio_sck(duration_secs: f32) -> Result<AudioData, AudioError> {
    info!(duration = duration_secs, "capturing system audio via ScreenCaptureKit (audio-only mode)");

    // SAFETY: C function that allocates a result struct. We own the result and free it below.
    let mut result = unsafe { axt_sck_capture_system_audio(duration_secs) };

    if result.error_code != 0 {
        let msg = error_msg_from_result(&result);
        // SAFETY: Free any partially allocated buffer.
        unsafe { axt_sck_free_result(&mut result) };
        return Err(AudioError::Framework(msg));
    }

    if result.samples.is_null() || result.sample_count <= 0 {
        unsafe { axt_sck_free_result(&mut result) };
        // No audio captured — return silence rather than error.
        debug!("SCK captured zero audio samples (no system audio playing?)");
        return Ok(AudioData::silent(duration_secs));
    }

    // Copy samples from the C-allocated buffer into a Rust Vec.
    let count = result.sample_count as usize;
    let native_samples: Vec<f32> =
        unsafe { std::slice::from_raw_parts(result.samples, count) }.to_vec();

    let native_rate = result.sample_rate;
    let native_channels = result.channels.max(1) as u16;

    // Free the C-allocated buffer now that we've copied the data.
    unsafe { axt_sck_free_result(&mut result) };

    // Downmix to mono if captured in stereo.
    let mono_samples = if native_channels > 1 {
        downmix_to_mono(&native_samples, native_channels)
    } else {
        native_samples
    };

    // Resample to our standard rate (16 kHz) if needed.
    let (final_samples, final_rate) = if (native_rate - SAMPLE_RATE as f32).abs() > 1.0 {
        let resampled = linear_resample(&mono_samples, native_rate as u32, SAMPLE_RATE);
        (resampled, SAMPLE_RATE)
    } else {
        (mono_samples, native_rate as u32)
    };

    #[allow(clippy::cast_precision_loss)]
    let actual_duration = final_samples.len() as f32 / final_rate as f32;

    debug!(
        native_rate,
        native_channels,
        final_samples = final_samples.len(),
        actual_duration,
        "SCK capture complete"
    );

    Ok(AudioData {
        samples: final_samples,
        sample_rate: final_rate,
        channels: CHANNELS,
        duration_secs: actual_duration.min(duration_secs),
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract null-terminated error message from the C result struct.
fn error_msg_from_result(result: &AXTSCKCaptureResult) -> String {
    let end = result
        .error_msg
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(result.error_msg.len());
    String::from_utf8_lossy(&result.error_msg[..end]).into_owned()
}

/// Downmix interleaved multi-channel audio to mono by averaging channels.
fn downmix_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels as usize;
    if ch <= 1 {
        return samples.to_vec();
    }
    samples
        .chunks_exact(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

/// Simple linear resampling from `from_rate` to `to_rate`.
///
/// Uses linear interpolation — good enough for speech/verification audio.
/// For production music capture we'd want sinc resampling, but that's
/// overkill for MCP tool responses.
fn linear_resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }

    let ratio = f64::from(to_rate) / f64::from(from_rate);
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let out_len = (samples.len() as f64 * ratio) as usize;
    let mut out = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_pos = i as f64 / ratio;
        let idx = src_pos as usize;
        let frac = (src_pos - idx as f64) as f32;

        let s0 = samples[idx.min(samples.len() - 1)];
        let s1 = samples[(idx + 1).min(samples.len() - 1)];
        out.push(s0 + frac * (s1 - s0));
    }

    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sck_available_returns_bool() {
        // Just verify it doesn't panic — result depends on macOS version.
        let _ = sck_available();
    }

    #[test]
    fn downmix_to_mono_identity_for_mono() {
        let mono = vec![0.5, -0.5, 0.25];
        assert_eq!(downmix_to_mono(&mono, 1), mono);
    }

    #[test]
    fn downmix_to_mono_averages_stereo() {
        // L=1.0, R=0.0 → 0.5; L=0.0, R=-1.0 → -0.5
        let stereo = vec![1.0, 0.0, 0.0, -1.0];
        let mono = downmix_to_mono(&stereo, 2);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.5).abs() < 1e-6);
        assert!((mono[1] - (-0.5)).abs() < 1e-6);
    }

    #[test]
    fn linear_resample_identity() {
        let samples = vec![1.0, 2.0, 3.0, 4.0];
        let resampled = linear_resample(&samples, 48000, 48000);
        assert_eq!(resampled, samples);
    }

    #[test]
    fn linear_resample_halves_rate() {
        // 4 samples at 48kHz → 2 samples at 24kHz
        let samples = vec![0.0, 1.0, 0.0, -1.0];
        let resampled = linear_resample(&samples, 48000, 24000);
        assert_eq!(resampled.len(), 2);
    }

    #[test]
    fn linear_resample_48k_to_16k() {
        // 48 samples at 48kHz = 1ms → should produce 16 samples at 16kHz
        let samples: Vec<f32> = (0..48).map(|i| (i as f32) / 48.0).collect();
        let resampled = linear_resample(&samples, 48000, 16000);
        assert_eq!(resampled.len(), 16);
        // First sample should be close to 0.0
        assert!(resampled[0].abs() < 0.05);
    }

    #[test]
    fn error_msg_from_result_extracts_string() {
        let mut result = AXTSCKCaptureResult {
            samples: std::ptr::null_mut(),
            sample_count: 0,
            sample_rate: 0.0,
            channels: 0,
            error_code: 1,
            error_msg: [0u8; 256],
        };
        let msg = b"test error";
        result.error_msg[..msg.len()].copy_from_slice(msg);
        assert_eq!(error_msg_from_result(&result), "test error");
    }
}
