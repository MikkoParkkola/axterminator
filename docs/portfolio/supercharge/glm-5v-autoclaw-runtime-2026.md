# MIK-3285 Research Memo: GLM-5V-Turbo, AutoClaw, and AXTerminator Runtime Fit

Evidence date: 2026-05-12

## Decision

GLM-5V-Turbo validates the same system split that AXTerminator already uses:
model as multimodal perception and planning layer; execution framework as the
hands. The research supports a provider-neutral GLM-style perception adapter for
AXTerminator, but it does not justify a vision-first default. ADR-0001 remains
binding: model output should be normalized into AX semantic actions whenever the
AX tree can resolve the target.

## Sources Reviewed

- GLM-V Team, "GLM-5V-Turbo: Toward a Native Foundation Model for Multimodal
  Agents", arXiv:2604.26752, version 2, last revised 2026-05-06:
  https://arxiv.org/abs/2604.26752
- Z.AI GLM-5V-Turbo developer documentation:
  https://docs.z.ai/guides/vlm/glm-5v-turbo
- Z.AI AutoClaw product page:
  https://autoglm.zhipuai.cn/autoclaw/
- Z.AI GLM skills repository:
  https://github.com/zai-org/GLM-skills

## Located References

The paper explicitly names the adjacent systems required by the ticket:

| Reference | Verbatim locator | Interpretation |
| --- | --- | --- |
| Claude Code and AutoClaw | "Claude Code and AutoClaw" | The paper treats these as external agent frameworks relevant to GLM-5V-Turbo deployment. |
| AutoClaw execution role | "AutoClaw provides the \"hands\"" | AutoClaw is framed as the execution layer for browser and GUI work. |
| Official skill invocation surface | "OpenClaw, AutoClaw and Claude Code" | GLM-5V-Turbo skills are intended to be callable from external agent frameworks. |

The Z.AI documentation also positions GLM-5V-Turbo as a multimodal coding model
with image, video, text, and file inputs, a 200K context window, function
calling, context caching, visual grounding, GUI autonomous exploration, frontend
recreation, code debugging from screenshots, and official skills for image
captioning, visual grounding, document-grounded writing, prompt generation, OCR,
table recognition, handwriting recognition, formula recognition, website
replication, and PRD-to-app workflows.

## AXTerminator Runtime Compatibility

Directly composable capabilities:

- Visual grounding can rank screenshot regions and bounding boxes, then map them
  back to AX candidates from `ax_get_tree`, `ax_find`, element bounds, role,
  title, label, value, and identifier.
- OCR, table recognition, formula recognition, and document-grounded writing can
  enrich AX tree summaries when a UI exposes weak labels or embeds important
  text in images.
- GUI autonomous exploration can propose next-action intents and target
  descriptions, while AXTerminator executes through `ax_click`, `ax_type`,
  `ax_scroll`, `ax_key_press`, and verifies through `ax_assert`,
  `ax_get_tree`, `ax_screenshot`, or visual diff.
- Frontend recreation, webpage reading, and visual code debugging are useful for
  diagnostics and demos: they can compare a rendered UI against expected visual
  structure, then let AXTerminator perform low-cost semantic interactions.
- The paper's design lenses match AXTerminator's roadmap: perception quality,
  hierarchical optimization, reliable task specification, controlled
  verification, and model-plus-harness co-design.

Not directly composable without guardrails:

- Raw coordinate actions should remain fallback-only. A GLM-style model can
  propose pixel targets, but AXTerminator should prefer semantic AX actions and
  use raw coordinates only when AX coverage is unavailable or low confidence.
- Z.AI MaaS and ClawHub skills are not a runtime dependency. AXTerminator should
  preserve provider neutrality and expose adapters for GLM, UI-TARS, CogAgent,
  or other VLM planners behind the same action contract.
- Cloud-only inference is not the default sovereign posture. Any GLM integration
  must make provider, endpoint, and data-flow boundaries explicit.
- Long-horizon visual memory remains unresolved. The paper itself identifies
  multimodal context management as a bottleneck, so AXTerminator should store
  compact screenshot hashes, AX tree hashes, element candidates, and verification
  evidence rather than retaining raw visual history by default.

## AutoClaw Compared With AXTerminator

AutoClaw, as described by the paper and the public product page, is an assistant
execution channel across browser and GUI tasks. The paper reference describes it
as Windows and macOS capable, with model hot-swapping, many skills, and AutoGLM
browser automation. The product page emphasizes an IM/chat entrypoint, task
decomposition, local tool execution, progress state, and context returning to
the chat thread.

AXTerminator differs in three important ways:

- AXTerminator is a macOS Accessibility execution layer first, not a full chat
  assistant. It exposes MCP and CLI tools that other agents can call.
- AXTerminator's moat is semantic execution: AX roles, labels, identifiers,
  self-healing locators, background operation, and assertion-based verification.
- AXTerminator can run underneath many agents. Claude Code, Codex, OpenClaw,
  AutoClaw-like shells, and future VLM planners can all treat it as the macOS
  hands layer.

Adoptable patterns:

- Make the brain/hands split explicit in user-facing positioning.
- Keep model hot-swapping at the planner/perception boundary.
- Package repeatable workflows as skills, but execute them through AXTerminator
  primitives and verification gates.
- Add a perception-critique loop: the model should critique target recognition
  before execution when AX evidence and screenshot evidence disagree.
- Track hierarchical evidence: perception candidate, grounded AX element,
  planned action, executed action, and verification result.

## Brand Validation Signal

The signal is external category validation, not dependency validation. A new GLM
paper names Claude Code and AutoClaw as reference execution frameworks while
framing GLM-5V-Turbo as the multimodal controller. That supports the
claude-elite and AXTerminator positioning:

- Claude Elite is the operating discipline and quality standard around
  best-of-breed agent brains.
- AXTerminator is the semantic macOS hands layer that turns model intent into
  fast, cheap, background-safe actions.
- The market is converging on harnesses, skills, memory, and execution layers as
  the durable surface around model brains.

Positioning update filed in
`/Users/mikko/.claude/data/portfolio/market-positioning-living.md`: use
"model brain plus semantic hands" and "harness-plus-hands layer" when discussing
Claude Elite plus AXTerminator. Avoid implying GLM-5V-Turbo, AutoClaw, or any
specific VLM becomes AXTerminator's default actuator.

## Acceptance Criteria Evidence

| AC | Evidence |
| --- | --- |
| MIK-3285.RESEARCH.1 | GLM-5V-Turbo paper and Z.AI docs reviewed. Verbatim references are recorded above. |
| MIK-3285.RESEARCH.2 | Runtime compatibility is split into directly composable capabilities and guardrailed/non-default capabilities. |
| MIK-3285.RESEARCH.3 | AutoClaw architecture and differences from AXTerminator are summarized with adoptable patterns. |
| MIK-3285.RESEARCH.4 | Brand validation signal is recorded as external category validation for model brain plus semantic hands. |
| MIK-3285.RESEARCH.5 | Claude Elite positioning update is filed in the canonical living market-positioning document. |

## Follow-On Work

- MIK-3286 should use this memo as upstream research and keep ADR-0001 as the
  binding execution rule.
- A future implementation ticket should define a provider-neutral perception
  adapter: `model observation -> ranked target candidates -> semantic AX action
  proposal -> AX execution -> verification evidence`.
- A benchmark slice should measure whether GLM-style visual grounding improves
  resolution on surfaces where AX coverage is below the README coverage gate.
