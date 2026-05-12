# MIK-3227 SUPERCHARGE Memo: UI-TARS-2, AXTerminator, hebb, and nvfp4-mojo

Evidence date: 2026-05-12

## Decision

Proceed with UI-TARS-2 as the capability reference for a sovereign GUI-agent stack, but do not claim the full paper model can meet the sub-3GB Apple Silicon target. The practical near-term path is a smaller UI-TARS-family checkpoint, with UI-TARS-2B-SFT as the first on-device feasibility target and UI-TARS-1.5-7B as a higher-quality edge reference.

ADR alignment: [ADR-0001: AX-first With Vision Fallback](../../architecture/decisions/ADR-0001-ax-first-with-vision-fallback.md) remains binding. UI-TARS-style screen recognition and action prediction are planner inputs. AXTerminator remains the semantic execution and verification layer, using raw coordinates only as a controlled fallback.

Recommendation:

- Go for architecture, quantization experiments, and a small-checkpoint local spike.
- No-go for bundling or promising the 230B-total UI-TARS-2 paper model as a sub-3GB local artifact.
- Keep the product claim as "model brain plus semantic hands": UI-TARS-family perception proposes actions; AXTerminator executes and verifies through macOS Accessibility.

## Acceptance Criteria

- MIK-3227.SUPER.1: evaluated UI-TARS-2 model size, quality, license posture, and Apple Silicon feasibility.
- MIK-3227.SUPER.2: designed the AXTerminator integration where UI-TARS-style screen-state to action prediction is normalized into semantic AX actions.
- MIK-3227.SUPER.3: designed an nvfp4-mojo quantization path and scoped the sub-3GB target to a realistic small-checkpoint tier.
- MIK-3227.SUPER.4: designed hebb integration for GUI session memory and user-pattern learning without bypassing live assertions.
- MIK-3227.SUPER.5: this memo is the requested artifact at `docs/portfolio/supercharge/ui-tars-2-axterminator-2026.md`.

## Model Evaluation

| Dimension | Finding | Product consequence |
| --- | --- | --- |
| Paper model size | UI-TARS-2 is initialized from Seed-thinking-1.6 and described as a MoE model with 23B active parameters and 230B total parameters. | The full model is not an Apple-laptop sub-3GB target. Treat it as a teacher, evaluator, or hosted/edge-server capability target. |
| Benchmark quality | The technical report reports 47.5 OSWorld, 50.6 WindowsAgentArena, 73.3 AndroidWorld, and 88.2 Online-Mind2Web. | Strong enough to justify UI-TARS-style planning as the first GUI-agent adapter target. |
| Public license posture | `bytedance/UI-TARS` and `bytedance/UI-TARS-desktop` are Apache-2.0 repositories. Hugging Face lists `ByteDance-Seed/UI-TARS-1.5-7B` and `ByteDance-Seed/UI-TARS-2B-SFT` as Apache-2.0. | The open-weight path is plausible for the smaller released checkpoints, but productization must recheck the exact checkpoint, license, and attribution before bundling. |
| UI-TARS-2 full-weight availability | Public Hugging Face search surfaced `UI-TARS-2B-SFT`, not an obvious 230B UI-TARS-2 package. | Do not build a critical path on unreleased or unpinned 230B weights. |
| Apple Silicon feasibility | A 2.44B-parameter checkpoint can plausibly fit under 3GB after FP4-style quantization plus runtime budgeting. A 7B checkpoint is likely above 3GB once scales, tokenizer assets, KV cache, and runtime overhead are counted. | Set sub-3GB as a 2B-class target. Treat 7B as a higher-quality local/edge reference unless deeper mixed-bit compression proves otherwise. |

## AXTerminator Integration

The integration contract keeps model intelligence and operating-system actuation separate:

1. Observation: capture screenshot, AX tree summary, window metadata, previous action result, and hebb memory hints.
2. Planner: UI-TARS-family model predicts intent, target descriptor, action primitive, confidence, and reasoning.
3. Resolver: AXTerminator maps the target descriptor to semantic candidates through `ax_find`, roles, labels, values, identifiers, bounds, and optional visual grounding.
4. Executor: AXTerminator runs `ax_click`, `ax_type`, `ax_scroll`, `ax_key_press`, `ax_set_value`, or workflow steps.
5. Verifier: `ax_assert`, screenshot diff, AX tree hash, and action-specific checks decide whether to continue, repair, or ask for human review.

The core action envelope should be stable across model providers:

```json
{
  "goal": "Send a short reply in the active chat app",
  "observation_id": "sha256:screenshot+ax-tree",
  "model": "ui-tars-family",
  "action": "click|type|scroll|key_press|wait|ask_human",
  "target": {
    "text": "Reply",
    "role": "AXTextArea",
    "bounds_hint": [0, 0, 0, 0],
    "visual_description": "message composer near bottom of window"
  },
  "input": "Acknowledged. I will send the update today.",
  "confidence": 0.0,
  "reason": "Short planner rationale for audit and correction."
}
```

Guardrails:

- AX-first With Vision Fallback is mandatory. Coordinates are a fallback, not the default actuator.
- Memory can rank candidates, but only current AX/screenshot evidence can authorize execution.
- Destructive or privacy-sensitive actions require explicit confirmation until task-specific policies prove safe.
- Every autonomous step records planned action, executed action, verification result, and repair decision.

