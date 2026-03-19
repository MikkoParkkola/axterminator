//! Virtual desktop (Spaces) management via the CoreGraphics private SPI.
//!
//! macOS Spaces are managed through undocumented `CGSSpace` functions exported
//! by the `CoreGraphics` framework (also reachable through `SkyLight` on macOS
//! 12+).  These symbols are **not part of Apple's public API** and may change
//! across OS releases.  This module:
//!
//! - Declares raw FFI bindings with no third-party crate.
//! - Wraps them in safe Rust types that enforce agent-created-vs-user-created
//!   distinction (AC8).
//! - Cleans up all agent-created spaces on `Drop` (AC5).
//!
//! ## Feature flag
//!
//! This module is compiled only when the `spaces` cargo feature is enabled.
//! Users must opt in because the private API may be blocked by SIP on some
//! configurations and cannot be used in App Store builds.
//!
//! ## SIP behaviour
//!
//! Standard SIP on macOS 14 (Sonoma) does **not** block `CGSSpace` APIs — they
//! work normally from a process with Accessibility permission.  If a function
//! returns `kCGErrorIllegalArgument` or `kCGErrorNotImplemented` on a specific
//! macOS version, [`SpaceError::ApiUnavailable`] is returned with a descriptive
//! message.
//!
//! ## macOS version support
//!
//! Tested on macOS 14 (Sonoma).  The `CGSSpace` functions have been stable
//! since macOS 10.9; the `SkyLight` re-export path exists since macOS 12.
//!
//! ## App Store
//!
//! Private API usage means builds with `--features spaces` **cannot** be
//! submitted to the Mac App Store.  Disable the feature for store builds.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Private SPI — FFI declarations
// ---------------------------------------------------------------------------

/// Opaque connection handle required by most `CGSSpace` functions.
pub type CGSConnectionID = u32;

/// Identifies a virtual desktop (Space).
pub type CGSSpaceID = u64;

/// Space type discriminant returned by `CGSSpaceGetType`.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CGSSpaceType {
    /// A normal user-managed desktop.
    User = 0,
    /// A full-screen application space.
    FullScreen = 1,
    /// A system-reserved space (e.g. Dashboard in older macOS).
    System = 2,
    /// Unrecognised discriminant value.
    Unknown = 0xFFFF_FFFF,
}

impl CGSSpaceType {
    fn from_raw(v: u32) -> Self {
        match v {
            0 => Self::User,
            1 => Self::FullScreen,
            2 => Self::System,
            _ => Self::Unknown,
        }
    }
}

// SAFETY: These are direct, documented (even if private) C-ABI functions
// exported by CoreGraphics.framework / SkyLight.framework.  The calling
// conventions match macOS ARM64 and x86-64 C ABI.  No mutable global state
// is shared; the CGSConnectionID is thread-safe by Apple's implementation.
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    /// Returns the default (per-process) CGS connection.
    fn CGSMainConnectionID() -> CGSConnectionID;

    /// Returns the ID of the currently active Space on the main display.
    fn CGSGetActiveSpace(cid: CGSConnectionID) -> CGSSpaceID;

    /// Returns the type of a Space (user/fullscreen/system).
    fn CGSSpaceGetType(cid: CGSConnectionID, sid: CGSSpaceID) -> u32;

    /// Creates a new user Space and returns its ID.
    /// `options` is a CF dictionary; passing NULL uses defaults.
    fn CGSSpaceCreate(cid: CGSConnectionID, options: *const std::ffi::c_void) -> CGSSpaceID;

    /// Destroys a Space.  All windows on it are moved to the active Space.
    fn CGSSpaceDestroy(cid: CGSConnectionID, sid: CGSSpaceID);

    /// Switches the active Space on the main display to `sid`.
    fn CGSShowSpaces(cid: CGSConnectionID, spaces: *const CGSSpaceID, count: i32);

    /// Moves the windows identified by `wids` to Space `sid`.
    ///
    /// `wids` is a C array of `CGWindowID` (`u32`); `count` is its length.
    fn CGSAddWindowsToSpaces(
        cid: CGSConnectionID,
        wids: *const u32,
        wid_count: i32,
        sids: *const CGSSpaceID,
        sid_count: i32,
    );

    /// Removes the windows identified by `wids` from Space `sid`.
    fn CGSRemoveWindowsFromSpaces(
        cid: CGSConnectionID,
        wids: *const u32,
        wid_count: i32,
        sids: *const CGSSpaceID,
        sid_count: i32,
    );

    /// Returns a `CFArray` of `CFNumber` objects, each holding a `CGSSpaceID`.
    /// Caller must `CFRelease` the returned array.
    fn CGSCopySpaces(cid: CGSConnectionID, mask: u32) -> *const std::ffi::c_void;

    /// Returns the count of elements in a `CFArray`.
    fn CFArrayGetCount(arr: *const std::ffi::c_void) -> i64;

    /// Returns the element at `index` from a `CFArray` (untyped).
    fn CFArrayGetValueAtIndex(arr: *const std::ffi::c_void, index: i64) -> *const std::ffi::c_void;

    /// Extracts a value from a `CFNumber` into `*value_ptr`.
    ///
    /// `type_code` 4 = `kCFNumberSInt32Type` (i32),
    ///             11 = `kCFNumberSInt64Type` (i64).
    /// Returns true on success.
    fn CFNumberGetValue(
        number: *const std::ffi::c_void,
        type_code: i32,
        value_ptr: *mut std::ffi::c_void,
    ) -> bool;

    /// Releases a Core Foundation object.
    fn CFRelease(obj: *const std::ffi::c_void);
}

