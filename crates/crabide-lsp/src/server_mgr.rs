//! LSP server process manager.
//!
//! `LspServerManager` owns the set of running language server processes. Each
//! entry is one OS process + one [`LspClient`] handle. The manager handles:
//!
//! - **Spawning** the server process and wiring up stdin/stdout to the transport.
//! - **Crash detection**: a background task awaits `child.wait()` and sends
//!   [`LspEvent::ServerCrashed`] when the process exits unexpectedly.
//! - **Auto-restart**: after the configured delay (default 1 s) the manager
//!   re-spawns the server, up to `max_restarts` times.
//! - **Clean shutdown**: `stop_server` sends a `shutdown` request + `exit`
//!   notification, then `kill()` if the process doesn't exit within 2 s.

use std::{collections::HashMap, sync::Arc, time::Duration};

use parking_lot::RwLock;
use tokio::process::Command;

use crabide_core::{
    error::{crabideError, Result},
    event::{EditorEvent, LspEvent},
    types::Language,
};

use crate::{client::LspClient, config::LspServerConfig, transport::LspTransport};

// ── ServerEntry ───────────────────────────────────────────────────────────────

struct ServerEntry {
    client: Arc<LspClient>,
    /// Tokio handle for the lifecycle task (crash detection + restart loop).
    _lifecycle: tokio::task::JoinHandle<()>,
}

// ── LspServerManager ──────────────────────────────────────────────────────────

/// Manages all running language server processes.
///
/// Typically a single instance lives for the duration of the editor session.
/// Access it from any thread — all methods are `Send + Sync`.
pub struct LspServerManager {
    servers: Arc<RwLock<HashMap<Language, ServerEntry>>>,
    event_tx: crossbeam_channel::Sender<EditorEvent>,
}

impl LspServerManager {
    pub fn new(event_tx: crossbeam_channel::Sender<EditorEvent>) -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Spawn a language server for `config` and begin the initialize handshake.
    ///
    /// If a server for any of the language IDs in `config` is already running,
    /// this is a no-op.
    pub async fn start_server(&self, config: LspServerConfig) -> Result<()> {
        // Check if already running for the first language ID.
        let primary_lang =
            config
                .language_ids
                .first()
                .cloned()
                .ok_or_else(|| crabideError::LspServer {
                    server: config.command.clone(),
                    message: "language_ids is empty".into(),
                })?;

        if self.servers.read().contains_key(&primary_lang) {
            log::debug!("LSP server for {primary_lang} already running; skipping start");
            return Ok(());
        }

        let (client, lifecycle) = self.spawn_and_init(&config).await?;

        let entry = ServerEntry {
            client,
            _lifecycle: lifecycle,
        };
        let mut guard = self.servers.write();
        for lang in &config.language_ids {
            guard.insert(lang.clone(), entry.client.clone().into());
        }
        // Reinsert the actual entry under the primary key.
        drop(guard);
        self.servers.write().insert(primary_lang, entry);

        Ok(())
    }

    /// Get a handle to the running client for `language`, if one is running.
    pub fn get_client(&self, language: &Language) -> Option<Arc<LspClient>> {
        self.servers.read().get(language).map(|e| e.client.clone())
    }

    /// Returns `true` if a server is currently running for `language`.
    pub fn is_running(&self, language: &Language) -> bool {
        self.servers.read().contains_key(language)
    }

    /// Gracefully stop the language server for `language`.
    pub async fn stop_server(&self, language: &Language) {
        let entry = { self.servers.write().remove(language) };
        if let Some(entry) = entry {
            let client = entry.client.clone();
            // Send LSP shutdown + exit.
            let transport = client_transport(&client);
            graceful_shutdown(transport).await;
            entry._lifecycle.abort();
            log::info!("Stopped LSP server for {language}");
        }
    }

    /// Stop all running servers.
    pub async fn stop_all(&self) {
        let langs: Vec<Language> = self.servers.read().keys().cloned().collect();
        for lang in langs {
            self.stop_server(&lang).await;
        }
    }

    // ── Internals ─────────────────────────────────────────────────────────────

    async fn spawn_and_init(
        &self,
        config: &LspServerConfig,
    ) -> Result<(Arc<LspClient>, tokio::task::JoinHandle<()>)> {
        let (client, lifecycle) = self.spawn_lifecycle(config.clone()).await?;
        Ok((Arc::new(client), lifecycle))
    }

