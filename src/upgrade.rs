//! Post-upgrade migration and "what's new" notification.
//!
//! # Version stamp
//!
//! On first run after an upgrade, `check_upgrade()` detects the version
//! change by comparing `~/.axterminator/version.stamp` with the compiled
//! `CARGO_PKG_VERSION`, prints what's new, and updates the stamp.
//!
//! # Migration registry
//!
//! `MIGRATIONS` is intentionally empty — axterminator has minimal config and
//! no persistent state that needs transforming.  The registry is the extension
//! point for future migrations.
//!
//! # Usage
//!
//! ```rust,no_run
//! use axterminator::upgrade::{check_upgrade, UpgradeOptions};
//!
//! // Called on every normal startup:
//! check_upgrade(&UpgradeOptions::default()).unwrap();
//!
//! // Called by `axterminator upgrade`:
//! check_upgrade(&UpgradeOptions { dry_run: true, quiet: false }).unwrap();
//! ```

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

// ---------------------------------------------------------------------------
// Version ordering
// ---------------------------------------------------------------------------

/// A parsed semantic version triple `(major, minor, patch)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SemVer(u32, u32, u32);

impl SemVer {
    /// Parse `"major.minor.patch"` — ignores pre-release suffixes.
    fn parse(s: &str) -> Option<Self> {
        let mut parts = s.splitn(3, '.').map(|p| {
            // Strip any pre-release suffix (e.g. "1-alpha" -> 1).
            p.split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|n| n.parse::<u32>().ok())
        });
        Some(Self(parts.next()??, parts.next()??, parts.next()??))
    }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

// ---------------------------------------------------------------------------
// Migration registry (empty framework)
// ---------------------------------------------------------------------------

/// A migration that may run when upgrading from `from_version`.
///
/// Migrations are skipped on dry-run and when `from_version >= since`.
struct Migration {
    /// First version that requires this migration (i.e. stamp < `since`).
    since: SemVer,
    /// Human-readable description shown during upgrade.
    description: &'static str,
    /// The migration function. Called once, returns `Ok(())` on success.
    run: fn() -> Result<()>,
}

/// Registered migrations, sorted ascending by `since`.
///
/// Add entries here when a new version needs a one-time config transform.
/// Keep the list sorted; migrations execute in order.
static MIGRATIONS: &[Migration] = &[
    // Example (disabled):
    //
    // Migration {
    //     since: SemVer(1, 0, 0),
    //     description: "Migrate legacy config key 'foo' → 'bar'",
    //     run: migrate_v1_0_0,
    // },
];

// ---------------------------------------------------------------------------
// What's new
// ---------------------------------------------------------------------------

/// "What's new" entries shown when upgrading to a specific version.
struct WhatsNew {
    /// Version that introduced these changes.
    version: SemVer,
    /// Bullet points shown to the user.
    items: &'static [&'static str],
}

static WHATS_NEW: &[WhatsNew] = &[WhatsNew {
    version: SemVer(0, 9, 0),
    items: &[
        "New `upgrade` command with version stamp and migration framework",
        "Shell completions for `upgrade` subcommand",
    ],
}];

/// Print "what's new" items for all versions strictly after `from`.
fn print_whats_new(from: SemVer, current: SemVer) {
    let items: Vec<_> = WHATS_NEW
        .iter()
        .filter(|w| w.version > from && w.version <= current)
        .flat_map(|w| w.items.iter())
        .collect();

    if items.is_empty() {
        return;
    }

    println!("What's new in v{current}:");
    for item in items {
        println!("  - {item}");
    }
}

// ---------------------------------------------------------------------------
// Stamp file I/O
// ---------------------------------------------------------------------------

/// Path to the version stamp file.
pub fn stamp_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    Ok(PathBuf::from(home)
        .join(".axterminator")
        .join("version.stamp"))
}

/// Read the version string from the stamp file.
///
/// Returns `None` when the file does not exist (fresh install).
pub fn read_stamp(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s.trim().to_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read stamp {}", path.display())),
    }
}

/// Write `version` to the stamp file, creating parent directories as needed.
pub fn write_stamp(path: &Path, version: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create stamp directory {}", parent.display()))?;
    }
    std::fs::write(path, version).with_context(|| format!("write stamp {}", path.display()))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Options controlling upgrade behaviour.
#[derive(Debug, Clone, Default)]
pub struct UpgradeOptions {
    /// Print what would happen but do not update the stamp or run migrations.
    pub dry_run: bool,
    /// Suppress all non-error output.
    pub quiet: bool,
}

