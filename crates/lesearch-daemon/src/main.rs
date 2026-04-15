//! `LeSearch` daemon entry point.
//!
//! Loads configuration, initializes storage, registers providers,
//! and starts the axum server with `WebSocket` + A2A endpoints.

use std::collections::HashMap;
use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use tokio::sync::{mpsc, Mutex};
use tower_http::trace::TraceLayer;

use lesearch_daemon::agent_manager::AgentManager;
use lesearch_daemon::session_writer::spawn_session_writer;
use lesearch_daemon::{DaemonState, SharedState};
use lesearch_providers::claude::ClaudeProvider;
use lesearch_providers::codex::CodexProvider;
use lesearch_providers::AgentProvider;
use lesearch_storage::config::LesearchConfig;
use lesearch_storage::keyring::Keyring;
use lesearch_storage::registry::Registry;
use lesearch_storage::search::SearchIndex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Load config
    let config = LesearchConfig::load_default()?;
    let home = config.resolve_home();
    tracing::info!(home = %home.display(), "starting lesearch daemon");

    // Initialize storage
    std::fs::create_dir_all(&home)?;

    let keys_dir = home.join(".keys");
    let keyring = Keyring::load_or_generate(&keys_dir)?;
    tracing::info!("keyring loaded");

    let registry = Registry::open(&home.join("registry.sqlite"))?;
    let search_index = SearchIndex::open(&home.join("search.sqlite"))?;

    // Register providers
    let mut providers: HashMap<String, Box<dyn AgentProvider>> = HashMap::new();
    providers.insert("claude".into(), Box::new(ClaudeProvider::new()));
    providers.insert("codex".into(), Box::new(CodexProvider::new()));
    tracing::info!(count = providers.len(), "providers registered");

    // Session writer channel
    let (event_tx, event_rx) = mpsc::channel(1024);
    let search_index = Arc::new(Mutex::new(search_index));

    // Spawn session writer background task
    let sessions_dir = home.join("sessions");
    std::fs::create_dir_all(&sessions_dir)?;
    spawn_session_writer(event_rx, sessions_dir, Keyring::generate(), search_index.clone());

    // Load or generate bearer token for WS auth
    let bearer_token = lesearch_daemon::auth::load_or_generate_token(&home)?;

    // Build shared state
    let state: SharedState = Arc::new(DaemonState {
        config,
        keyring,
        registry: Mutex::new(registry),
        search_index,
        providers,
        agent_manager: Mutex::new(AgentManager::new(event_tx, home)),
        bearer_token,
    });

    // Build router
    let app = Router::new()
        .route("/ws", get(lesearch_daemon::auth::authenticated_ws_handler))
        .route(
            "/.well-known/agent.json",
            get(lesearch_daemon::a2a::agent_card_handler),
        )
        .route("/health", get(lesearch_daemon::a2a::health_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Bind and serve
    let bind = lesearch_protocol::DEFAULT_BIND;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "daemon listening");

    axum::serve(listener, app).await?;

    Ok(())
}