/// `kCFNumberSInt64Type` — extract a 64-bit integer from a `CFNumber`.
const CF_NUMBER_SINT64_TYPE: i32 = 11;

/// Mask value for `CGSCopySpaces` — returns all Space types.
const CGS_SPACE_ALL: u32 = 0x7;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from Space management operations.
#[derive(Debug, thiserror::Error)]
pub enum SpaceError {
    /// The `CGSSpace` API returned an error or an unexpected null.
    #[error("CGSSpace API error: {0}")]
    ApiError(String),

    /// The API could not be called (wrong macOS version or other constraint).
    #[error("CGSSpace API unavailable: {0}")]
    ApiUnavailable(String),

    /// Caller attempted to destroy a Space not created by the agent.
    ///
    /// This is a hard safety invariant: agent code must never destroy a Space
    /// the user created.
    #[error("space {0} was not created by the agent — refusing to destroy")]
    NotAgentSpace(CGSSpaceID),

    /// The requested Space does not exist.
    #[error("space {0} not found")]
    NotFound(CGSSpaceID),

    /// No windows matched the request.
    #[error("no windows found for the operation")]
    NoWindows,
}

/// Result alias for Space operations.
pub type SpaceResult<T> = Result<T, SpaceError>;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A virtual desktop (Space) and its metadata.
///
/// # Examples
///
/// ```
/// use axterminator::spaces::SpaceManager;
///
/// let mgr = SpaceManager::new();
/// for space in mgr.list_spaces().unwrap_or_default() {
///     println!("space {} type={:?} active={}", space.id, space.space_type, space.is_active);
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    /// CoreGraphics Space identifier.
    pub id: CGSSpaceID,
    /// Whether this is the currently active (visible) Space.
    pub is_active: bool,
    /// Space type (user desktop, full-screen app, system).
    pub space_type: CGSSpaceType,
    /// `true` when this Space was created by the agent (not the user).
    pub is_agent_created: bool,
}

// ---------------------------------------------------------------------------
// SpaceManager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of agent-created Spaces.
///
/// `SpaceManager` is the single owner of all Spaces created by the agent.
/// On `Drop` it destroys every agent-created Space, ensuring no orphaned
/// virtual desktops are left behind after a session ends (AC5).
///
/// It is cheaply cloneable via `Arc` so it can be shared across tool handlers.
///
/// # Safety invariant
///
/// [`SpaceManager::destroy_space`] will **refuse** to destroy Spaces not in its
/// internal `agent_spaces` set, preventing accidental deletion of user Spaces.
///
/// # Examples
///
/// ```no_run
/// use axterminator::spaces::SpaceManager;
///
/// let mgr = SpaceManager::new();
///
/// // Create an isolated agent workspace (requires a WindowServer session).
/// let space = mgr.create_space().expect("failed to create space");
/// println!("created space {}", space.id);
///
/// // Explicit cleanup (also happens automatically on Drop)
/// mgr.destroy_space(space.id).expect("failed to destroy space");
/// ```
#[derive(Clone, Debug)]
pub struct SpaceManager {
    inner: Arc<Mutex<SpaceManagerInner>>,
}