/// Check the version stamp and act on any version change.
///
/// This is the single entry point for both automatic startup checks and
/// the explicit `axterminator upgrade` command.
///
/// # Behaviour
///
/// | Stamp state        | Action                                           |
/// |--------------------|--------------------------------------------------|
/// | Missing            | Fresh install — write current version, no output |
/// | `stamp == current` | No-op                                            |
/// | `stamp < current`  | Print what's new, run migrations, update stamp   |
/// | `stamp > current`  | Warn about downgrade, no stamp update            |
///
/// On `dry_run`, the stamp is never modified and migrations are skipped.
pub fn check_upgrade(opts: &UpgradeOptions) -> Result<UpgradeOutcome> {
    let path = stamp_path()?;
    let current_str = env!("CARGO_PKG_VERSION");
    let current = SemVer::parse(current_str)
        .with_context(|| format!("cannot parse CARGO_PKG_VERSION {current_str:?}"))?;

    let stamp_raw = read_stamp(&path)?;

    let outcome = match stamp_raw.as_deref() {
        None => handle_fresh_install(&path, current_str, opts)?,
        Some(s) => match SemVer::parse(s) {
            None => handle_corrupt_stamp(&path, s, current_str, opts)?,
            Some(stamp) => match stamp.cmp(&current) {
                Ordering::Equal => UpgradeOutcome::UpToDate,
                Ordering::Less => handle_upgrade(&path, stamp, current, current_str, opts)?,
                Ordering::Greater => handle_downgrade(stamp, current, opts),
            },
        },
    };

    if !opts.quiet {
        print_outcome_summary(&outcome, current_str);
    }

    Ok(outcome)
}

// ---------------------------------------------------------------------------
// Outcome
// ---------------------------------------------------------------------------

/// Result of a `check_upgrade()` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeOutcome {
    /// First run — stamp did not exist.
    FreshInstall,
    /// Already on the current version.
    UpToDate,
    /// Upgraded from `from` to `to`.
    Upgraded { from: String, to: String },
    /// Stamp version is newer than the binary (downgrade detected).
    Downgrade { stamp: String, binary: String },
    /// Stamp contained an unparseable version string.
    CorruptStamp,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

fn handle_fresh_install(
    path: &Path,
    current: &str,
    opts: &UpgradeOptions,
) -> Result<UpgradeOutcome> {
    if !opts.dry_run {
        write_stamp(path, current)?;
    }
    Ok(UpgradeOutcome::FreshInstall)
}

fn handle_corrupt_stamp(
    path: &Path,
    bad: &str,
    current: &str,
    opts: &UpgradeOptions,
) -> Result<UpgradeOutcome> {
    if !opts.quiet {
        eprintln!(
            "axterminator: warning: version stamp contains unrecognised value {bad:?}, resetting"
        );
    }
    if !opts.dry_run {
        write_stamp(path, current)?;
    }
    Ok(UpgradeOutcome::CorruptStamp)
}

fn handle_upgrade(
    path: &Path,
    from: SemVer,
    to: SemVer,
    to_str: &str,
    opts: &UpgradeOptions,
) -> Result<UpgradeOutcome> {
    if !opts.quiet {
        print_whats_new(from, to);
    }

    for migration in MIGRATIONS.iter().filter(|m| m.since > from) {
        if opts.dry_run {
            if !opts.quiet {
                println!("  [dry-run] would run migration: {}", migration.description);
            }
        } else {
            if !opts.quiet {
                println!("  Running migration: {}", migration.description);
            }
            (migration.run)()?;
        }
    }

    if !opts.dry_run {
        write_stamp(path, to_str)?;
    }

    Ok(UpgradeOutcome::Upgraded {
        from: from.to_string(),
        to: to.to_string(),
    })
}

fn handle_downgrade(stamp: SemVer, binary: SemVer, opts: &UpgradeOptions) -> UpgradeOutcome {
    if !opts.quiet {
        eprintln!(
            "axterminator: warning: stamp version v{stamp} is newer than binary v{binary} — \
             downgrade detected. Run `axterminator upgrade` to reset the stamp."
        );
    }
    UpgradeOutcome::Downgrade {
        stamp: stamp.to_string(),
        binary: binary.to_string(),
    }
}

