# Bench Probe Corpus — `ax_vs_vision`

This directory contains the probe corpus for `cargo bench --bench ax_vs_vision`.

## Gate

**PRs touching `accessibility/`, `ax_provider/`, or `vision_fallback/` must include
benchmark output from this harness.**

Pass gate: AX-first resolution succeeds on **≥80% of probes** (no vision fallback
needed). PRs that drop below this threshold must explain the regression before merge.
Regressions >5% vs the `main` baseline trigger a documented re-justification, not a
silent merge.

## Probe Format

Each probe is a TOML file:

```toml
# benches/probes/<id>.toml
id          = "finder-toolbar-new-folder"
app         = "Finder"               # bundle-name or bundle-id
description = "New Folder button in Finder toolbar"
query       = "New Folder"           # AX semantic query (passed to ax_find)
category    = "system"               # "system" | "third-party" | "canvas"

# Expected result
expect_ax   = true    # AX resolution should succeed
# If false: this probe is a canvas/vision-only surface
```

Fields:
| Field | Required | Description |
|-------|----------|-------------|
| `id` | ✅ | Unique kebab-case identifier |
| `app` | ✅ | App name or bundle ID |
| `description` | ✅ | Human-readable description of the target |
| `query` | ✅ | AX semantic query (same syntax as `axterminator find`) |
| `category` | ✅ | `system`, `third-party`, or `canvas` |
| `expect_ax` | ✅ | Whether AX resolution is expected to succeed |

## Coverage Audit (one-week gate)

Before implementing new vision-fallback features, run the AX coverage audit:

```bash
# Run audit mode: records which probes resolve via AX vs need vision
cargo bench --bench ax_vs_vision -- --audit

# Output: benches/probes/audit_results.json
# Shows: ax_success_rate, vision_needed_rate, per-probe breakdown
```

**Interpret results:**
- **>95% AX-resolvable** → vision fallback is a nice-to-have; ship positioning +
  bench numbers first
- **80–95% AX-resolvable** → vision fallback is useful but not urgent
- **<80% AX-resolvable** → vision fallback is the bottleneck; build it before
  competitive positioning work

The audit file `audit_results.json` is **not committed** (`.gitignore`). Run it
against your own app surface for one week, then interpret the gate above.

## Probe Corpus Requirements

The harness requires ≥50 probes before the 80% gate is enforced:

| Category | Minimum | Apps to cover |
|----------|---------|---------------|
| macOS system apps | 30 | Finder, Safari, Mail, Calendar, Notes, TextEdit, Terminal, System Settings, Activity Monitor, Contacts, Maps, Preview, Calculator, Reminders, Messages |
| Popular third-party | 15 | Slack, Chrome, VS Code, Figma, Notion, 1Password, Spotify, Zoom, Arc, Linear |
| Canvas-only surfaces | 5 | Figma canvas, game window, video player controls |

Canvas probes set `expect_ax = false` and benchmark the vision-fallback path
specifically; they do not count against the 80% gate (they are expected misses).

## Metrics per Probe

The harness records:

| Metric | AX path | Vision path |
|--------|---------|-------------|
| Resolution success | binary | binary |
| Latency (ms) | ✅ | ✅ |
| Token cost | n/a | ✅ (input + output tokens) |
| Fallback triggered | false | true |

## Status

- [ ] Probe corpus populated (≥50 probes required before gate is enforced)
- [ ] `ax_vs_vision` bench harness wired to real app interactions
- [ ] Audit baseline recorded on `main`
- [ ] CI integration added (`cargo bench --bench ax_vs_vision`)

The harness skeleton in `benches/ax_vs_vision.rs` compiles and runs; it reports
`SKIP: probe corpus empty` until probes are added.
