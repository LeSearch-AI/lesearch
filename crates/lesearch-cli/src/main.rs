//! `lesearch` — command-line client for the `LeSearch` daemon.
//!
//! Connects to the daemon via `WebSocket` and sends JSON-RPC requests.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod ws_client;

/// The open control plane for your CLI agents.
#[derive(Parser)]
#[command(name = "lesearch", version, about)]
struct Cli {
    /// Daemon address (default: 127.0.0.1:6767).
    #[arg(long, default_value = lesearch_protocol::DEFAULT_BIND)]
    daemon: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Spawn a new agent.
    Run {
        /// Provider to use (e.g., "claude", "codex").
        #[arg(long, default_value = "claude")]
        provider: String,
        /// Model identifier (provider-specific).
        #[arg(long)]
        model: Option<String>,
        /// Working directory for the agent.
        #[arg(long)]
        cwd: Option<String>,
        /// The prompt to send to the agent.
        prompt: String,
    },
    /// List running and recent agents.
    Ls,
    /// Attach to a running agent and stream output.
    Attach {
        /// Agent id.
        id: String,
    },
    /// Send text input to a running agent.
    Send {
        /// Agent id.
        id: String,
        /// Text to send.
        text: String,
    },
    /// Stop a running agent.
    Stop {
        /// Agent id.
        id: String,
    },
    /// Session management.
    Sessions {
        #[command(subcommand)]
        action: SessionAction,
    },
    /// Check daemon health.
    Doctor,
    /// Start the daemon (foreground).
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
}

#[derive(Subcommand)]
enum SessionAction {
    /// Search across all sessions.
    Search {
        /// Search query (substring match).
        #[arg(long)]
        grep: String,
        /// Max results.
        #[arg(long, default_value = "50")]
        limit: u32,
    },
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon.
    Start,
    /// Check daemon status.
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            provider,
            model,
            cwd,
            prompt,
        } => cmd_run(&cli.daemon, &provider, model, cwd, &prompt).await,
        Commands::Ls => cmd_ls(&cli.daemon).await,
        Commands::Attach { id } => cmd_attach(&cli.daemon, &id).await,
        Commands::Send { id, text } => cmd_send(&cli.daemon, &id, &text).await,
        Commands::Stop { id } => cmd_stop(&cli.daemon, &id).await,
        Commands::Sessions { action } => match action {
            SessionAction::Search { grep, limit } => {
                cmd_session_search(&cli.daemon, &grep, limit).await
            }
        },
        Commands::Doctor => cmd_doctor(&cli.daemon).await,
        Commands::Daemon { action } => match action {
            DaemonAction::Start => {
                println!("Use `lesearch-daemon` binary to start the daemon directly.");
                println!("Or run: lesearch-daemon (from the daemon crate)");
                Ok(())
            }
            DaemonAction::Status => cmd_doctor(&cli.daemon).await,
        },
    }
}

async fn cmd_run(
    daemon: &str,
    provider: &str,
    model: Option<String>,
    cwd: Option<String>,
    prompt: &str,
) -> Result<()> {
    let mut client = ws_client::WsClient::connect(daemon).await?;

    let params = serde_json::json!({
        "provider": provider,
        "prompt": prompt,
        "model": model,
        "cwd": cwd,
    });

    let result = client
        .call(lesearch_protocol::jsonrpc::method::AGENT_SPAWN, params)
        .await?;

    let agent_id = result
        .get("agent_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");

    println!("Agent spawned: {agent_id}");
    println!("Streaming output (Ctrl+C to detach)...\n");

    // Stream events until disconnect
    client.stream_until_close().await?;

    Ok(())
}

