//! DAP Content-Length framing transport.
//!
//! The Debug Adapter Protocol uses the same `Content-Length` header framing as
//! LSP, but the message envelope is different (`seq`, `type`, `command`/`event`
//! rather than `jsonrpc`, `method`).
//!
//! Architecture: identical to `crabide_lsp::transport` — a writer Tokio task
//! and a reader Tokio task, with a DashMap for pending requests.

use anyhow::{anyhow, Context, Result};
use dashmap::DashMap;
use serde_json::Value;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, ChildStdout},
    sync::{mpsc, oneshot},
};

use crate::types::DapMessage;

// ── Pending request registry ──────────────────────────────────────────────────

struct PendingRequest {
    respond: oneshot::Sender<Result<Option<Value>>>,
}

// ── DapTransport ─────────────────────────────────────────────────────────────

/// Clone-friendly handle to the running DAP transport.
///
/// Internally Arc-wrapped; cloning is cheap.  The transport runs two background
/// Tokio tasks: a writer and a reader.  The writer uses a bounded channel (1024
/// slots) with backpressure — if the adapter is slow to consume, the client will
/// block rather than unboundedly buffering.
#[derive(Clone)]
pub struct DapTransport {
    inner: Arc<TransportInner>,
}

struct TransportInner {
    tx: mpsc::Sender<Vec<u8>>,
    pending: Arc<DashMap<u32, PendingRequest>>,
    seq: AtomicU32,
}

impl DapTransport {
    /// Spawn the transport on an already-started debug adapter process.
    ///
    /// Returns the transport handle and a receiver that delivers incoming
    /// adapter → client messages (events and reverse-requests).
    pub fn spawn(
        stdin: ChildStdin,
        stdout: ChildStdout,
    ) -> (Self, mpsc::UnboundedReceiver<DapMessage>) {
        // Bounded channel with backpressure: 1024 messages max buffered.
        let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>(1024);
        let (in_tx, in_rx) = mpsc::unbounded_channel::<DapMessage>();

        let inner = Arc::new(TransportInner {
            tx: out_tx,
            pending: Arc::new(DashMap::new()),
            seq: AtomicU32::new(1),
        });

        // Writer task.
        tokio::spawn(run_writer(stdin, out_rx));

        // Reader task.
        let reader_pending = Arc::clone(&inner.pending);
        tokio::spawn(run_reader(stdout, reader_pending, in_tx));

        (Self { inner }, in_rx)
    }

    /// Send a DAP request and await the response body.
    ///
    /// Returns `Ok(Some(body))` on success, `Ok(None)` if the response had no
    /// body, or `Err` if the adapter returned an error or the transport closed.
    pub async fn request(&self, command: &str, arguments: Value) -> Result<Option<Value>> {
        self.request_with_timeout(command, arguments, None).await
    }

    /// Send a DAP request with an optional timeout.
    pub async fn request_with_timeout(
        &self,
        command: &str,
        arguments: Value,
        timeout: Option<std::time::Duration>,
    ) -> Result<Option<Value>> {
        let seq = self.inner.seq.fetch_add(1, Ordering::Relaxed);
        let msg = DapMessage::request(seq, command, arguments);

        let (tx, rx) = oneshot::channel();
        self.inner
            .pending
            .insert(seq, PendingRequest { respond: tx });

        self.send_raw(&msg)?;

        let result = if let Some(dur) = timeout {
            tokio::time::timeout(dur, rx)
                .await
                .map_err(|_| anyhow!("DAP request {command} timed out after {:?}", dur))?
                .map_err(|_| anyhow!("DAP transport closed while awaiting response to {command}"))?
        } else {
            rx.await
                .map_err(|_| anyhow!("DAP transport closed while awaiting response to {command}"))?
        };

        self.inner.pending.remove(&seq);
        result
    }

    /// Send a DAP notification (fire-and-forget; no response expected).
    pub fn notify(&self, command: &str, arguments: Value) -> Result<()> {
        let seq = self.inner.seq.fetch_add(1, Ordering::Relaxed);
        let msg = DapMessage::request(seq, command, arguments);
        self.send_raw(&msg)
    }

    fn send_raw(&self, msg: &DapMessage) -> Result<()> {
        let json = serde_json::to_vec(msg).context("Serialising DAP message")?;
        let header = format!("Content-Length: {}\r\n\r\n", json.len());
        let mut frame = Vec::with_capacity(header.len() + json.len());
        frame.extend_from_slice(header.as_bytes());
        frame.extend_from_slice(&json);
        self.inner
            .tx
            .try_send(frame)
            .map_err(|_| anyhow!("DAP writer channel full — adapter too slow"))
    }

    /// Send an arbitrary response to the adapter (used for reverse-request replies).
    pub fn send_response(&self, msg: DapMessage) -> Result<()> {
        self.send_raw(&msg)
    }
}

// ── Writer task ───────────────────────────────────────────────────────────────

async fn run_writer(mut stdin: ChildStdin, mut rx: mpsc::Receiver<Vec<u8>>) {
    while let Some(frame) = rx.recv().await {
        if let Err(e) = stdin.write_all(&frame).await {
            log::error!("DAP writer: failed to write: {e}");
            break;
        }
    }
    log::debug!("DAP writer task exited");
}

// ── Reader task ───────────────────────────────────────────────────────────────

async fn run_reader(
    stdout: ChildStdout,
    pending: Arc<DashMap<u32, PendingRequest>>,
    in_tx: mpsc::UnboundedSender<DapMessage>,
) {
    let mut reader = BufReader::new(stdout);

    loop {
        // ── 1. Read headers until blank line ──────────────────────────────────
        let mut content_length: Option<usize> = None;
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    log::debug!("DAP reader: EOF");
                    return;
                }
                Err(e) => {
                    log::error!("DAP reader: header read error: {e}");
                    return;
                }
                Ok(_) => {}
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
                if let Ok(n) = rest.trim().parse::<usize>() {
                    content_length = Some(n);
                }
            }
        }

        let length = match content_length {
            Some(n) => n,
            None => {
                log::warn!("DAP reader: no Content-Length in header block");
                continue;
            }
        };

        // ── 2. Read body ──────────────────────────────────────────────────────
        let mut body = vec![0u8; length];
        if let Err(e) = reader.read_exact(&mut body).await {
            log::error!("DAP reader: body read error: {e}");
            return;
        }

        // ── 3. Parse ──────────────────────────────────────────────────────────
        let msg: DapMessage = match serde_json::from_slice(&body) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("DAP reader: malformed message: {e}");
                continue;
            }
        };

        // ── 4. Dispatch ───────────────────────────────────────────────────────
        if msg.is_response() {
            let req_seq = match msg.request_seq {
                Some(s) => s,
                None => {
                    log::warn!("DAP reader: response missing request_seq");
                    continue;
                }
            };
            match pending.remove(&req_seq) { Some((_, req)) => {
                let outcome = if msg.success.unwrap_or(false) {
                    Ok(msg.body)
                } else {
                    Err(anyhow!(
                        "DAP error (cmd {:?}): {}",
                        msg.command,
                        msg.message.as_deref().unwrap_or("unknown error")
                    ))
                };
                let _ = req.respond.send(outcome);
            } _ => {
                log::warn!("DAP reader: response for unknown seq {req_seq}");
            }}
        } else {
            // Event or reverse-request — forward to the client.
            if in_tx.send(msg).is_err() {
                log::debug!("DAP reader: in-channel closed; exiting");
                return;
            }
        }
    }
}