#[derive(Debug)]
struct SpaceManagerInner {
    cid: CGSConnectionID,
    /// IDs of Spaces created by this manager instance.
    agent_spaces: HashSet<CGSSpaceID>,
}

impl SpaceManager {
    /// Create a new `SpaceManager` connected to the default CGS session.
    #[must_use]
    pub fn new() -> Self {
        // SAFETY: CGSMainConnectionID has no preconditions and always returns
        // a valid connection ID for the current process.
        let cid = unsafe { CGSMainConnectionID() };
        Self {
            inner: Arc::new(Mutex::new(SpaceManagerInner {
                cid,
                agent_spaces: HashSet::new(),
            })),
        }
    }

    /// Enumerate all currently active Spaces.
    ///
    /// # Errors
    ///
    /// Returns [`SpaceError::ApiError`] when `CGSCopySpaces` returns null.
    pub fn list_spaces(&self) -> SpaceResult<Vec<Space>> {
        let inner = self.lock();
        list_spaces_impl(inner.cid, &inner.agent_spaces)
    }

    /// Create a new user Space.
    ///
    /// The new Space is **not** switched to automatically — the user's current
    /// desktop is unaffected.  The returned `Space` struct reflects the newly
    /// created Space.
    ///
    /// # Errors
    ///
    /// Returns [`SpaceError::ApiError`] when `CGSSpaceCreate` returns 0.
    pub fn create_space(&self) -> SpaceResult<Space> {
        let mut inner = self.lock();
        // SAFETY: cid is valid; NULL options pointer selects defaults.
        let sid = unsafe { CGSSpaceCreate(inner.cid, std::ptr::null()) };
        if sid == 0 {
            return Err(SpaceError::ApiError(
                "CGSSpaceCreate returned 0 — API may be unavailable on this macOS version".into(),
            ));
        }
        inner.agent_spaces.insert(sid);
        let cid = inner.cid;
        let agent_spaces = &inner.agent_spaces;
        // SAFETY: cid is valid, sid is just-created and non-zero.
        let space_type = CGSSpaceType::from_raw(unsafe { CGSSpaceGetType(cid, sid) });
        let is_active = unsafe { CGSGetActiveSpace(cid) } == sid;
        Ok(Space {
            id: sid,
            is_active,
            space_type,
            is_agent_created: agent_spaces.contains(&sid),
        })
    }

    /// Destroy an agent-created Space.
    ///
    /// Refuses to destroy Spaces not created by this manager (AC8).  Windows
    /// on the destroyed Space are moved by macOS to the previously active Space.
    ///
    /// # Errors
    ///
    /// - [`SpaceError::NotAgentSpace`] — Space was not created by the agent.
    /// - [`SpaceError::NotFound`] — Space does not exist in the current list.
    pub fn destroy_space(&self, sid: CGSSpaceID) -> SpaceResult<()> {
        let mut inner = self.lock();
        if !inner.agent_spaces.contains(&sid) {
            return Err(SpaceError::NotAgentSpace(sid));
        }
        // SAFETY: cid is valid, sid belongs to our tracked agent set.
        unsafe { CGSSpaceDestroy(inner.cid, sid) };
        inner.agent_spaces.remove(&sid);
        Ok(())
    }

    /// Switch the active Space to `sid`.
    ///
    /// This moves the user's view to the named Space.  Use sparingly: for
    /// background automation, prefer moving the target app to an agent Space
    /// and interacting there without switching.
    ///
    /// # Errors
    ///
    /// Returns [`SpaceError::NotFound`] when `sid` is not among active Spaces.
    pub fn switch_to_space(&self, sid: CGSSpaceID) -> SpaceResult<()> {
        let inner = self.lock();
        // Verify the space exists before attempting to switch.
        let spaces = list_spaces_impl(inner.cid, &inner.agent_spaces)?;
        spaces
            .iter()
            .find(|s| s.id == sid)
            .ok_or(SpaceError::NotFound(sid))?;

        // SAFETY: cid is valid; `sid` is verified to exist.
        unsafe { CGSShowSpaces(inner.cid, &raw const sid, 1) };
        Ok(())
    }

