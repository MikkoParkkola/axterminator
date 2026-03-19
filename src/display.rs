//! Multi-monitor display enumeration and coordinate mapping.
//!
//! macOS uses a single global coordinate space shared by all connected displays.
//! The primary display's top-left corner is always `(0, 0)`.  Secondary monitors
//! placed to the left of the primary have **negative** x-coordinates; monitors
//! above have negative y-coordinates.  Retina displays report logical (point)
//! bounds from `CGDisplayBounds` and physical (pixel) counts from
//! `CGDisplayPixelsWide` / `CGDisplayPixelsHigh`.
//!
//! ## Usage
//!
//! ```
//! use axterminator::display::{list_displays, display_for_point};
//!
//! let displays = list_displays().unwrap_or_default();
//! if let Some(primary) = displays.iter().find(|d| d.is_primary) {
//!     println!("primary: {:?}", primary.bounds);
//! }
//!
//! // Find which display owns (−1920, 100) — common for a monitor left of primary
//! let display = display_for_point(-1920.0, 100.0, &displays);
//! ```
//!
//! ## Coordinate conventions
//!
//! All bounds and coordinates returned by this module use **global screen
//! coordinates in logical points** (the same coordinate space as the macOS
//! Accessibility API).  Callers that need physical pixels must multiply by
//! `Display::scale_factor`.

use core_graphics::display::{CGDirectDisplayID, CGDisplay};
use serde::{Deserialize, Serialize};

use crate::error::{AXError, AXResult};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Axis-aligned rectangle in global logical-point coordinates.
///
/// `origin` is the top-left corner; `size` is always positive.
/// The origin of the primary display is always `(0, 0)`.
/// Secondary displays can have negative origin values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// X coordinate of the left edge (may be negative).
    pub x: f64,
    /// Y coordinate of the top edge (may be negative).
    pub y: f64,
    /// Width in logical points (always positive).
    pub width: f64,
    /// Height in logical points (always positive).
    pub height: f64,
}

impl Rect {
    /// Returns `true` if the point `(px, py)` falls within this rect.
    ///
    /// The check is half-open: `[x, x+w) × [y, y+h)`.
    #[must_use]
    pub fn contains_point(&self, px: f64, py: f64) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }

    /// Returns the union of `self` and `other` — the smallest rect that
    /// contains both.
    #[must_use]
    pub fn union(self, other: Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        Rect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }

    /// Returns `true` if `other` overlaps with `self`.
    #[must_use]
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}

/// A connected display and its geometry.
///
/// Bounds are in the global macOS coordinate space (logical points).
/// Multiply width/height by `scale_factor` to get physical pixel dimensions.
///
/// # Examples
///
/// ```
/// use axterminator::display::list_displays;
///
/// for display in list_displays().unwrap_or_default() {
///     println!(
///         "id={} bounds={:?} scale={:.0}x primary={}",
///         display.id, display.bounds, display.scale_factor, display.is_primary
///     );
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Display {
    /// Unique `CGDirectDisplayID` assigned by CoreGraphics.
    pub id: u32,
    /// Bounds in global logical-point coordinates.
    pub bounds: Rect,
    /// Retina / `HiDPI` scale factor.
    ///
    /// `1.0` for standard-resolution displays, `2.0` for typical Retina.
    pub scale_factor: f64,
    /// `true` if this is the system's primary display (origin `(0, 0)`).
    pub is_primary: bool,
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/// Return all currently active (connected and drawable) displays.
///
/// On a single-monitor setup this returns exactly one element.
/// Returns an error if CoreGraphics fails to enumerate displays, which should
/// not happen in normal operation.
///
/// # Errors
///
/// Returns [`AXError::SystemError`] when `CGGetActiveDisplayList` fails.
///
/// # Examples
///
/// ```
/// use axterminator::display::list_displays;
///
/// let displays = list_displays().expect("failed to list displays");
/// assert!(!displays.is_empty(), "at least one display must be active");
/// ```
pub fn list_displays() -> AXResult<Vec<Display>> {
    let ids = CGDisplay::active_displays()
        .map_err(|code| AXError::SystemError(format!("CGGetActiveDisplayList failed: {code}")))?;

    ids.into_iter().map(build_display).collect()
}

/// Find the display that contains the point `(x, y)` in global coordinates.
///
/// Returns `None` when the point does not fall within any connected display.
/// This can happen if the coordinates are stale (display disconnected) or
/// fall exactly on the boundary between displays (treated as the lower-right
/// display in the overlap, i.e. the half-open `[x, x+w)` check).
///
/// # Examples
///
/// ```
/// use axterminator::display::{display_for_point, list_displays};
///
/// let displays = list_displays().unwrap_or_default();
/// // Point at the origin always belongs to the primary display.
/// let d = display_for_point(0.0, 0.0, &displays);
/// assert!(d.is_some());
/// ```
#[must_use]
pub fn display_for_point(x: f64, y: f64, displays: &[Display]) -> Option<&Display> {
    displays.iter().find(|d| d.bounds.contains_point(x, y))
}

/// Determine which display(s) a window rect overlaps.
///
/// Returns all displays whose bounds intersect `window_bounds`.  A window
/// straddling two monitors will be included in both.  The caller can use
/// [`Rect::union`] on the returned bounds to compute the spanning rect.
///
/// # Examples
///
/// ```
/// use axterminator::display::{displays_for_rect, list_displays, Rect};
///
/// let displays = list_displays().unwrap_or_default();
/// let window = Rect { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 };
/// let overlapping = displays_for_rect(&window, &displays);
/// assert!(!overlapping.is_empty());
/// ```
#[must_use]
pub fn displays_for_rect<'d>(window_bounds: &Rect, displays: &'d [Display]) -> Vec<&'d Display> {
    displays
        .iter()
        .filter(|d| d.bounds.intersects(window_bounds))
        .collect()
}

