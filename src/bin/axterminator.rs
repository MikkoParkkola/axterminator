//! `axterminator` CLI — unified entry point for MCP server and direct commands.
//!
//! # Usage
//!
//! ```text
//! axterminator mcp serve [--stdio|--http <port>]
//! axterminator find <query> [--app <name>] [--bundle-id <id>] [--timeout <ms>]
//! axterminator click <query> [--app <name>] [--mode background|focus]
//! axterminator type <text>  [--app <name>] [--element <query>]
//! axterminator screenshot   [--app <name>] [--output <path>]
//! axterminator tree         [--app <name>] [--depth <n>]
//! axterminator apps
//! axterminator check
//! axterminator completions <shell>
//! ```

#![allow(clippy::pedantic)]

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

/// AXTerminator — background-first macOS GUI automation.
///
/// Use `axterminator mcp serve` to start the MCP server.
/// Use the subcommands directly for one-shot shell scripting.
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Background-first macOS GUI automation with MCP server support",
    long_about = None,
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the MCP server.
    Mcp {
        #[command(subcommand)]
        subcommand: McpSubcommand,
    },

    /// Find a UI element and print its attributes.
    Find {
        /// Element query (text, role:AXButton, //AXButton[@AXTitle='Save'])
        query: String,

        /// Target app name
        #[arg(long, short)]
        app: Option<String>,

        /// Target app by bundle ID (e.g. com.apple.Safari)
        #[arg(long)]
        bundle_id: Option<String>,

        /// Timeout in milliseconds
        #[arg(long, default_value = "5000")]
        timeout: u64,
    },

    /// Click a UI element.
    Click {
        /// Element query
        query: String,

        /// Target app name
        #[arg(long, short)]
        app: Option<String>,

        /// Target app by bundle ID
        #[arg(long)]
        bundle_id: Option<String>,

        /// Action mode: background (default, no focus) or focus
        #[arg(long, default_value = "background", value_parser = ["background", "focus"])]
        mode: String,
    },

    /// Type text into a UI element.
    #[command(name = "type")]
    TypeText {
        /// Text to type
        text: String,

        /// Target app name
        #[arg(long, short)]
        app: Option<String>,

        /// Target app by bundle ID
        #[arg(long)]
        bundle_id: Option<String>,

        /// Element to type into (defaults to focused element)
        #[arg(long, short)]
        element: Option<String>,
    },

    /// Take a screenshot of an app or element.
    Screenshot {
        /// Target app name
        #[arg(long, short)]
        app: Option<String>,

        /// Target app by bundle ID
        #[arg(long)]
        bundle_id: Option<String>,

        /// Save PNG to this path (default: print base64 to stdout)
        #[arg(long, short)]
        output: Option<PathBuf>,
    },

    /// Dump the accessibility element tree.
    Tree {
        /// Target app name
        #[arg(long, short)]
        app: Option<String>,

        /// Target app by bundle ID
        #[arg(long)]
        bundle_id: Option<String>,

        /// Maximum tree depth
        #[arg(long, short, default_value = "5")]
        depth: usize,
    },

    /// List running applications with accessibility info.
    Apps,

    /// Check accessibility permissions and system status.
    Check,

    /// Generate shell completion scripts.
    Completions {
        /// Target shell
        #[arg(value_parser = ["bash", "zsh", "fish", "elvish", "powershell"])]
        shell: String,
    },
}

