# Finding Elements

AXTerminator provides multiple ways to locate UI elements, with automatic fallback through self-healing locators.

## Basic Finding

```bash
# Find by text -- searches title, description, value, label, and identifier
axterminator find "5" --app Calculator

# Find with timeout (milliseconds)
axterminator find "Save" --app Safari --timeout 5000
```

Or via MCP tool call:

```
ax_find query="5" app="Calculator"
ax_find query="Save" app="Safari" timeout_ms=5000
```

## Query Syntax

Use prefixes to specify search type:

```bash
# Simple text -- matches ANY of: title, description, value, label, identifier
axterminator find "Save" --app Safari

# By identifier
axterminator find "identifier:_NS:9" --app Safari

# By role
axterminator find "role:AXButton" --app Safari

# By title
axterminator find "title:Save" --app Safari

# By description (useful for apps like Calculator where buttons use AXDescription)
axterminator find "description:equals" --app Calculator

# By value
axterminator find "value:42" --app Calculator

# By label
axterminator find "label:Save button" --app Safari

# Combined (AND semantics -- all specified fields must match)
axterminator find "role:AXButton title:OK" --app Safari
```

> **Note:** Simple text queries (without a prefix) search across ALL text-bearing attributes using OR semantics. Prefixed queries use AND semantics -- every specified field must match.

## Finding Multiple Elements

```bash
# Find all buttons
axterminator find "role:AXButton" --app Safari --all
```

Or via MCP:

```
ax_find query="role:AXButton" app="Safari" multiple=true
```

## Element Properties

Inspect all attributes of a found element:

```bash
axterminator tree --app Safari
```

Or via MCP:

```
ax_get_attributes app="Safari" query="Save"
ax_get_tree app="Safari"
```

Attributes returned include: title, role, value, identifier, enabled, focused, and more.

## Hierarchical Navigation

Use the tree command to see the full element hierarchy:

```bash
# Full element hierarchy
axterminator tree --app Finder

# Inspect specific element attributes
axterminator find "role:AXToolbar" --app Finder
```

Or via MCP:

```
ax_get_tree app="Finder"
ax_find query="role:AXToolbar" app="Finder"
```

## Waiting for Elements

Use the timeout parameter to wait for elements to appear:

```bash
axterminator find "Done" --app Safari --timeout 5000
```

Or via MCP:

```
ax_find query="Done" app="Safari" timeout_ms=5000
ax_wait_idle app="Safari" timeout_ms=3000
```

## Error Handling

If an element is not found within the timeout, AXTerminator returns an error. Use `axterminator tree` or `ax_get_tree` to inspect the current element hierarchy and adjust your query.

Common issues:

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Element not found` for short labels | App uses `AXDescription` not `AXTitle` | Try `description:label` or inspect with `axterminator tree` |
| No results for role query | Wrong role name | Check `axterminator tree` output for actual role names |
| Timeout finding element | Element not yet rendered | Increase `--timeout` value |