    /// Move windows identified by `window_ids` to Space `sid`.
    ///
    /// First removes the windows from all other Spaces, then adds them to `sid`.
    /// Returns the number of windows successfully moved.
    ///
    /// # Errors
    ///
    /// - [`SpaceError::NoWindows`] — `window_ids` is empty.
    /// - [`SpaceError::NotFound`] — target Space `sid` does not exist.
    pub fn move_windows_to_space(&self, window_ids: &[u32], sid: CGSSpaceID) -> SpaceResult<usize> {
        if window_ids.is_empty() {
            return Err(SpaceError::NoWindows);
        }

        let inner = self.lock();
        let spaces = list_spaces_impl(inner.cid, &inner.agent_spaces)?;
        if !spaces.iter().any(|s| s.id == sid) {
            return Err(SpaceError::NotFound(sid));
        }

        let other_space_ids: Vec<CGSSpaceID> = spaces
            .iter()
            .filter(|s| s.id != sid)
            .map(|s| s.id)
            .collect();

        if !other_space_ids.is_empty() {
            // Window and space counts are always small (< 2^31); the casts
            // are safe in practice. CGS APIs use i32 for counts.
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let wid_count = window_ids.len() as i32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let sid_count = other_space_ids.len() as i32;
            // SAFETY: all pointers are to stack-allocated, non-null slices.
            unsafe {
                CGSRemoveWindowsFromSpaces(
                    inner.cid,
                    window_ids.as_ptr(),
                    wid_count,
                    other_space_ids.as_ptr(),
                    sid_count,
                );
            }
        }

        // Window count is always small (< 2^31).
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let wid_count = window_ids.len() as i32;
        // SAFETY: sid is verified to exist; slices are valid for the duration
        // of the call.
        unsafe {
            CGSAddWindowsToSpaces(
                inner.cid,
                window_ids.as_ptr(),
                wid_count,
                &raw const sid,
                1,
            );
        }
        Ok(window_ids.len())
    }

    /// Destroy all agent-created Spaces.
    ///
    /// Called automatically by `Drop`; may also be called explicitly to
    /// release Spaces at session-end without waiting for the manager to go
    /// out of scope.
    pub fn destroy_all_agent_spaces(&self) {
        let mut inner = self.lock();
        let to_destroy: Vec<CGSSpaceID> = inner.agent_spaces.iter().copied().collect();
        for sid in to_destroy {
            // SAFETY: cid is valid; sid is in our agent set.
            unsafe { CGSSpaceDestroy(inner.cid, sid) };
        }
        inner.agent_spaces.clear();
    }

    /// Return a snapshot of the set of agent-created Space IDs.
    #[must_use]
    pub fn agent_space_ids(&self) -> HashSet<CGSSpaceID> {
        self.lock().agent_spaces.clone()
    }

    // -- internal ------------------------------------------------------------

    fn lock(&self) -> std::sync::MutexGuard<'_, SpaceManagerInner> {
        self.inner.lock().expect("SpaceManager mutex poisoned")
    }
}