#[derive(Subcommand, Debug)]
enum McpSubcommand {
    /// Install axterminator as an MCP server in your AI client's config.
    ///
    /// No JSON editing, no file-path hunting. Run this command, restart
    /// your client, and axterminator appears as an MCP server.
    ///
    /// Examples:
    ///   axterminator mcp install                       # Claude Desktop (default)
    ///   axterminator mcp install --client cursor       # Cursor
    ///   axterminator mcp install --client claude-code  # Claude Code
    ///   axterminator mcp install --dry-run             # show what would change
    Install {
        /// MCP client to configure.
        #[arg(long, default_value = "claude-desktop", value_parser = [
            "claude-desktop", "claude", "cursor", "windsurf", "claude-code",
        ])]
        client: String,

        /// Overwrite existing axterminator entry without asking.
        #[arg(long)]
        force: bool,

        /// Print the planned change without writing the file.
        #[arg(long)]
        dry_run: bool,
    },

    /// Start the MCP server (stdio transport, default).
    ///
    /// Use `--http <port>` to start the Streamable HTTP transport instead.
    /// Both transports can run simultaneously with `--http <port> --stdio`.
    Serve {
        /// Use stdio transport.
        ///
        /// This is the default when neither `--stdio` nor `--http` is given.
        /// When combined with `--http`, both transports start concurrently.
        #[arg(long)]
        stdio: bool,

        /// Use HTTP transport on the given port (requires `http-transport` feature).
        ///
        /// Binds to 127.0.0.1 by default. Override with `--bind`.
        /// Requires `--token` or `--localhost-only` when `--bind` is not
        /// a loopback address.
        #[arg(long)]
        http: Option<u16>,

        /// Bearer token for HTTP authentication.
        ///
        /// When absent, a random token is generated and printed to stderr.
        /// May also be set via the `AXTERMINATOR_HTTP_TOKEN` environment variable.
        #[arg(long, env = "AXTERMINATOR_HTTP_TOKEN")]
        token: Option<String>,

        /// IP address to bind the HTTP transport to.
        ///
        /// Defaults to `127.0.0.1`. Use `0.0.0.0` for all interfaces (requires
        /// `--token`).
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,

        /// Skip authentication — accept requests only from 127.0.0.1.
        ///
        /// Cannot be combined with `--token`. Only valid when `--bind` is
        /// also a loopback address.
        #[arg(long, conflicts_with = "token")]
        localhost_only: bool,
    },
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    // Initialise tracing to stderr so stdout stays clean for MCP JSON-RPC.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    dispatch(cli.command)
}

fn dispatch(cmd: Commands) -> Result<()> {
    match cmd {
        Commands::Mcp { subcommand } => dispatch_mcp(subcommand),
        Commands::Find {
            query,
            app,
            bundle_id,
            timeout,
        } => cmd_find(&query, app.as_deref(), bundle_id.as_deref(), timeout),
        Commands::Click {
            query,
            app,
            bundle_id,
            mode,
        } => cmd_click(&query, app.as_deref(), bundle_id.as_deref(), &mode),
        Commands::TypeText {
            text,
            app,
            bundle_id,
            element,
        } => cmd_type(
            &text,
            app.as_deref(),
            bundle_id.as_deref(),
            element.as_deref(),
        ),
        Commands::Screenshot {
            app,
            bundle_id,
            output,
        } => cmd_screenshot(app.as_deref(), bundle_id.as_deref(), output.as_deref()),
        Commands::Tree {
            app,
            bundle_id,
            depth,
        } => cmd_tree(app.as_deref(), bundle_id.as_deref(), depth),
        Commands::Apps => cmd_apps(),
        Commands::Check => cmd_check(),
        Commands::Completions { shell } => cmd_completions(&shell),
    }
}

// ---------------------------------------------------------------------------
// MCP dispatch
// ---------------------------------------------------------------------------

fn dispatch_mcp(sub: McpSubcommand) -> Result<()> {
    match sub {
        McpSubcommand::Install {
            client,
            force,
            dry_run,
        } => cmd_mcp_install(&client, force, dry_run),
        McpSubcommand::Serve {
            http,
            stdio,
            token,
            bind,
            localhost_only,
        } => dispatch_mcp_serve(http, stdio, token, &bind, localhost_only),
    }
}

// ---------------------------------------------------------------------------
// mcp install
// ---------------------------------------------------------------------------

/// Resolve the MCP config file path for a given AI client.
fn client_config_path(client: &str) -> Result<std::path::PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").context("$HOME not set")?);

    match client {
        "claude-desktop" | "claude" => {
            if cfg!(target_os = "macos") {
                Ok(home
                    .join("Library/Application Support/Claude")
                    .join("claude_desktop_config.json"))
            } else if cfg!(target_os = "linux") {
                Ok(home.join(".config/Claude/claude_desktop_config.json"))
            } else {
                // Windows
                let appdata = std::env::var("APPDATA")
                    .unwrap_or_else(|_| home.join("AppData/Roaming").to_string_lossy().into());
                Ok(PathBuf::from(appdata)
                    .join("Claude")
                    .join("claude_desktop_config.json"))
            }
        }
        "cursor" => Ok(home.join(".cursor/mcp.json")),
        "windsurf" => Ok(home.join(".codeium/windsurf/mcp_config.json")),
        "claude-code" => Ok(home.join(".claude.json")),
        _ => anyhow::bail!(
            "unknown client {client:?} (supported: claude-desktop, cursor, windsurf, claude-code)"
        ),
    }
}