async fn cmd_ls(daemon: &str) -> Result<()> {
    let mut client = ws_client::WsClient::connect(daemon).await?;

    let agents = client
        .call(
            lesearch_protocol::jsonrpc::method::AGENT_LIST,
            serde_json::json!({}),
        )
        .await?;

    let agents = agents.as_array();
    let count = agents.map_or(0, Vec::len);

    if count == 0 {
        println!("No agents.");
        return Ok(());
    }

    println!("{:<38} {:<8} {:<10} CWD", "ID", "PROVIDER", "STATUS");
    for a in agents.unwrap_or(&vec![]) {
        println!(
            "{:<38} {:<8} {:<10} {}",
            a.get("id").and_then(serde_json::Value::as_str).unwrap_or("?"),
            a.get("provider").and_then(serde_json::Value::as_str).unwrap_or("?"),
            a.get("status").and_then(serde_json::Value::as_str).unwrap_or("?"),
            a.get("cwd").and_then(serde_json::Value::as_str).unwrap_or("?"),
        );
    }
    println!("\n{count} agent(s)");

    Ok(())
}

async fn cmd_attach(daemon: &str, id: &str) -> Result<()> {
    println!("Attaching to agent {id}...");
    println!("(Ctrl+C to detach)\n");

    let mut client = ws_client::WsClient::connect(daemon).await?;

    // Verify agent exists via handshake first
    client
        .call(
            lesearch_protocol::jsonrpc::method::HANDSHAKE,
            serde_json::json!({
                "client_name": "lesearch-cli",
                "client_version": env!("CARGO_PKG_VERSION"),
            }),
        )
        .await?;

    // Stream events until disconnect
    client.stream_until_close().await?;

    Ok(())
}

async fn cmd_send(daemon: &str, id: &str, text: &str) -> Result<()> {
    let mut client = ws_client::WsClient::connect(daemon).await?;

    let params = serde_json::json!({
        "agent_id": id,
        "text": text,
    });

    client
        .call(lesearch_protocol::jsonrpc::method::AGENT_INPUT, params)
        .await?;

    println!("Input sent to agent {id}");
    Ok(())
}

async fn cmd_stop(daemon: &str, id: &str) -> Result<()> {
    let mut client = ws_client::WsClient::connect(daemon).await?;

    let params = serde_json::json!({ "agent_id": id });

    client
        .call(lesearch_protocol::jsonrpc::method::AGENT_STOP, params)
        .await?;

    println!("Agent {id} stopped");
    Ok(())
}

async fn cmd_session_search(daemon: &str, query: &str, limit: u32) -> Result<()> {
    let mut client = ws_client::WsClient::connect(daemon).await?;

    let params = serde_json::json!({
        "query": query,
        "limit": limit,
    });

    let result = client
        .call(
            lesearch_protocol::jsonrpc::method::SESSION_SEARCH,
            params,
        )
        .await?;

    let hits = result
        .get("hits")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);

    println!("Found {hits} results for \"{query}\":");

    if let Some(arr) = result.get("hits").and_then(serde_json::Value::as_array) {
        for hit in arr {
            println!(
                "  [{ts}] {et} — {sid}",
                ts = hit.get("timestamp").and_then(serde_json::Value::as_str).unwrap_or("?"),
                et = hit.get("event_type").and_then(serde_json::Value::as_str).unwrap_or("?"),
                sid = hit.get("session_id").and_then(serde_json::Value::as_str).unwrap_or("?"),
            );
        }
    }

    Ok(())
}

async fn cmd_doctor(daemon: &str) -> Result<()> {
    println!("lesearch doctor — checking daemon at {daemon}");

    match ws_client::WsClient::connect(daemon).await {
        Ok(mut client) => {
            let result = client
                .call(
                    lesearch_protocol::jsonrpc::method::HANDSHAKE,
                    serde_json::json!({
                        "client_name": "lesearch-cli",
                        "client_version": env!("CARGO_PKG_VERSION"),
                    }),
                )
                .await
                .context("handshake failed")?;

            println!("  Daemon: OK");
            println!(
                "  Protocol: {}",
                result
                    .get("protocol_version")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("?")
            );
            println!(
                "  Version:  {}",
                result
                    .get("daemon_version")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("?")
            );
        }
        Err(e) => {
            println!("  Daemon: UNREACHABLE ({e})");
            println!("  Hint: start with `lesearch-daemon`");
        }
    }

    Ok(())
}