impl Default for SpaceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SpaceManager {
    fn drop(&mut self) {
        // Only clean up when we hold the last Arc reference.
        if Arc::strong_count(&self.inner) == 1 {
            self.destroy_all_agent_spaces();
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// List all spaces; mark which ones were created by the agent.
fn list_spaces_impl(
    cid: CGSConnectionID,
    agent_spaces: &HashSet<CGSSpaceID>,
) -> SpaceResult<Vec<Space>> {
    // SAFETY: CGSCopySpaces returns a CFArray or NULL on error.
    let arr = unsafe { CGSCopySpaces(cid, CGS_SPACE_ALL) };
    if arr.is_null() {
        return Err(SpaceError::ApiError(
            "CGSCopySpaces returned null — verify Accessibility permission is granted".into(),
        ));
    }

    // SAFETY: arr is a non-null CFArray returned by CGSCopySpaces.
    let count = unsafe { CFArrayGetCount(arr) };
    // SAFETY: arr is non-null; active space query is always safe.
    let active_sid = unsafe { CGSGetActiveSpace(cid) };

    // count is always non-negative (macOS invariant); sign-loss and truncation
    // are benign — no system could have 2^63 Spaces.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let mut spaces = Vec::with_capacity(count as usize);
    for i in 0..count {
        // SAFETY: arr is valid; i < count.
        let num_ptr = unsafe { CFArrayGetValueAtIndex(arr, i) };
        if num_ptr.is_null() {
            continue;
        }

        // Each element is a CFNumber holding the CGSSpaceID as an i64.
        // SAFETY: num_ptr is a non-null CFNumber from CGSCopySpaces.
        let mut sid_raw: i64 = 0;
        let ok = unsafe {
            CFNumberGetValue(
                num_ptr,
                CF_NUMBER_SINT64_TYPE,
                std::ptr::addr_of_mut!(sid_raw).cast(),
            )
        };
        if !ok || sid_raw <= 0 {
            continue;
        }
        // sid_raw > 0 is verified above; reinterpreting as u64 is safe.
        #[allow(clippy::cast_sign_loss)]
        let sid = sid_raw as CGSSpaceID;

        // SAFETY: cid and sid are both valid.
        let space_type = CGSSpaceType::from_raw(unsafe { CGSSpaceGetType(cid, sid) });
        spaces.push(Space {
            id: sid,
            is_active: sid == active_sid,
            space_type,
            is_agent_created: agent_spaces.contains(&sid),
        });
    }

    // SAFETY: arr is a CFArray we own; we must release it.
    unsafe { CFRelease(arr) };
    Ok(spaces)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // CGSSpaceType
    // -----------------------------------------------------------------------

    #[test]
    fn space_type_from_raw_user_is_zero() {
        assert_eq!(CGSSpaceType::from_raw(0), CGSSpaceType::User);
    }

    #[test]
    fn space_type_from_raw_fullscreen_is_one() {
        assert_eq!(CGSSpaceType::from_raw(1), CGSSpaceType::FullScreen);
    }

    #[test]
    fn space_type_from_raw_system_is_two() {
        assert_eq!(CGSSpaceType::from_raw(2), CGSSpaceType::System);
    }

    #[test]
    fn space_type_from_raw_unknown_for_high_value() {
        assert_eq!(CGSSpaceType::from_raw(99), CGSSpaceType::Unknown);
    }

    // -----------------------------------------------------------------------
    // SpaceManager — safety invariants (mock-free, purely Rust logic)
    // -----------------------------------------------------------------------

    #[test]
    fn destroy_space_rejects_non_agent_space() {
        // GIVEN: a manager with no created spaces
        let mgr = SpaceManager::new();
        // WHEN: attempting to destroy a space not in the agent set
        let result = mgr.destroy_space(12345);
        // THEN: NotAgentSpace error
        assert!(matches!(result, Err(SpaceError::NotAgentSpace(12345))));
    }

    #[test]
    fn agent_space_ids_empty_on_new_manager() {
        // GIVEN: a fresh manager
        let mgr = SpaceManager::new();
        // THEN: no agent spaces tracked
        assert!(mgr.agent_space_ids().is_empty());
    }

    #[test]
    fn move_windows_to_space_rejects_empty_window_list() {
        // GIVEN: a manager and a plausible space ID
        let mgr = SpaceManager::new();
        // WHEN: moving empty window list
        let result = mgr.move_windows_to_space(&[], 42);
        // THEN: NoWindows error
        assert!(matches!(result, Err(SpaceError::NoWindows)));
    }

    #[test]
    fn space_error_not_agent_space_display_contains_id() {
        let err = SpaceError::NotAgentSpace(9876);
        let s = err.to_string();
        assert!(s.contains("9876"));
    }

    #[test]
    fn space_error_not_found_display_contains_id() {
        let err = SpaceError::NotFound(42);
        assert!(err.to_string().contains("42"));
    }

    // -----------------------------------------------------------------------
    // list_spaces — integration (requires macOS)
    // -----------------------------------------------------------------------

    #[test]
    fn list_spaces_returns_at_least_one_space() {
        // Integration: every Mac has at least one Space.
        let mgr = SpaceManager::new();
        let spaces = mgr.list_spaces().expect("must list spaces");
        assert!(!spaces.is_empty(), "at least one Space must exist");
    }

    #[test]
    fn list_spaces_has_exactly_one_active() {
        let mgr = SpaceManager::new();
        let spaces = mgr.list_spaces().expect("must list spaces");
        let active_count = spaces.iter().filter(|s| s.is_active).count();
        assert_eq!(active_count, 1, "exactly one Space is active at a time");
    }

    #[test]
    fn list_spaces_active_space_type_is_user_or_fullscreen() {
        let mgr = SpaceManager::new();
        let spaces = mgr.list_spaces().expect("must list spaces");
        let active = spaces.iter().find(|s| s.is_active).unwrap();
        assert!(
            matches!(
                active.space_type,
                CGSSpaceType::User | CGSSpaceType::FullScreen
            ),
            "active space must be user or full-screen, got {:?}",
            active.space_type
        );
    }

    #[test]
    fn list_spaces_none_are_agent_created_on_fresh_manager() {
        let mgr = SpaceManager::new();
        let spaces = mgr.list_spaces().expect("must list spaces");
        assert!(
            spaces.iter().all(|s| !s.is_agent_created),
            "no spaces should be agent-created on a fresh manager"
        );
    }

    // -----------------------------------------------------------------------
    // Space lifecycle — integration
    //
    // These tests require `CGSSpaceCreate` to succeed, which needs an active
    // WindowServer display session.  They will fail (SIGSEGV/SIGBUS) when run
    // headlessly (e.g. in `cargo test` on a build server or via SSH without a
    // display).  Run them manually in an interactive macOS session:
    //
    //   cargo test --features spaces --lib spaces::tests::create -- --ignored
    // -----------------------------------------------------------------------

    /// Create → verify → destroy lifecycle.
    ///
    /// Requires a WindowServer session (`CGSSpaceCreate` returns 0 without one).
    /// Run with `cargo test --features spaces -- --ignored`.
    #[test]
    #[ignore = "requires WindowServer session (interactive macOS only)"]
    fn create_and_destroy_space_round_trip() {
        let mgr = SpaceManager::new();

        let space = mgr.create_space().expect("must create space");
        assert_ne!(space.id, 0);
        assert!(mgr.agent_space_ids().contains(&space.id));

        // Space must appear in the full list and be marked as agent-created.
        let spaces = mgr.list_spaces().expect("must list spaces after create");
        let found = spaces.iter().find(|s| s.id == space.id);
        assert!(found.is_some(), "created space must appear in list");
        assert!(found.unwrap().is_agent_created);

        // Destroy it.
        mgr.destroy_space(space.id).expect("must destroy own space");
        assert!(!mgr.agent_space_ids().contains(&space.id));

        // Must no longer appear in the list.
        let spaces_after = mgr.list_spaces().expect("must list spaces after destroy");
        assert!(
            !spaces_after.iter().any(|s| s.id == space.id),
            "destroyed space must not appear in list"
        );
    }

    /// Create two spaces → destroy_all → verify both gone.
    #[test]
    #[ignore = "requires WindowServer session (interactive macOS only)"]
    fn destroy_all_agent_spaces_removes_all() {
        let mgr = SpaceManager::new();
        let s1 = mgr.create_space().expect("create space 1");
        let s2 = mgr.create_space().expect("create space 2");

        assert_eq!(mgr.agent_space_ids().len(), 2);
        mgr.destroy_all_agent_spaces();
        assert!(mgr.agent_space_ids().is_empty());

        // Verify both gone from the system list.
        let spaces = mgr.list_spaces().expect("list after destroy_all");
        assert!(!spaces.iter().any(|s| s.id == s1.id || s.id == s2.id));
    }

    /// Drop destroys agent-created spaces automatically.
    #[test]
    #[ignore = "requires WindowServer session (interactive macOS only)"]
    fn drop_destroys_agent_spaces() {
        let sid;
        {
            let mgr = SpaceManager::new();
            let space = mgr.create_space().expect("create space");
            sid = space.id;
        } // mgr drops here → should destroy the space

        // Verify the space no longer exists using a fresh manager.
        let checker = SpaceManager::new();
        let spaces = checker.list_spaces().expect("list spaces after drop");
        assert!(
            !spaces.iter().any(|s| s.id == sid),
            "dropped manager should have destroyed its spaces"
        );
    }

    /// `destroy_space` with an agent-space-ID-that-was-never-inserted returns NotAgentSpace.
    ///
    /// Verifies the safety invariant without calling any CGS mutation functions.
    #[test]
    fn destroy_space_with_plausible_id_rejects_if_not_agent_space() {
        // GIVEN: a fresh manager, not tracking any spaces
        let mgr = SpaceManager::new();
        // Obtain a real space ID from the system
        let spaces = mgr.list_spaces().unwrap_or_default();
        if let Some(s) = spaces.first() {
            // WHEN: attempting to destroy a *real* user space
            let result = mgr.destroy_space(s.id);
            // THEN: must refuse (NotAgentSpace) — never crash
            assert!(
                matches!(result, Err(SpaceError::NotAgentSpace(_))),
                "must refuse to destroy a user-created space"
            );
        }
    }
}