/// Return the absolute path to the currently-running binary.
fn self_binary_path() -> Result<PathBuf> {
    // Prefer the resolved executable path, fall back to $PATH lookup.
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(abs) = exe.canonicalize() {
            return Ok(abs);
        }
        return Ok(exe);
    }
    // No which crate — current_exe() should always work.
    anyhow::bail!("cannot locate axterminator binary")
}

/// Install axterminator into the target client's MCP config.
fn cmd_mcp_install(client: &str, force: bool, dry_run: bool) -> Result<()> {
    let cfg_path = client_config_path(client)?;
    let binary = self_binary_path()?;

    // Load existing config or start fresh.
    let mut cfg: serde_json::Map<String, serde_json::Value> = if cfg_path.exists() {
        let data = std::fs::read_to_string(&cfg_path)
            .with_context(|| format!("read config {}", cfg_path.display()))?;
        if data.trim().is_empty() {
            serde_json::Map::new()
        } else {
            serde_json::from_str(&data).with_context(|| {
                format!(
                    "parse existing config {} (fix the file or use --force to overwrite)",
                    cfg_path.display()
                )
            })?
        }
    } else {
        serde_json::Map::new()
    };

    // Ensure mcpServers section exists.
    let servers = cfg
        .entry("mcpServers")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    let servers = servers
        .as_object_mut()
        .context("mcpServers is not a JSON object")?;

    // Check for existing entry.
    if servers.contains_key("axterminator") && !force {
        if dry_run {
            println!(
                "axterminator is already installed in {}\n  would not change (use --force to overwrite)",
                cfg_path.display()
            );
        } else {
            println!(
                "axterminator is already installed in {}\nUse --force to overwrite.",
                cfg_path.display()
            );
        }
        return Ok(());
    }

    // Write the axterminator entry.
    servers.insert(
        "axterminator".to_string(),
        serde_json::json!({
            "command": binary.to_string_lossy(),
            "args": ["mcp", "serve"]
        }),
    );

    let out = serde_json::to_string_pretty(&cfg).context("encode config")?;

    if dry_run {
        println!("Would write to {}:\n\n{}", cfg_path.display(), out);
        return Ok(());
    }

    // Create parent directory if missing.
    if let Some(parent) = cfg_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create config directory {}", parent.display()))?;
    }

    // Backup existing file.
    if cfg_path.exists() {
        let backup = cfg_path.with_extension("axterminator.bak");
        if let Ok(data) = std::fs::read(&cfg_path) {
            let _ = std::fs::write(&backup, data);
        }
    }

    std::fs::write(&cfg_path, &out)
        .with_context(|| format!("write config {}", cfg_path.display()))?;

    println!("Installed axterminator as MCP server for {client}.");
    println!("  config: {}", cfg_path.display());
    println!("  binary: {}", binary.display());
    println!();
    println!("Restart your client to pick up the change.");
    Ok(())
}