## nvfp4-mojo Quantization Path

`nvfp4-mojo` is the quantization and kernel research vehicle, not yet a finished Apple Silicon runtime. The current local evidence shows NVFP4 ModelOpt FP4 kernels implemented in Mojo for MAX, E2M1 FP4 values, FP8 E4M3 blockscales, group size 16, two FP4 values per byte, safetensors/GGUF loading work, fused dequantization in GEMM, working SGLang integration for SM121, and pending MAX integration.

Target tiers:

| Tier | Candidate | Role | Footprint decision |
| --- | --- | --- | --- |
| A | UI-TARS-2B-SFT | First sub-3GB sovereign local spike. | Plausible after FP4 packing, scale budgeting, lazy assets, and bounded KV cache. |
| B | UI-TARS-1.5-7B | Quality and adapter reference. | Do not promise sub-3GB. Target a larger local/edge budget first. |
| C | UI-TARS-2 230B-total / 23B-active MoE | Capability teacher, hosted evaluator, or DGX/Spark-class edge server target. | Not a local sub-3GB target. |

Quantization plan:

1. Pin the exact Hugging Face checkpoint, architecture, tokenizer, and license.
2. Export linear weights from safetensors and convert eligible matrices to NVFP4 packed storage.
3. Preserve per-group scaling metadata and avoid materializing FP16 intermediates in the hot path.
4. Validate layer-level numeric error against FP16/BF16 on representative image-plus-instruction prompts.
5. Run a tiny GUI-action benchmark with AXTerminator verification, not just perplexity or text generation.
6. Record a budget table that includes weights, scales, tokenizer/assets, KV cache, runtime, and memory fragmentation.

Kill gates:

- Stop the sub-3GB claim if measured memory exceeds 3GB after warmup on the target hardware.
- Stop bundling work if the target checkpoint license or redistribution terms are not explicitly compatible.
- Stop quality claims if FP4 action selection regresses below the unquantized baseline on the smoke benchmark.

## hebb Session Memory

hebb should provide durable, local GUI session memory:

- Selector priors: prior successful element labels, roles, identifiers, and window/app contexts.
- Workflow fragments: recurring action sequences with their verification evidence.
- User preferences: app-specific default choices and safe confirmation preferences.
- Failure memory: selectors, coordinates, or action forms that failed and should be deprioritized.
- Correction traces: human corrections tied to screenshot hash, AX tree hash, and task intent.

Trace schema:

```json
{
  "task_id": "MIK-3227-demo-001",
  "app": "Finder",
  "window": "Downloads",
  "goal": "Open the newest PDF",
  "screenshot_hash": "sha256:...",
  "ax_tree_hash": "sha256:...",
  "hebb_keys": ["selector-prior:finder:downloads:newest-file"],
  "model_action": {"action": "click", "target": "newest PDF in list"},
  "ax_action": {"tool": "ax_click", "query": "role:AXRow title:*.pdf"},
  "verification": {"assertion": "focused row title ends with .pdf", "passed": true},
  "human_correction": null
}
```

Memory policy:

- hebb recall can bias candidate ranking and repair choice.
- hebb recall cannot override `ax_assert`, screenshot diff, or current AX tree evidence.
- Store only task-relevant UI metadata and avoid unnecessary user-content retention.
- Use `remember` for durable facts, `decide` for explicit policy choices, and `replay` for session recovery.

## Agent Stack Bets

- B1-IDENT: this memo is attributable to MIK-3227 and should be linked from the PR and Linear issue.
- B2-MEM: hebb is the local memory integration target, with selector priors and correction traces as the first slice.
- B3-DURABLE: the decision is durable through this memo plus ADR-0001.
- B4-PLATFORM: the stack combines AXTerminator execution, UI-TARS-family planning, hebb memory, and nvfp4-mojo quantization research.

## Next Implementation Slice

1. Add a provider-neutral action adapter type in AXTerminator for model-proposed GUI actions.
2. Add a trace format for screenshot hash, AX tree hash, hebb keys, model action, executed AX action, and verification result.
3. Build a 5-task local benchmark that runs in dry-run mode without Accessibility permission and in live mode with `axterminator check` passing.
4. Prototype conversion for UI-TARS-2B-SFT and record measured artifact size plus warm memory.
5. Revisit UI-TARS-1.5-7B only after the 2B adapter and verification benchmark are stable.

## Sources

- UI-TARS-2 technical report: https://arxiv.org/abs/2509.02544
- UI-TARS repository: https://github.com/bytedance/UI-TARS
- UI-TARS Desktop repository: https://github.com/bytedance/UI-TARS-desktop
- UI-TARS-1.5-7B model card: https://huggingface.co/ByteDance-Seed/UI-TARS-1.5-7B
- UI-TARS-2B-SFT model card: https://huggingface.co/ByteDance-Seed/UI-TARS-2B-SFT
- MIK-3286 stack-selection memo: [cogagent-stack-2026.md](cogagent-stack-2026.md)
- MIK-3285 GLM-5V and AutoClaw runtime-fit memo: [glm-5v-autoclaw-runtime-2026.md](glm-5v-autoclaw-runtime-2026.md)
