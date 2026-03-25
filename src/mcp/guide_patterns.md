# Automation Patterns

Common step-by-step patterns for macOS UI automation with axterminator.

---

## Pattern 1 — Click a Button

Use when you need to press a labelled button in any application.

```
1. ax_connect   app_name="Safari"
2. ax_find      role="AXButton", label="Open"
3. ax_click     element=<result from step 2>
4. ax_assert    condition="button is no longer visible"  (optional)
```

**Tips**
- If multiple buttons share the same label, add `index=1` to `ax_find` to
  select the second match, or narrow the search with `parent` to scope it to
  a specific window or group.
- Use `ax_wait_idle` between the click and any subsequent find to let the UI
  settle before continuing.

---

## Pattern 2 — Fill a Form

Use when you need to populate text fields and submit a form.

```
1. ax_connect    app_name="Mail"
2. ax_find       role="AXTextField", label="To"
3. ax_click      element=<result from step 2>
4. ax_type       text="recipient@example.com"
5. ax_find       role="AXTextField", label="Subject"
6. ax_set_value  element=<result from step 5>, value="Meeting notes"
7. ax_find       role="AXButton", label="Send"
8. ax_click      element=<result from step 7>
```

**When to use `ax_type` vs `ax_set_value`**
- `ax_type` simulates real keystrokes — use it when the app listens to
  keyboard events (autocomplete, live validation).
- `ax_set_value` writes the value directly via the accessibility attribute —
  faster but bypasses keyboard event handlers.

---

## Pattern 3 — Extract Data

Use when you need to read text, values, or structured content from the UI.

```
1. ax_connect    app_name="Finder"
2. ax_find       role="AXTable", label="Files"
3. ax_get_tree   element=<result from step 2>, depth=2
4. ax_get_value  element=<specific row element>
```

**Tips**
- `ax_get_tree` with a shallow `depth` is cheaper than a full tree dump when
  you only need one subtree.
- `ax_query` accepts a natural-language question and returns matching elements
  with roles and values — useful when you do not know the exact tree structure.
- For tabular data, iterate over `AXRow` children of the `AXTable` element.

---

## Pattern 4 — Wait for a UI Change

Use when an action triggers an asynchronous operation (network request, animation,
file load) and you need to react once the UI has settled.

```
1. ax_connect    app_name="Chrome"
2. ax_find       role="AXButton", label="Submit"
3. ax_click      element=<result from step 2>
4. ax_wait_idle  timeout_ms=5000
5. ax_find       role="AXStaticText", label="Success"
6. ax_assert     element=<result from step 5>, attribute="AXValue", contains="Success"
```

**Tips**
- `ax_wait_idle` polls the accessibility tree for stability; it returns once
  no element changes are detected within the polling interval.
- For longer operations, combine `ax_wait_idle` with retried `ax_find` calls.
- Subscribe to the `axterminator://capture/status` resource for real-time
  change notifications during continuous capture sessions.

---

## Pattern 5 — Visual Fallback

Use when the accessibility tree is sparse, missing labels, or the element you
need cannot be found with role/label search (common in some Electron apps and
games).

```
1. ax_connect      app_name="Slack"
2. ax_screenshot   app_name="Slack"
3. ax_find_visual  screenshot=<result from step 2>, description="Send button"
4. ax_click        element=<result from step 3>
```

**When to prefer visual fallback**
- `ax_find` returns no results despite the element being visible on screen.
- The element has no accessible label or role (`AXUnknown`).
- The application renders its UI on a canvas or via a non-native framework.

**Tips**
- `ax_find_visual` uses a vision model to locate elements by visual description;
  be specific in `description` to reduce false matches.
- Prefer accessibility tree tools (`ax_find`) when they work — visual fallback
  is slower and depends on screenshot quality.
- `ax_app_profile` exposes CSS selectors for known Electron apps (VS Code,
  Slack, Chrome) which can be used as an alternative to both approaches.