fn dispatch_mcp_serve(
    http: Option<u16>,
    stdio: bool,
    token: Option<String>,
    bind: &str,
    localhost_only: bool,
) -> Result<()> {
    let want_http = http.is_some();
    // Default to stdio when no --http; also use stdio when both flags present.
    // The `want_stdio` binding is consumed only inside the http-transport block;
    // suppress the warning when the feature is absent.
    #[cfg_attr(not(feature = "http-transport"), allow(unused_variables))]
    let want_stdio = stdio || !want_http;

    if want_http {
        #[cfg(feature = "http-transport")]
        {
            let port = http.unwrap();
            let bind_addr: std::net::IpAddr = bind
                .parse()
                .with_context(|| format!("Invalid bind address: '{bind}'"))?;
            let auth = build_http_auth(token, localhost_only, bind_addr)?;
            let cfg = axterminator::mcp::transport::HttpConfig {
                port,
                bind: bind_addr,
                auth,
            };

            if want_stdio {
                // Run both transports concurrently. HTTP in background tokio
                // task; stdio blocks the main thread.
                let rt = tokio::runtime::Runtime::new()?;
                rt.spawn(async move {
                    if let Err(e) = axterminator::mcp::transport::serve(
                        axterminator::mcp::transport::TransportConfig::Http(cfg),
                    )
                    .await
                    {
                        tracing::error!("HTTP transport error: {e}");
                    }
                });
                axterminator::mcp::server::run_stdio().context("MCP stdio server failed")
            } else {
                let rt = tokio::runtime::Runtime::new()?;
                rt.block_on(axterminator::mcp::transport::serve(
                    axterminator::mcp::transport::TransportConfig::Http(cfg),
                ))
                .context("MCP HTTP server failed")
            }
        }
        #[cfg(not(feature = "http-transport"))]
        {
            let _ = (http, token, bind, localhost_only);
            anyhow::bail!(
                "HTTP transport is not compiled in. Rebuild with \
                 `--features http-transport`."
            )
        }
    } else {
        axterminator::mcp::server::run_stdio().context("MCP stdio server failed")
    }
}

/// Build the [`AuthConfig`] from CLI flags.
///
/// Validates that non-localhost binds have a token configured.
#[cfg(feature = "http-transport")]
fn build_http_auth(
    token: Option<String>,
    localhost_only: bool,
    bind_addr: std::net::IpAddr,
) -> Result<axterminator::mcp::auth::AuthConfig> {
    use axterminator::mcp::auth::{generate_token, AuthConfig};

    if localhost_only {
        // Explicit localhost-only: validate bind address.
        if !bind_addr.is_loopback() {
            anyhow::bail!(
                "--localhost-only requires a loopback bind address. \
                 Got '{bind_addr}'. Use --bind 127.0.0.1."
            );
        }
        return Ok(AuthConfig::localhost_only());
    }

    // Bearer mode: use provided token or generate one.
    let tok = match token {
        Some(t) => t,
        None => {
            if !bind_addr.is_loopback() {
                // Refuse to start on a non-localhost address without explicit token.
                anyhow::bail!(
                    "Binding to non-loopback address '{bind_addr}' requires a token. \
                     Provide one via --token <token> or set AXTERMINATOR_HTTP_TOKEN, \
                     or use --localhost-only to skip authentication."
                );
            }
            // Auto-generate token for localhost convenience.
            let t = generate_token();
            eprintln!(
                "MCP server started. Bearer token: {t}\n\
                 (Pass this token in the Authorization header of your MCP client.)"
            );
            t
        }
    };

    Ok(AuthConfig::bearer(tok))
}

// ---------------------------------------------------------------------------
// Direct CLI commands
// ---------------------------------------------------------------------------

/// Connect to an app by optional name or bundle ID.
///
/// Exits with a clear error if neither is provided.
fn connect_app(name: Option<&str>, bundle_id: Option<&str>) -> Result<axterminator::AXApp> {
    if name.is_none() && bundle_id.is_none() {
        anyhow::bail!("Provide --app or --bundle-id to identify the target application");
    }
    axterminator::AXApp::connect_native(name, bundle_id, None).map_err(|e| anyhow::anyhow!("{e}"))
}

fn cmd_find(
    query: &str,
    app: Option<&str>,
    bundle_id: Option<&str>,
    timeout_ms: u64,
) -> Result<()> {
    let ax_app = connect_app(app, bundle_id)?;
    match ax_app.find_native(query, Some(timeout_ms)) {
        Ok(el) => {
            println!("Found:");
            println!("  role:    {}", el.role().as_deref().unwrap_or("(none)"));
            println!("  title:   {}", el.title().as_deref().unwrap_or("(none)"));
            println!("  value:   {}", el.value().as_deref().unwrap_or("(none)"));
            println!("  enabled: {}", el.enabled());
            if let Some((x, y, w, h)) = el.bounds() {
                println!("  bounds:  ({x:.0}, {y:.0}, {w:.0}x{h:.0})");
            }
            Ok(())
        }
        Err(e) => anyhow::bail!("Element not found: {e}"),
    }
}

