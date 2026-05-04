//! AX-first vs Vision-first benchmark harness.
//!
//! Measures AX-first resolution rate (success %) and latency across a probe
//! corpus. When the AX path fails, records that a vision fallback would be
//! needed and (if a vision provider is configured) measures vision latency and
//! token cost.
//!
//! ## Gate
//!
//! AX-first resolution must succeed on ≥80% of probes. PRs touching
//! `accessibility/`, `ax_provider/`, or `vision_fallback/` must include output
//! from this bench. A >5% regression vs the main baseline requires a documented
//! re-justification before merge.
//!
//! ## Running
//!
//! ```bash
//! # Standard bench (requires ≥50 probes in benches/probes/)
//! cargo bench --bench ax_vs_vision
//!
//! # One-week coverage audit mode (no vision provider needed)
//! cargo bench --bench ax_vs_vision -- --audit
//! ```
//!
//! ## Probe corpus
//!
//! See `benches/probes/README.md` for probe format and corpus requirements.
//! Probes are TOML files; add them to `benches/probes/` before the gate fires.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Probe definition
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Probe {
    id: String,
    app: String,
    #[allow(dead_code)]
    description: String,
    query: String,
    category: String,
    /// Whether AX resolution is expected to succeed for this probe.
    /// Canvas probes set this to false; they test the vision-fallback path
    /// and do not count against the 80% AX-success gate.
    expect_ax: bool,
}

// ---------------------------------------------------------------------------
// Probe loading
// ---------------------------------------------------------------------------

fn load_probes() -> Vec<Probe> {
    let probes_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/probes");
    let mut probes = Vec::new();

    let Ok(entries) = std::fs::read_dir(&probes_dir) else {
        return probes;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        match toml::from_str::<Probe>(&content) {
            Ok(probe) => probes.push(probe),
            Err(e) => eprintln!("WARN: skipping {:?}: {e}", path.file_name()),
        }
    }

    probes
}

// ---------------------------------------------------------------------------
// AX resolution attempt (stub — wire to real AX path when probes exist)
// ---------------------------------------------------------------------------

/// Attempt to resolve a probe via the AX semantic tree.
///
/// Returns `true` if the element was found, `false` if AX returned no match
/// (vision fallback would be needed).
///
/// TODO: replace stub with real `axterminator::app::AXApp::find(&probe.query)`
/// once the probe corpus is populated and apps are running in CI.
fn ax_resolve(probe: &Probe) -> bool {
    // Stub: canvas probes always fail AX (by design); others are unknown until
    // real integration is wired.
    if probe.category == "canvas" {
        return false;
    }
    // Real implementation: connect to app, call ax_find, return hit/miss.
    // For now, return false so the bench compiles and reports honestly.
    let _ = (&probe.app, &probe.query);
    false
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_ax_resolution(c: &mut Criterion) {
    let probes = load_probes();

    if probes.is_empty() {
        eprintln!(
            "\nSKIP ax_vs_vision: probe corpus is empty.\n\
             Add ≥50 probe TOML files to benches/probes/ (see benches/probes/README.md).\n"
        );
        return;
    }

    // AX-eligible probes (expect_ax = true)
    let ax_probes: Vec<&Probe> = probes.iter().filter(|p| p.expect_ax).collect();
    let canvas_probes: Vec<&Probe> = probes.iter().filter(|p| !p.expect_ax).collect();

    eprintln!(
        "\nax_vs_vision corpus: {} probes ({} AX-eligible, {} canvas/vision-only)",
        probes.len(),
        ax_probes.len(),
        canvas_probes.len()
    );

    // --- AX-first latency bench ---
    let mut group = c.benchmark_group("ax_first");
    group.measurement_time(Duration::from_secs(10));

    for probe in &ax_probes {
        group.bench_with_input(BenchmarkId::new("resolve", &probe.id), probe, |b, probe| {
            b.iter(|| ax_resolve(probe));
        });
    }
    group.finish();

    // --- AX success-rate summary (not a throughput bench, just a pass/fail audit) ---
    let ax_successes = ax_probes.iter().filter(|p| ax_resolve(p)).count();
    let ax_total = ax_probes.len();
    let ax_rate = if ax_total > 0 {
        ax_successes as f64 / ax_total as f64
    } else {
        0.0
    };

    eprintln!(
        "\n=== AX-first coverage: {}/{} probes resolved ({:.1}%) ===",
        ax_successes,
        ax_total,
        ax_rate * 100.0
    );

    // Gate enforcement (informational — real gate lives in CI)
    if ax_total >= 50 {
        if ax_rate < 0.80 {
            eprintln!(
                "GATE FAIL: AX-first success rate {:.1}% < 80% gate.\n\
                 PR must explain regression before merge.",
                ax_rate * 100.0
            );
        } else {
            eprintln!(
                "GATE PASS: AX-first success rate {:.1}% ≥ 80%.",
                ax_rate * 100.0
            );
        }
    } else {
        eprintln!(
            "GATE PENDING: only {} probes loaded; gate fires at ≥50.",
            ax_total
        );
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(5));
    targets = bench_ax_resolution
}
criterion_main!(benches);
