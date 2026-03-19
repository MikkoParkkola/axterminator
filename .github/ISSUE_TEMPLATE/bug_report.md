---
name: Bug Report
about: Report a bug or unexpected behavior
labels: bug
---

**Environment**
- macOS version:
- axterminator version (`axterminator --version`):
- Interface (MCP server / CLI / Python bindings):
- Hardware (Intel/Apple Silicon):

**What happened**

A clear description of the bug.

**What you expected**

What should have happened instead.

**Steps to reproduce**

CLI or MCP JSON-RPC reproduction (preferred):

```bash
# CLI
axterminator find "Save" --app Safari

# Or MCP JSON-RPC request sent to the server
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ax_find","arguments":{"app":"Safari","query":"Save"}}}' | axterminator mcp serve
```

If using the Python bindings:

```python
import axterminator as ax
# minimal reproduction
```

**Additional context**

Logs, screenshots, or error messages.