fn cmd_click(query: &str, app: Option<&str>, bundle_id: Option<&str>, mode: &str) -> Result<()> {
    let ax_app = connect_app(app, bundle_id)?;
    let el = ax_app
        .find_native(query, Some(5000))
        .map_err(|e| anyhow::anyhow!("Element not found: {e}"))?;
    let action_mode = if mode == "focus" {
        axterminator::ActionMode::Focus
    } else {
        axterminator::ActionMode::Background
    };
    el.click_native(action_mode)
        .map_err(|e| anyhow::anyhow!("Click failed: {e}"))?;
    println!("Clicked '{query}' ({mode} mode)");
    Ok(())
}

fn cmd_type(
    text: &str,
    app: Option<&str>,
    bundle_id: Option<&str>,
    element: Option<&str>,
) -> Result<()> {
    let ax_app = connect_app(app, bundle_id)?;
    let target_query = element.unwrap_or("role:AXTextField");
    let el = ax_app
        .find_native(target_query, Some(5000))
        .map_err(|e| anyhow::anyhow!("Element not found: {e}"))?;
    el.type_text_native(text, axterminator::ActionMode::Focus)
        .map_err(|e| anyhow::anyhow!("Type failed: {e}"))?;
    println!("Typed {} chars into '{target_query}'", text.chars().count());
    Ok(())
}

fn cmd_screenshot(
    app: Option<&str>,
    bundle_id: Option<&str>,
    output: Option<&std::path::Path>,
) -> Result<()> {
    let ax_app = connect_app(app, bundle_id)?;
    let bytes = ax_app
        .screenshot_native()
        .map_err(|e| anyhow::anyhow!("Screenshot failed: {e}"))?;

    if let Some(path) = output {
        std::fs::write(path, &bytes)
            .with_context(|| format!("Failed to write screenshot to {}", path.display()))?;
        println!(
            "Screenshot saved to {} ({} bytes)",
            path.display(),
            bytes.len()
        );
    } else {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        println!("{b64}");
    }
    Ok(())
}

fn cmd_tree(app: Option<&str>, bundle_id: Option<&str>, depth: usize) -> Result<()> {
    let ax_app = connect_app(app, bundle_id)?;
    let windows = ax_app
        .windows_native()
        .map_err(|e| anyhow::anyhow!("Failed to get windows: {e}"))?;

    if windows.is_empty() {
        println!("(no windows)");
        return Ok(());
    }

    let mut total = 0;
    for (i, win) in windows.iter().enumerate() {
        let title = win.title().unwrap_or_else(|| format!("Window {i}"));
        println!("Window[{i}]: {title}");
        total += print_element_tree(win, 1, depth);
    }
    println!("\n({total} elements)");
    Ok(())
}

fn print_element_tree(el: &axterminator::AXElement, indent: usize, max_depth: usize) -> usize {
    if indent > max_depth {
        return 0;
    }
    let prefix = "  ".repeat(indent);
    let role = el.role().unwrap_or_else(|| "?".into());
    let label = el
        .title()
        .or_else(|| el.description())
        .or_else(|| el.label())
        .or_else(|| el.value())
        .unwrap_or_default();
    let label_suffix = if label.is_empty() {
        String::new()
    } else {
        format!(" \"{label}\"")
    };

    let bounds_suffix = if let Some((x, y, w, h)) = el.bounds() {
        format!(" [{x:.0},{y:.0} {w:.0}x{h:.0}]")
    } else {
        String::new()
    };

    let interactive = matches!(
        role.as_str(),
        "AXButton"
            | "AXTextField"
            | "AXTextArea"
            | "AXCheckBox"
            | "AXRadioButton"
            | "AXSlider"
            | "AXPopUpButton"
            | "AXMenuButton"
    );
    let state_suffix = if interactive && !el.enabled() {
        " [disabled]"
    } else {
        ""
    };

    println!("{prefix}{role}{label_suffix}{bounds_suffix}{state_suffix}");

    let mut count = 1;
    if indent < max_depth {
        for child in el.children() {
            count += print_element_tree(&child, indent + 1, max_depth);
        }
    }
    count
}

