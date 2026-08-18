# VLM Vision Detection

When the accessibility tree does not yield a match, the `ax_find_visual` MCP
tool can fall back to a vision model that locates the element from a
screenshot and a natural-language description.

## Where the Model Runs

AXTerminator does not run or configure a vision model itself, and it reads no
VLM API key. It hands the work to the MCP client through sampling
(`sampling/createMessage`), so the element is located by whichever model your
agent already uses.

## Usage

Call the tool with the connected app and a description:

```json
{
  "name": "ax_find_visual",
  "arguments": {
    "app": "Safari",
    "description": "the blue Save button in the toolbar"
  }
}
```

## How It Works

1. If `AXTERMINATOR_PRIORITY_MODE=explicit` and the accessibility tree already
   holds a candidate scoring at least 0.3 for the description, the tool returns
   that element with `visual_skipped: true` and never takes a screenshot.
2. Otherwise the tool checks whether the client advertises the MCP sampling
   capability. If it does not, the call returns an error suggesting you capture
   the app with `ax_screenshot` and run an external vision model yourself.
3. With sampling available, the tool takes a screenshot and returns
   `screenshot_b64` plus a ready-made `sampling_request` for
   `sampling/createMessage`.
4. Your client sends that request to its own model, which replies with
   `{"found": true, "x": <int>, "y": <int>, "description": "..."}`.
5. Feed those coordinates to `ax_click_at`.

```
ax_find_visual  ->  screenshot + sampling request
                        |
              client model (sampling/createMessage)
                        |
              {"found": true, "x": 450, "y": 120}
                        |
                   ax_click_at
```

## Tips

1. **Be specific.** "blue Save button in toolbar" beats "save button".
2. **Use it as a fallback.** `ax_find` against the accessibility tree is faster
   and needs no model round trip.
3. **Set `AXTERMINATOR_PRIORITY_MODE=explicit`** if you want the accessibility
   tree to win whenever it can answer, which skips the screenshot entirely.
