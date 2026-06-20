# axterminator (npm)

npm distribution for [**axterminator**](https://github.com/MikkoParkkola/axterminator) — an MCP server for macOS desktop automation that lets AI agents see and control macOS applications through the Accessibility API.

This package is a thin wrapper. On install it downloads the matching native binary from the corresponding GitHub release and verifies it against the published SHA-256 checksum before use.

## Requirements

- **macOS only** (arm64 or x64). axterminator drives the macOS Accessibility API; it does not run on Linux or Windows.
- Node.js >= 16 (for the installer only; the tool itself is a native binary).

## Use as an MCP server

Run directly:

```sh
npx axterminator mcp serve
```

Or wire it into an MCP client:

```json
{
  "mcpServers": {
    "axterminator": {
      "command": "axterminator",
      "args": ["mcp", "serve"]
    }
  }
}
```

(After a global install: `npm install -g axterminator`.)

## CLI

The wrapper forwards all arguments to the native binary, so the full CLI works too:

```sh
npx axterminator tree --app Finder
npx axterminator mcp serve --http 8080 --token secret
```

## How it works

`postinstall` runs `bin/install.js`, which:

1. Resolves the macOS target triple (`aarch64-apple-darwin` / `x86_64-apple-darwin`).
2. Downloads the binary + `checksums-sha256.txt` from the release tagged `v<package-version>`.
3. Verifies the SHA-256 and refuses to install on mismatch.
4. Writes the executable into the package `bin/` directory.

## License

See [LICENSE.md](https://github.com/MikkoParkkola/axterminator/blob/main/LICENSE.md) (AXTerminator Community License). For full docs, see the [main repository](https://github.com/MikkoParkkola/axterminator).