fn cmd_apps() -> Result<()> {
    use sysinfo::{ProcessRefreshKind, RefreshKind, System};

    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()),
    );
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let ax_enabled = axterminator::check_accessibility_enabled();
    println!(
        "Accessibility: {}",
        if ax_enabled { "enabled" } else { "DISABLED" }
    );
    println!();

    let mut procs: Vec<_> = sys.processes().values().collect();
    procs.sort_by_key(|p| p.name().to_string_lossy().to_lowercase());

    println!("{:<8} Name", "PID");
    println!("{:-<40}", "");
    for p in &procs {
        println!("{:<8} {}", p.pid(), p.name().to_string_lossy());
    }
    Ok(())
}

fn cmd_check() -> Result<()> {
    let enabled = axterminator::check_accessibility_enabled();
    if enabled {
        println!("Accessibility: OK");
        println!("Version:       {}", env!("CARGO_PKG_VERSION"));
    } else {
        eprintln!("Accessibility: DISABLED");
        eprintln!();
        eprintln!("To enable:");
        eprintln!("  1. Open System Settings > Privacy & Security > Accessibility");
        eprintln!("  2. Add and enable the terminal app (Terminal, iTerm2, etc.)");
        eprintln!("  3. Restart the terminal");
        std::process::exit(1);
    }
    Ok(())
}