/// Translate global-coordinate point `(x, y)` to display-local coordinates.
///
/// Returns `None` when the point does not belong to any active display.
///
/// # Examples
///
/// ```
/// use axterminator::display::{global_to_local, list_displays};
///
/// let displays = list_displays().unwrap_or_default();
/// // Primary display origin should map to (0.0, 0.0) local.
/// if let Some((lx, ly)) = global_to_local(0.0, 0.0, &displays) {
///     assert_eq!(lx, 0.0);
///     assert_eq!(ly, 0.0);
/// }
/// ```
#[must_use]
pub fn global_to_local(x: f64, y: f64, displays: &[Display]) -> Option<(f64, f64)> {
    display_for_point(x, y, displays).map(|d| (x - d.bounds.x, y - d.bounds.y))
}

/// Translate display-local coordinates back to global coordinates.
///
/// `display_id` must match a display in `displays`; returns `None` otherwise.
///
/// # Examples
///
/// ```
/// use axterminator::display::{local_to_global, list_displays};
///
/// let displays = list_displays().unwrap_or_default();
/// if let Some(primary) = displays.iter().find(|d| d.is_primary) {
///     let (gx, gy) = local_to_global(primary.id, 100.0, 200.0, &displays).unwrap();
///     assert_eq!(gx, 100.0 + primary.bounds.x);
///     assert_eq!(gy, 200.0 + primary.bounds.y);
/// }
/// ```
#[must_use]
pub fn local_to_global(
    display_id: u32,
    local_x: f64,
    local_y: f64,
    displays: &[Display],
) -> Option<(f64, f64)> {
    displays
        .iter()
        .find(|d| d.id == display_id)
        .map(|d| (local_x + d.bounds.x, local_y + d.bounds.y))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build a [`Display`] from a `CGDirectDisplayID`.
#[allow(clippy::unnecessary_wraps)] // Result preserved for forward-compatibility
fn build_display(id: CGDirectDisplayID) -> AXResult<Display> {
    let cg = CGDisplay::new(id);
    let cg_bounds = cg.bounds();

    let bounds = Rect {
        x: cg_bounds.origin.x,
        y: cg_bounds.origin.y,
        width: cg_bounds.size.width,
        height: cg_bounds.size.height,
    };

    let scale_factor = compute_scale_factor(cg, &bounds);

    Ok(Display {
        id,
        bounds,
        scale_factor,
        is_primary: cg.is_main(),
    })
}

/// Compute the display's Retina scale factor.
///
/// `CGDisplayBounds` returns logical point dimensions; `pixels_wide` /
/// `pixels_high` return physical pixel counts.  Dividing physical by logical
/// yields the scale factor (1.0 for standard, 2.0 for Retina).
///
/// We use the width axis; if logical width is zero (degenerate display), we
/// fall back to 1.0.
fn compute_scale_factor(cg: CGDisplay, logical_bounds: &Rect) -> f64 {
    if logical_bounds.width == 0.0 {
        return 1.0;
    }
    // pixels_wide() returns usize on macOS; precision loss from usize→f64 is
    // acceptable here (display pixel counts are always < 2^53).
    #[allow(clippy::cast_precision_loss)]
    let physical_width = cg.pixels_wide() as f64;
    (physical_width / logical_bounds.width).max(1.0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Rect ----------------------------------------------------------------

    #[test]
    fn rect_contains_point_origin() {
        // GIVEN: a rect starting at origin
        let r = Rect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        // WHEN/THEN: origin is inside
        assert!(r.contains_point(0.0, 0.0));
    }

    #[test]
    fn rect_contains_point_negative_origin() {
        // GIVEN: rect with negative origin (secondary monitor left of primary)
        let r = Rect {
            x: -2560.0,
            y: 0.0,
            width: 2560.0,
            height: 1440.0,
        };
        // WHEN/THEN: point inside negative-x region
        assert!(r.contains_point(-1280.0, 720.0));
        assert!(!r.contains_point(100.0, 100.0));
    }

    #[test]
    fn rect_contains_point_half_open_right_edge_is_exclusive() {
        // GIVEN: rect [0, 1920) x [0, 1080)
        let r = Rect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        // THEN: right edge is exclusive
        assert!(!r.contains_point(1920.0, 0.0));
        assert!(r.contains_point(1919.999, 0.0));
    }

    #[test]
    fn rect_union_two_adjacent_displays() {
        // GIVEN: primary and secondary side by side
        let primary = Rect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        let secondary = Rect {
            x: -2560.0,
            y: 0.0,
            width: 2560.0,
            height: 1440.0,
        };
        // WHEN: united
        let u = primary.union(secondary);
        // THEN: spans from -2560 to 1920
        assert_eq!(u.x, -2560.0);
        assert_eq!(u.y, 0.0);
        assert_eq!(u.width, 4480.0);
        assert_eq!(u.height, 1440.0);
    }

    #[test]
    fn rect_intersects_overlapping_rects() {
        let a = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let b = Rect {
            x: 50.0,
            y: 50.0,
            width: 100.0,
            height: 100.0,
        };
        assert!(a.intersects(&b));
    }

    #[test]
    fn rect_intersects_non_overlapping_rects() {
        let a = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let b = Rect {
            x: 200.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        assert!(!a.intersects(&b));
    }

    #[test]
    fn rect_intersects_touching_edge_is_non_overlapping() {
        // Adjacent displays share an edge but do not overlap.
        let a = Rect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        let b = Rect {
            x: 1920.0,
            y: 0.0,
            width: 2560.0,
            height: 1440.0,
        };
        // x + width == other.x → strict inequality → no intersection
        assert!(!a.intersects(&b));
    }

    // -- display_for_point ---------------------------------------------------

    #[test]
    fn display_for_point_primary_at_origin() {
        // GIVEN: synthetic two-display layout
        let displays = synthetic_two_display_layout();
        // WHEN: querying origin (primary)
        let d = display_for_point(0.0, 0.0, &displays);
        // THEN: primary found
        assert!(d.is_some());
        assert!(d.unwrap().is_primary);
    }

    #[test]
    fn display_for_point_secondary_negative_x() {
        // GIVEN: two-display layout with secondary at -2560
        let displays = synthetic_two_display_layout();
        // WHEN: querying a point in the negative-x region
        let d = display_for_point(-1000.0, 500.0, &displays);
        // THEN: secondary found
        assert!(d.is_some());
        assert!(!d.unwrap().is_primary);
    }

    #[test]
    fn display_for_point_returns_none_for_gap() {
        // GIVEN: two displays with a gap between them (unusual but valid config)
        let displays = vec![
            make_display(1, 0.0, 0.0, 1920.0, 1080.0, true),
            make_display(2, 2000.0, 0.0, 1920.0, 1080.0, false), // gap at 1920–2000
        ];
        // WHEN: point in the gap
        let d = display_for_point(1950.0, 100.0, &displays);
        // THEN: no display found
        assert!(d.is_none());
    }

    // -- displays_for_rect ---------------------------------------------------

    #[test]
    fn displays_for_rect_window_on_primary_only() {
        let displays = synthetic_two_display_layout();
        let window = Rect {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        };
        let found = displays_for_rect(&window, &displays);
        assert_eq!(found.len(), 1);
        assert!(found[0].is_primary);
    }

    #[test]
    fn displays_for_rect_window_spans_both_displays() {
        let displays = synthetic_two_display_layout();
        // Window straddles the boundary at x=0
        let window = Rect {
            x: -100.0,
            y: 0.0,
            width: 500.0,
            height: 400.0,
        };
        let found = displays_for_rect(&window, &displays);
        assert_eq!(found.len(), 2);
    }

    // -- global_to_local / local_to_global ------------------------------------

    #[test]
    fn global_to_local_primary_origin_maps_to_zero() {
        let displays = synthetic_two_display_layout();
        let result = global_to_local(0.0, 0.0, &displays);
        assert_eq!(result, Some((0.0, 0.0)));
    }

    #[test]
    fn global_to_local_secondary_negative_x() {
        let displays = synthetic_two_display_layout();
        // Point at -2000, 100 → local x = -2000 - (-2560) = 560
        let result = global_to_local(-2000.0, 100.0, &displays);
        assert!(result.is_some());
        let (lx, ly) = result.unwrap();
        assert!((lx - 560.0).abs() < f64::EPSILON);
        assert!((ly - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn local_to_global_round_trips_primary() {
        let displays = synthetic_two_display_layout();
        let primary_id = displays.iter().find(|d| d.is_primary).unwrap().id;
        let (gx, gy) = local_to_global(primary_id, 300.0, 400.0, &displays).unwrap();
        let (lx, ly) = global_to_local(gx, gy, &displays).unwrap();
        assert!((lx - 300.0).abs() < f64::EPSILON);
        assert!((ly - 400.0).abs() < f64::EPSILON);
    }

    #[test]
    fn local_to_global_unknown_display_returns_none() {
        let displays = synthetic_two_display_layout();
        assert!(local_to_global(9999, 0.0, 0.0, &displays).is_none());
    }

    // -- compute_scale_factor ------------------------------------------------

    #[test]
    fn compute_scale_factor_degenerate_zero_width_returns_one() {
        // GIVEN: zero-width logical bounds (degenerate)
        let cg = CGDisplay::new(0); // id=0 won't be called
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
        // WHEN/THEN: must not divide by zero, returns 1.0
        let sf = compute_scale_factor(cg, &bounds);
        assert_eq!(sf, 1.0);
    }

    // -- list_displays integration -------------------------------------------

    #[test]
    fn list_displays_returns_at_least_one() {
        // Integration: must have at least the built-in display on any Mac.
        let displays = list_displays().expect("CGGetActiveDisplayList must succeed");
        assert!(!displays.is_empty(), "at least one display must be active");
    }

    #[test]
    fn list_displays_has_exactly_one_primary() {
        let displays = list_displays().expect("must enumerate displays");
        let primaries: Vec<_> = displays.iter().filter(|d| d.is_primary).collect();
        assert_eq!(primaries.len(), 1, "exactly one primary display");
    }

    #[test]
    fn list_displays_scale_factor_at_least_one() {
        let displays = list_displays().expect("must enumerate displays");
        for d in &displays {
            assert!(d.scale_factor >= 1.0, "scale factor must be >= 1.0");
        }
    }

    #[test]
    fn list_displays_primary_bounds_width_positive() {
        let displays = list_displays().expect("must enumerate displays");
        let primary = displays.iter().find(|d| d.is_primary).unwrap();
        assert!(primary.bounds.width > 0.0);
        assert!(primary.bounds.height > 0.0);
    }

    // -- helpers -------------------------------------------------------------

    fn make_display(id: u32, x: f64, y: f64, w: f64, h: f64, primary: bool) -> Display {
        Display {
            id,
            bounds: Rect {
                x,
                y,
                width: w,
                height: h,
            },
            scale_factor: 1.0,
            is_primary: primary,
        }
    }

    fn synthetic_two_display_layout() -> Vec<Display> {
        vec![
            make_display(1, 0.0, 0.0, 1920.0, 1080.0, true),
            make_display(2, -2560.0, 0.0, 2560.0, 1440.0, false),
        ]
    }
}
