//! On-device OCR fallback via `VNRecognizeTextRequest` (Vision framework).
//!
//! Uses `axt_recognize_text` from `ocr_objc.m`.  No TCC permission is
//! required beyond what the screenshot path already holds.
//!
//! ## Feature gate
//!
//! Compiled unconditionally on macOS — `VNRecognizeTextRequest` has been
//! available since macOS 10.15 (Catalina).  The `ObjC` implementation guards
//! the API with `@available(macOS 10.15, *)` and returns an empty string on
//! older systems, so the binary never crashes.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use axterminator::ocr::recognize_text_from_png;
//!
//! # fn example(png_bytes: &[u8]) {
//! let text = recognize_text_from_png(png_bytes, 0.3).unwrap_or_default();
//! if !text.trim().is_empty() {
//!     println!("OCR found: {text}");
//! }
//! # }
//! ```

#[cfg(target_os = "macos")]
use std::ffi::{c_char, CStr};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during OCR.
#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    /// The Vision framework returned a null pointer (should not happen in practice).
    #[error("ocr_null: axt_recognize_text returned a null pointer")]
    NullResult,
}

// ---------------------------------------------------------------------------
// FFI declarations
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
extern "C" {
    /// Run `VNRecognizeTextRequest` on PNG bytes.
    ///
    /// # Safety
    ///
    /// * `png_data` must be a valid pointer to at least `png_len` bytes.
    /// * The returned pointer must be freed via `axt_free_text`.
    /// * The returned pointer is never null (`ObjC` side always returns `strdup("")`
    ///   on failure).
    fn axt_recognize_text(png_data: *const u8, png_len: usize, min_confidence: f32)
        -> *const c_char;

    /// Free a string allocated by `axt_recognize_text`.
    ///
    /// # Safety
    ///
    /// `text` must have been returned by `axt_recognize_text` and not yet freed.
    /// Passing `null` is safe.
    fn axt_free_text(text: *const c_char);
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Run on-device OCR on a PNG-encoded screenshot.
///
/// Returns the recognised text as a single `String` with observations
/// joined by newlines.  Returns `Ok("")` when no text is found or the
/// Vision framework is unavailable (macOS < 10.15).
///
/// # Arguments
///
/// * `png_data` — PNG-encoded image bytes (e.g. from `app.screenshot_native()`).
/// * `min_confidence` — Per-observation confidence threshold in `[0.0, 1.0]`.
///   Values below this are discarded.  `0.3` is a reasonable default.
///
/// # Errors
///
/// Returns [`OcrError::NullResult`] when the FFI function returns a null
/// pointer, which is not expected in normal operation.
///
/// # Examples
///
/// ```rust,no_run
/// use axterminator::ocr::recognize_text_from_png;
///
/// # fn example(png_bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
/// let text = recognize_text_from_png(png_bytes, 0.3)?;
/// println!("recognized: {text}");
/// # Ok(())
/// # }
/// ```
#[cfg(target_os = "macos")]
pub fn recognize_text_from_png(png_data: &[u8], min_confidence: f32) -> Result<String, OcrError> {
    // SAFETY:
    //   - png_data.as_ptr() is valid for png_data.len() bytes.
    //   - axt_recognize_text never returns null (ObjC side: `strdup("")` on error).
    //   - We call axt_free_text exactly once after converting to String.
    let raw = unsafe { axt_recognize_text(png_data.as_ptr(), png_data.len(), min_confidence) };

    if raw.is_null() {
        return Err(OcrError::NullResult);
    }

    // SAFETY: raw is a valid NUL-terminated UTF-8 string from ObjC strdup.
    let text = unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned();

    // SAFETY: raw was returned by axt_recognize_text and not yet freed.
    unsafe { axt_free_text(raw) };

    Ok(text)
}

/// Stub for non-macOS targets so the module compiles on all platforms.
#[cfg(not(target_os = "macos"))]
pub fn recognize_text_from_png(_png_data: &[u8], _min_confidence: f32) -> Result<String, OcrError> {
    Ok(String::new())
}

// ---------------------------------------------------------------------------
// Internal helper for the AX fallback path
// ---------------------------------------------------------------------------

/// Minimum characters of AX text before OCR fallback is suppressed.
pub(crate) const OCR_FALLBACK_THRESHOLD: usize = 50;

/// Env-var name that gates OCR fallback activation.
pub(crate) const OCR_ENV_VAR: &str = "AXTERMINATOR_OCR";

/// Returns `true` when the OCR fallback is enabled via `AXTERMINATOR_OCR=true`.
#[must_use]
pub(crate) fn ocr_fallback_enabled() -> bool {
    std::env::var(OCR_ENV_VAR)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Run OCR on `png_bytes` and return a fallback annotation string, or `None`
/// when OCR yields no text.
///
/// This is the central call site used by the AX-tree handlers.
#[must_use]
pub(crate) fn ocr_fallback_annotation(png_bytes: &[u8]) -> Option<String> {
    let text = recognize_text_from_png(png_bytes, 0.3).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(format!("[OCR fallback: {trimmed}]"))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{ocr_fallback_enabled, ocr_fallback_threshold_check, OCR_ENV_VAR};

    #[test]
    fn ocr_fallback_disabled_by_default() {
        // GIVEN: env var is not set
        std::env::remove_var(OCR_ENV_VAR);
        // THEN: fallback is disabled
        assert!(!ocr_fallback_enabled());
    }

    #[test]
    fn ocr_fallback_enabled_with_true() {
        // GIVEN: env var set to "true"
        std::env::set_var(OCR_ENV_VAR, "true");
        // THEN: fallback is enabled
        assert!(ocr_fallback_enabled());
        std::env::remove_var(OCR_ENV_VAR);
    }

    #[test]
    fn ocr_fallback_enabled_with_one() {
        // GIVEN: env var set to "1"
        std::env::set_var(OCR_ENV_VAR, "1");
        // THEN: fallback is enabled
        assert!(ocr_fallback_enabled());
        std::env::remove_var(OCR_ENV_VAR);
    }

    #[test]
    fn ocr_fallback_disabled_with_false() {
        // GIVEN: env var set to "false"
        std::env::set_var(OCR_ENV_VAR, "false");
        // THEN: fallback is disabled
        assert!(!ocr_fallback_enabled());
        std::env::remove_var(OCR_ENV_VAR);
    }

    #[test]
    fn threshold_check_below_limit() {
        // GIVEN: 49 chars of AX text
        let short = "a".repeat(49);
        // THEN: threshold triggered → needs OCR
        assert!(ocr_fallback_threshold_check(&short));
    }

    #[test]
    fn threshold_check_above_limit() {
        // GIVEN: 50+ chars of AX text
        let long = "a".repeat(50);
        // THEN: threshold NOT triggered → no OCR needed
        assert!(!ocr_fallback_threshold_check(&long));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recognize_text_empty_png_returns_ok() {
        // GIVEN: empty bytes (not a valid PNG)
        // THEN: returns Ok("") without panicking
        let result = super::recognize_text_from_png(&[], 0.3);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recognize_text_invalid_png_returns_ok() {
        // GIVEN: random garbage bytes
        let garbage = b"this is not a png image at all 12345";
        // THEN: returns Ok("") gracefully
        let result = super::recognize_text_from_png(garbage, 0.3);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }
}

/// Returns `true` when the AX-tree text is below the OCR fallback threshold.
#[must_use]
pub(crate) fn ocr_fallback_threshold_check(ax_text: &str) -> bool {
    ax_text.len() < OCR_FALLBACK_THRESHOLD
}