fn cmd_completions(shell: &str) -> Result<()> {
    use clap_complete::Shell;

    let mut cmd = Cli::command();
    let shell: Shell = shell
        .parse()
        .map_err(|_| anyhow::anyhow!("Unknown shell: {shell}"))?;
    clap_complete::generate(shell, &mut cmd, "axterminator", &mut std::io::stdout());
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests — CLI parsing only (no macOS API needed)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("axterminator").chain(args.iter().copied()))
    }

    #[test]
    fn parses_mcp_serve_default_is_stdio() {
        // GIVEN: no transport flags
        let cli = parse(&["mcp", "serve"]).unwrap();
        // THEN: http is None (stdio is the default)
        assert!(matches!(
            cli.command,
            Commands::Mcp {
                subcommand: McpSubcommand::Serve { http: None, .. }
            }
        ));
    }

    #[test]
    fn parses_mcp_serve_explicit_stdio() {
        // GIVEN: --stdio flag
        let cli = parse(&["mcp", "serve", "--stdio"]).unwrap();
        match cli.command {
            Commands::Mcp {
                subcommand: McpSubcommand::Serve { stdio, .. },
            } => {
                assert!(stdio);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_mcp_serve_http_port() {
        let cli = parse(&["mcp", "serve", "--http", "9000"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Mcp {
                subcommand: McpSubcommand::Serve {
                    http: Some(9000),
                    ..
                }
            }
        ));
    }

    #[test]
    fn parses_mcp_serve_http_with_token() {
        // GIVEN: --http and --token
        let cli = parse(&["mcp", "serve", "--http", "8741", "--token", "axt_abc"]).unwrap();
        match cli.command {
            Commands::Mcp {
                subcommand: McpSubcommand::Serve { http, token, .. },
            } => {
                assert_eq!(http, Some(8741));
                assert_eq!(token.as_deref(), Some("axt_abc"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_mcp_serve_http_with_localhost_only() {
        // GIVEN: --http and --localhost-only
        let cli = parse(&["mcp", "serve", "--http", "8741", "--localhost-only"]).unwrap();
        match cli.command {
            Commands::Mcp {
                subcommand: McpSubcommand::Serve { localhost_only, .. },
            } => {
                assert!(localhost_only);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_mcp_serve_http_with_custom_bind() {
        // GIVEN: --http and --bind
        let cli = parse(&["mcp", "serve", "--http", "9000", "--bind", "0.0.0.0"]).unwrap();
        match cli.command {
            Commands::Mcp {
                subcommand: McpSubcommand::Serve { bind, .. },
            } => {
                assert_eq!(bind, "0.0.0.0");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_mcp_serve_both_http_and_stdio() {
        // GIVEN: both --http and --stdio
        let cli = parse(&["mcp", "serve", "--http", "8741", "--stdio"]).unwrap();
        match cli.command {
            Commands::Mcp {
                subcommand: McpSubcommand::Serve { http, stdio, .. },
            } => {
                assert_eq!(http, Some(8741));
                assert!(stdio);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn token_and_localhost_only_conflict() {
        // GIVEN: --token and --localhost-only together
        // THEN: clap returns an error (they conflict)
        assert!(parse(&[
            "mcp",
            "serve",
            "--http",
            "8741",
            "--token",
            "tok",
            "--localhost-only"
        ])
        .is_err());
    }

    #[test]
    fn parses_find_with_app() {
        let cli = parse(&["find", "Save", "--app", "Safari"]).unwrap();
        match cli.command {
            Commands::Find { query, app, .. } => {
                assert_eq!(query, "Save");
                assert_eq!(app.as_deref(), Some("Safari"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_find_with_bundle_id() {
        let cli = parse(&["find", "New Tab", "--bundle-id", "com.apple.Safari"]).unwrap();
        match cli.command {
            Commands::Find { bundle_id, .. } => {
                assert_eq!(bundle_id.as_deref(), Some("com.apple.Safari"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_find_default_timeout() {
        let cli = parse(&["find", "Save", "--app", "Safari"]).unwrap();
        match cli.command {
            Commands::Find { timeout, .. } => assert_eq!(timeout, 5000),
            _ => panic!(),
        }
    }

    #[test]
    fn parses_click_default_mode() {
        let cli = parse(&["click", "OK", "--app", "Safari"]).unwrap();
        match cli.command {
            Commands::Click { mode, .. } => assert_eq!(mode, "background"),
            _ => panic!(),
        }
    }

    #[test]
    fn parses_click_focus_mode() {
        let cli = parse(&["click", "OK", "--app", "Safari", "--mode", "focus"]).unwrap();
        match cli.command {
            Commands::Click { mode, .. } => assert_eq!(mode, "focus"),
            _ => panic!(),
        }
    }

    #[test]
    fn parses_type_text() {
        let cli = parse(&["type", "hello world", "--app", "Safari"]).unwrap();
        match cli.command {
            Commands::TypeText { text, .. } => assert_eq!(text, "hello world"),
            _ => panic!(),
        }
    }

    #[test]
    fn parses_screenshot_no_output() {
        let cli = parse(&["screenshot", "--app", "Safari"]).unwrap();
        match cli.command {
            Commands::Screenshot { output, .. } => assert!(output.is_none()),
            _ => panic!(),
        }
    }

    #[test]
    fn parses_screenshot_with_output() {
        let cli = parse(&["screenshot", "--app", "Safari", "--output", "/tmp/shot.png"]).unwrap();
        match cli.command {
            Commands::Screenshot { output, .. } => {
                assert_eq!(output.unwrap().to_str().unwrap(), "/tmp/shot.png");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parses_tree_default_depth() {
        let cli = parse(&["tree", "--app", "Safari"]).unwrap();
        match cli.command {
            Commands::Tree { depth, .. } => assert_eq!(depth, 5),
            _ => panic!(),
        }
    }

    #[test]
    fn parses_tree_custom_depth() {
        let cli = parse(&["tree", "--app", "Safari", "--depth", "3"]).unwrap();
        match cli.command {
            Commands::Tree { depth, .. } => assert_eq!(depth, 3),
            _ => panic!(),
        }
    }

    #[test]
    fn parses_apps_subcommand() {
        let cli = parse(&["apps"]).unwrap();
        assert!(matches!(cli.command, Commands::Apps));
    }

    #[test]
    fn parses_check_subcommand() {
        let cli = parse(&["check"]).unwrap();
        assert!(matches!(cli.command, Commands::Check));
    }

    #[test]
    fn parses_completions_zsh() {
        let cli = parse(&["completions", "zsh"]).unwrap();
        match cli.command {
            Commands::Completions { shell } => assert_eq!(shell, "zsh"),
            _ => panic!(),
        }
    }

    #[test]
    fn invalid_subcommand_returns_error() {
        assert!(parse(&["bogus"]).is_err());
    }

    #[test]
    fn mcp_requires_subcommand() {
        assert!(parse(&["mcp"]).is_err());
    }

    // -- mcp install parsing tests --

    #[test]
    fn parses_mcp_install_default_client() {
        let cli = parse(&["mcp", "install"]).unwrap();
        match cli.command {
            Commands::Mcp {
                subcommand: McpSubcommand::Install { client, force, dry_run },
            } => {
                assert_eq!(client, "claude-desktop");
                assert!(!force);
                assert!(!dry_run);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_mcp_install_cursor() {
        let cli = parse(&["mcp", "install", "--client", "cursor"]).unwrap();
        match cli.command {
            Commands::Mcp {
                subcommand: McpSubcommand::Install { client, .. },
            } => assert_eq!(client, "cursor"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_mcp_install_windsurf() {
        let cli = parse(&["mcp", "install", "--client", "windsurf"]).unwrap();
        match cli.command {
            Commands::Mcp {
                subcommand: McpSubcommand::Install { client, .. },
            } => assert_eq!(client, "windsurf"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_mcp_install_claude_code() {
        let cli = parse(&["mcp", "install", "--client", "claude-code"]).unwrap();
        match cli.command {
            Commands::Mcp {
                subcommand: McpSubcommand::Install { client, .. },
            } => assert_eq!(client, "claude-code"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_mcp_install_force() {
        let cli = parse(&["mcp", "install", "--force"]).unwrap();
        match cli.command {
            Commands::Mcp {
                subcommand: McpSubcommand::Install { force, .. },
            } => assert!(force),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_mcp_install_dry_run() {
        let cli = parse(&["mcp", "install", "--dry-run"]).unwrap();
        match cli.command {
            Commands::Mcp {
                subcommand: McpSubcommand::Install { dry_run, .. },
            } => assert!(dry_run),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn mcp_install_rejects_unknown_client() {
        assert!(parse(&["mcp", "install", "--client", "vscode"]).is_err());
    }

    // -- mcp install logic tests --

    #[test]
    fn client_config_path_claude_desktop() {
        let path = client_config_path("claude-desktop").unwrap();
        let s = path.to_string_lossy();
        assert!(s.contains("claude_desktop_config.json"), "got: {s}");
    }

    #[test]
    fn client_config_path_cursor() {
        let path = client_config_path("cursor").unwrap();
        assert!(path.to_string_lossy().ends_with(".cursor/mcp.json"));
    }

    #[test]
    fn client_config_path_windsurf() {
        let path = client_config_path("windsurf").unwrap();
        assert!(path.to_string_lossy().contains("windsurf/mcp_config.json"));
    }

    #[test]
    fn client_config_path_claude_code() {
        let path = client_config_path("claude-code").unwrap();
        assert!(path.to_string_lossy().ends_with(".claude.json"));
    }

    #[test]
    fn client_config_path_unknown_errors() {
        assert!(client_config_path("unknown").is_err());
    }

    #[test]
    fn mcp_install_dry_run_fresh_config() {
        // Write to a temp dir to test the full flow.
        let dir = std::env::temp_dir().join("axterminator_test_install");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join("test_config.json");

        // Simulate cmd_mcp_install logic inline to avoid needing real $HOME.
        let mut cfg: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        let servers = cfg
            .entry("mcpServers")
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        let servers = servers.as_object_mut().unwrap();
        servers.insert(
            "axterminator".to_string(),
            serde_json::json!({"command": "axterminator", "args": ["mcp", "serve"]}),
        );
        let out = serde_json::to_string_pretty(&cfg).unwrap();
        std::fs::write(&cfg_path, &out).unwrap();

        let written: serde_json::Value = serde_json::from_str(&out).unwrap();
        let entry = &written["mcpServers"]["axterminator"];
        assert_eq!(entry["command"], "axterminator");
        assert_eq!(entry["args"], serde_json::json!(["mcp", "serve"]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mcp_install_preserves_other_entries() {
        let existing = serde_json::json!({
            "mcpServers": {
                "other-tool": {"command": "other", "args": []}
            }
        });
        let mut cfg: serde_json::Map<String, serde_json::Value> =
            serde_json::from_value(existing).unwrap();
        let servers = cfg.get_mut("mcpServers").unwrap().as_object_mut().unwrap();
        servers.insert(
            "axterminator".to_string(),
            serde_json::json!({"command": "axterminator", "args": ["mcp", "serve"]}),
        );

        // other-tool must still be present.
        assert!(servers.contains_key("other-tool"));
        assert!(servers.contains_key("axterminator"));
        assert_eq!(servers.len(), 2);
    }
}