fn print_outcome_summary(outcome: &UpgradeOutcome, current: &str) {
    match outcome {
        UpgradeOutcome::FreshInstall => {
            println!("axterminator v{current} — fresh install, stamp created.");
        }
        UpgradeOutcome::UpToDate
        | UpgradeOutcome::Downgrade { .. }
        | UpgradeOutcome::CorruptStamp => {}
        UpgradeOutcome::Upgraded { from, to } => {
            println!("axterminator upgraded v{from} → v{to}");
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        tempfile::Builder::new()
            .prefix("axt-upgrade-")
            .tempdir()
            .unwrap()
    }

    // ── SemVer parsing ───────────────────────────────────────────────────

    #[test]
    fn semver_parse_standard_triple() {
        // GIVEN: canonical "major.minor.patch"
        // WHEN: parsed
        // THEN: all components extracted
        assert_eq!(SemVer::parse("1.2.3"), Some(SemVer(1, 2, 3)));
    }

    #[test]
    fn semver_parse_strips_prerelease_suffix() {
        // GIVEN: version with pre-release label
        // WHEN: parsed
        // THEN: numeric parts kept, suffix discarded
        assert_eq!(SemVer::parse("0.9.0-alpha"), Some(SemVer(0, 9, 0)));
    }

    #[test]
    fn semver_parse_rejects_garbage() {
        // GIVEN: non-version string
        // WHEN: parsed
        // THEN: None returned
        assert_eq!(SemVer::parse("notaversion"), None);
        assert_eq!(SemVer::parse(""), None);
        assert_eq!(SemVer::parse("1.2"), None);
    }

    #[test]
    fn semver_ordering_is_correct() {
        // GIVEN: two versions
        // WHEN: compared
        // THEN: lexicographic semver order
        assert!(SemVer(1, 0, 0) > SemVer(0, 9, 99));
        assert!(SemVer(0, 9, 1) > SemVer(0, 9, 0));
        assert_eq!(SemVer(1, 2, 3), SemVer(1, 2, 3));
    }

    // ── stamp I/O ────────────────────────────────────────────────────────

    #[test]
    fn read_stamp_returns_none_when_missing() {
        // GIVEN: non-existent path
        let dir = temp_dir();
        let path = dir.path().join("version.stamp");
        // WHEN: read
        // THEN: None (not an error)
        assert_eq!(read_stamp(&path).unwrap(), None);
    }

    #[test]
    fn write_then_read_stamp_roundtrips() {
        // GIVEN: a temp path
        let dir = temp_dir();
        let path = dir.path().join("version.stamp");
        // WHEN: written and read back
        write_stamp(&path, "1.2.3").unwrap();
        // THEN: same string returned
        assert_eq!(read_stamp(&path).unwrap().as_deref(), Some("1.2.3"));
    }

    #[test]
    fn write_stamp_creates_parent_dirs() {
        // GIVEN: nested path whose parents don't exist
        let dir = temp_dir();
        let path = dir.path().join("nested").join("deep").join("version.stamp");
        // WHEN: written
        // THEN: no error, file exists
        write_stamp(&path, "0.1.0").unwrap();
        assert!(path.exists());
    }

    #[test]
    fn write_stamp_strips_trailing_whitespace_on_read() {
        // GIVEN: stamp written with trailing newline (common in editors)
        let dir = temp_dir();
        let path = dir.path().join("version.stamp");
        std::fs::write(&path, "0.9.0\n").unwrap();
        // WHEN: read
        // THEN: whitespace trimmed
        assert_eq!(read_stamp(&path).unwrap().as_deref(), Some("0.9.0"));
    }

    // ── check_upgrade scenarios ──────────────────────────────────────────

    fn opts_quiet_dry() -> UpgradeOptions {
        UpgradeOptions {
            dry_run: true,
            quiet: true,
        }
    }

    fn opts_quiet() -> UpgradeOptions {
        UpgradeOptions {
            dry_run: false,
            quiet: true,
        }
    }

    /// Invoke `check_upgrade` against a specific stamp path (not `~/.axterminator`).
    fn check_with_path(stamp: &Path, opts: &UpgradeOptions) -> Result<UpgradeOutcome> {
        // We call the internal helpers directly to avoid touching the real $HOME.
        let current_str = env!("CARGO_PKG_VERSION");
        let current = SemVer::parse(current_str).unwrap();
        let raw = read_stamp(stamp)?;
        match raw.as_deref() {
            None => handle_fresh_install(stamp, current_str, opts),
            Some(s) => match SemVer::parse(s) {
                None => handle_corrupt_stamp(stamp, s, current_str, opts),
                Some(v) => match v.cmp(&current) {
                    Ordering::Equal => Ok(UpgradeOutcome::UpToDate),
                    Ordering::Less => handle_upgrade(stamp, v, current, current_str, opts),
                    Ordering::Greater => Ok(handle_downgrade(v, current, opts)),
                },
            },
        }
    }

    #[test]
    fn fresh_install_writes_stamp_and_returns_fresh() {
        // GIVEN: no stamp file
        let dir = temp_dir();
        let stamp = dir.path().join("version.stamp");
        // WHEN: check_upgrade runs
        let outcome = check_with_path(&stamp, &opts_quiet()).unwrap();
        // THEN: FreshInstall outcome, stamp written
        assert_eq!(outcome, UpgradeOutcome::FreshInstall);
        assert!(stamp.exists());
    }

    #[test]
    fn fresh_install_dry_run_does_not_write_stamp() {
        // GIVEN: no stamp file, dry_run = true
        let dir = temp_dir();
        let stamp = dir.path().join("version.stamp");
        // WHEN: check_upgrade runs in dry-run mode
        let outcome = check_with_path(&stamp, &opts_quiet_dry()).unwrap();
        // THEN: FreshInstall, but stamp NOT created
        assert_eq!(outcome, UpgradeOutcome::FreshInstall);
        assert!(!stamp.exists());
    }

    #[test]
    fn up_to_date_stamp_is_noop() {
        // GIVEN: stamp == current version
        let dir = temp_dir();
        let stamp = dir.path().join("version.stamp");
        write_stamp(&stamp, env!("CARGO_PKG_VERSION")).unwrap();
        let mtime_before = std::fs::metadata(&stamp).unwrap().modified().unwrap();
        // WHEN: check_upgrade runs
        let outcome = check_with_path(&stamp, &opts_quiet()).unwrap();
        // THEN: UpToDate, stamp untouched
        assert_eq!(outcome, UpgradeOutcome::UpToDate);
        let mtime_after = std::fs::metadata(&stamp).unwrap().modified().unwrap();
        assert_eq!(mtime_before, mtime_after);
    }

    #[test]
    fn older_stamp_triggers_upgrade_and_updates_stamp() {
        // GIVEN: stamp at a version below current
        let dir = temp_dir();
        let stamp = dir.path().join("version.stamp");
        write_stamp(&stamp, "0.0.1").unwrap();
        // WHEN: check_upgrade runs
        let outcome = check_with_path(&stamp, &opts_quiet()).unwrap();
        // THEN: Upgraded outcome, stamp updated to current
        assert!(matches!(outcome, UpgradeOutcome::Upgraded { .. }));
        let new_stamp = read_stamp(&stamp).unwrap().unwrap();
        assert_eq!(new_stamp, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn older_stamp_dry_run_does_not_update_stamp() {
        // GIVEN: stamp below current, dry_run = true
        let dir = temp_dir();
        let stamp = dir.path().join("version.stamp");
        write_stamp(&stamp, "0.0.1").unwrap();
        // WHEN: dry-run check_upgrade
        let outcome = check_with_path(&stamp, &opts_quiet_dry()).unwrap();
        // THEN: Upgraded outcome reported, but stamp unchanged
        assert!(matches!(outcome, UpgradeOutcome::Upgraded { .. }));
        let unchanged = read_stamp(&stamp).unwrap().unwrap();
        assert_eq!(unchanged, "0.0.1");
    }

    #[test]
    fn newer_stamp_triggers_downgrade_warning() {
        // GIVEN: stamp at a version far above current
        let dir = temp_dir();
        let stamp = dir.path().join("version.stamp");
        write_stamp(&stamp, "99.0.0").unwrap();
        // WHEN: check_upgrade runs
        let outcome = check_with_path(&stamp, &opts_quiet()).unwrap();
        // THEN: Downgrade outcome
        assert!(matches!(outcome, UpgradeOutcome::Downgrade { .. }));
    }

    #[test]
    fn corrupt_stamp_resets_to_current() {
        // GIVEN: stamp contains garbage
        let dir = temp_dir();
        let stamp = dir.path().join("version.stamp");
        std::fs::write(&stamp, "not-a-version").unwrap();
        // WHEN: check_upgrade runs
        let outcome = check_with_path(&stamp, &opts_quiet()).unwrap();
        // THEN: CorruptStamp, stamp reset to current
        assert_eq!(outcome, UpgradeOutcome::CorruptStamp);
        let reset = read_stamp(&stamp).unwrap().unwrap();
        assert_eq!(reset, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn upgrade_outcome_from_contains_old_version() {
        // GIVEN: stamp at 0.0.1
        let dir = temp_dir();
        let stamp = dir.path().join("version.stamp");
        write_stamp(&stamp, "0.0.1").unwrap();
        // WHEN: upgraded
        let outcome = check_with_path(&stamp, &opts_quiet()).unwrap();
        // THEN: from field is "0.0.1"
        match outcome {
            UpgradeOutcome::Upgraded { from, .. } => assert_eq!(from, "0.0.1"),
            other => panic!("expected Upgraded, got {other:?}"),
        }
    }
}