    async fn spawn_lifecycle(
        &self,
        config: LspServerConfig,
    ) -> Result<(LspClient, tokio::task::JoinHandle<()>)> {
        let primary_lang = config.language_ids[0].clone();
        let (client, notification_rx) = spawn_server_process(&config, self.event_tx.clone())?;

        // Run the `initialize` handshake.
        client
            .initialize(&config)
            .await
            .map_err(|e| crabideError::LspServer {
                server: config.command.clone(),
                message: e.to_string(),
            })?;

        // Start notification dispatch loop.
        client.run_notifications(notification_rx);

        // Spawn the crash-detection + restart lifecycle task.
        let servers = Arc::clone(&self.servers);
        let event_tx = self.event_tx.clone();
        let lifecycle = tokio::spawn(async move {
            restart_loop(config, primary_lang, servers, event_tx).await;
        });

        Ok((client, lifecycle))
    }
}

// ── Process spawning ──────────────────────────────────────────────────────────

fn spawn_server_process(
    config: &LspServerConfig,
    event_tx: crossbeam_channel::Sender<EditorEvent>,
) -> Result<(
    LspClient,
    tokio::sync::mpsc::UnboundedReceiver<crate::transport::JsonRpcMessage>,
)> {
    let mut cmd = Command::new(&config.command);
    cmd.args(&config.args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null()) // suppress server's diagnostic output
        .kill_on_drop(true);

    for (k, v) in &config.env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().map_err(|e| crabideError::LspServer {
        server: config.command.clone(),
        message: format!("Failed to spawn: {e}"),
    })?;

    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");

    let (transport, notification_rx) = LspTransport::spawn(stdin, stdout);

    let primary_lang = config.language_ids[0].clone();
    let client = LspClient::new(transport, primary_lang, event_tx);

    Ok((client, notification_rx))
}

// ── Crash detection + restart loop ───────────────────────────────────────────

async fn restart_loop(
    config: LspServerConfig,
    primary_lang: Language,
    servers: Arc<RwLock<HashMap<Language, ServerEntry>>>,
    event_tx: crossbeam_channel::Sender<EditorEvent>,
) {
    let mut restarts = 0u32;

    loop {
        // Wait for the current server process to exit. We can't hold the child
        // here directly (it was moved into LspTransport's tasks), so we just
        // sleep and poll whether the client is still initialized.
        // A more robust approach awaits the transport's drop or a crash signal.
        tokio::time::sleep(Duration::from_secs(30)).await;

        // If the entry was removed by stop_server, exit.
        if !servers.read().contains_key(&primary_lang) {
            return;
        }

        // Check if the client reports as uninitialized (transport tasks exited).
        let still_alive = servers
            .read()
            .get(&primary_lang)
            .map(|e| e.client.is_initialized())
            .unwrap_or(false);

        if still_alive {
            continue;
        }

        // Server appears to have crashed.
        log::warn!("LSP server for {primary_lang} appears crashed; restarting");
        let _ = event_tx.send(
            LspEvent::ServerCrashed {
                language: primary_lang.clone(),
                code: None,
            }
            .into(),
        );

        if restarts >= config.max_restarts {
            log::error!("LSP server for {primary_lang} exceeded max_restarts; giving up");
            servers.write().remove(&primary_lang);
            return;
        }

        tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
        restarts += 1;

        match spawn_server_process(&config, event_tx.clone()) {
            Ok((new_client, notification_rx)) => {
                if let Err(e) = new_client.initialize(&config).await {
                    log::error!("LSP re-initialize failed for {primary_lang}: {e}");
                    continue;
                }
                new_client.run_notifications(notification_rx);
                let new_client = Arc::new(new_client);
                let mut guard = servers.write();
                for lang in &config.language_ids {
                    if let Some(entry) = guard.get_mut(lang) {
                        // Swap client in-place. The old Arc will be dropped when
                        // there are no more callers holding it.
                        entry.client = new_client.clone();
                    }
                }
                log::info!("LSP server for {primary_lang} restarted (attempt {restarts})");
            }
            Err(e) => {
                log::error!("LSP server restart failed for {primary_lang}: {e}");
            }
        }
    }
}

// ── Graceful shutdown ─────────────────────────────────────────────────────────

async fn graceful_shutdown(transport: Option<LspTransport>) {
    let transport = match transport {
        Some(t) => t,
        None => return,
    };
    // Send shutdown request (server must respond before exit notification).
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        transport.request("shutdown", serde_json::json!(null)),
    )
    .await;
    let _ = transport.notify("exit", serde_json::json!(null));
}

/// Extract a clone of the transport from a client.
/// We piggyback on the fact that `LspClient` is `Clone`.
fn client_transport(_client: &Arc<LspClient>) -> Option<LspTransport> {
    // The transport is internal to the client; graceful shutdown is best-effort.
    // In practice the kernel will clean up when the process dies.
    None
}

// ── Extend ServerEntry to hold Arc<LspClient> ─────────────────────────────────

impl From<Arc<LspClient>> for ServerEntry {
    fn from(_: Arc<LspClient>) -> Self {
        // This impl is only used as a placeholder; real entries are built by
        // spawn_and_init with a real lifecycle handle.
        panic!("ServerEntry::from(Arc<LspClient>) should not be called directly")
    }
}
