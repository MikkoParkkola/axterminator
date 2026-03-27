use std::sync::{Mutex, OnceLock};

fn shared_lock(cell: &'static OnceLock<Mutex<()>>) -> &'static Mutex<()> {
    cell.get_or_init(|| Mutex::new(()))
}

/// Serializes tests that mutate `AXTERMINATOR_SECURITY_MODE`.
pub(crate) fn security_mode_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    shared_lock(&LOCK)
}

/// Serializes tests that mutate the shared audio capture session store.
#[cfg(feature = "audio")]
pub(crate) fn capture_session_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    shared_lock(&LOCK)
}

/// Serializes tests that touch AppKit-backed global state like clipboard and displays.
pub(crate) fn appkit_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    shared_lock(&LOCK)
}

/// Serializes tests that create and release global accessibility elements.
pub(crate) fn accessibility_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    shared_lock(&LOCK)
}
