# ADR-0001: AX-first With Vision Fallback

Status: Accepted
Date: 2026-05-12
Tracking: MIK-3305, MIK-3285, MIK-3286

## Decision

AXTerminator remains AX-first with vision fallback. The default execution path
is the macOS Accessibility tree, self-healing locators, and semantic MCP/CLI
actions. Vision and multimodal GUI models are allowed as perception, planning,
ranking, and fallback layers, but they do not become the default actuator for
macOS GUI work.

This ADR supersedes any interpretation of MIK-3285 or MIK-3286 that would make
screenshot-to-action or vision-first control the default macOS path. It does
not supersede multimodal research. Model output may still propose intents,
targets, candidates, and plans, but AXTerminator should normalize those outputs
to semantic AX actions and verify them with AX assertions or screenshot/tree
deltas where possible.

## Source Evidence

`CLAUDE.md` currently defines the product direction:

> AX-first with vision-fallback means most interactions use the accessibility tree (fast, reliable, semantic), falling back to screenshot+vision only when AX is not available.

The locked decision table is stricter:

> AX-first with vision fallback (not vision-first)

The same table explicitly rejects:

> Make vision the default pipeline

`README.md` mirrors that public positioning: AXTerminator uses the AX semantic
tree as the opposite default to screenshot-to-pixel computer-use agents, with
vision reserved for canvas apps, games, and renderer surfaces that AX cannot
reach.

MIK-3285 asks for GLM-5V-Turbo and AutoClaw research, including which vision
capabilities are composable with AXTerminator's AX tree input. That is a
multimodal compatibility question, not a binding default-pipeline decision.

MIK-3286 is closer to the conflict. It frames CogAgent as multimodal GUI
understanding, names a screenshot-to-action pipeline in its acceptance
criteria, and the local SUPERCHARGE memo says UI-TARS-style desktop
computer-use actions map into AXTerminator primitives. The binding
interpretation is: screenshot-to-action models may feed the planner, but
AXTerminator remains the semantic execution and verification layer.

## Decision Matrix

| Dimension | AX-first default | Vision-first default | Decision |
| --- | --- | --- | --- |
| Latency | Sub-millisecond element access is already the product claim and benchmark target. | Every action pays screenshot capture, model inference, and coordinate dispatch latency. | AX-first wins for macOS automation. |
| Reliability | Semantic roles, labels, identifiers, and self-healing locators survive theme, font, and layout changes. | Pixel coordinates are brittle under visual drift, scaling, occlusion, and foreground-window changes. | AX-first wins for native and labeled UIs. |
| Maintenance cost | One semantic action contract (`ax_find`, `ax_click`, `ax_type`, `ax_assert`) serves CLI, MCP, tests, and model adapters. | Each model family needs prompt/action parsing, coordinate normalization, error recovery, and drift-specific verification. | AX-first wins as the stable core. |
| Capability ceiling | Cannot directly understand pure canvas, games, video, or non-AX-rendered regions without fallback. | Can inspect arbitrary screenshots and reason about visual-only surfaces. | Hybrid wins: AX-first with measured vision fallback. |

## Binding Rules

1. Default routing order is AX semantic lookup, self-healing locator recovery,
   app/web semantic fallback if available, then vision fallback.
2. A model may emit an intent, target descriptor, candidate element, or action
   proposal. AXTerminator must prefer converting that proposal into semantic
   AX actions before using raw coordinates.
3. Raw pixel actions are allowed only when AX is unavailable, low-confidence,
   or intentionally out of scope for the target surface.
4. Vision fallback work must preserve the AX coverage gate already documented
   in `README.md`: invest in vision defaults only when measured AX resolution
   on the target surface is below the documented threshold.
5. Any future change to vision-first defaults requires a superseding ADR with
   AX coverage data, latency data, reliability results, and a migration plan.
6. MIK-3285, MIK-3286, and follow-on multimodal issues must link this ADR and
   treat vision/model output as planner input unless they explicitly propose a
   superseding ADR.

## Consequences

AXTerminator's competitive moat stays coherent: background-safe, semantic,
cheap, and fast interaction on macOS. Multimodal work is still valuable because
it can expand coverage for canvas-like surfaces, improve candidate ranking, and
make cross-platform research practical. The cost is that model adapters must
translate into AXTerminator's action vocabulary instead of directly owning the
main control loop.

## Validation

- MIK-3305.ADR.1: `CLAUDE.md` AX-first wording read and quoted above.
- MIK-3305.ADR.2: MIK-3285 and MIK-3286 claims summarized above.
- MIK-3305.ADR.3: latency, reliability, maintenance cost, and capability
  ceiling evaluated in the decision matrix.
- MIK-3305.ADR.4: binding decision and explicit vision-first supersession are
  recorded in this ADR.
- MIK-3305.ADR.5: Hebb decision memory recorded as
  `memory:2qnn0x5qhyz856oiq1ii`; contradiction check returned no
  conflicting decisions.
- MIK-3305.ADR.6: `CLAUDE.md` and the MIK-3286 SUPERCHARGE memo link this ADR;
  MIK-3285/MIK-3286 Linear updates should link the merged PR or main-branch ADR.
