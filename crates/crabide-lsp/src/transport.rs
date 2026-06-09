//! Custom JSON-RPC transport for the LSP client.
//!
//! This replaces `async-lsp` (which pulls in `tower`, `tower-service`,
//! `tower-layer`, `futures`, and `pin-project`). The LSP transport protocol
//! is simple: Content-Length framed JSON over stdin/stdout. That's ~300 lines
//! of Tokio async Rust — no middleware chain required for a client-only use.
//!
//! # Protocol
//!
//! ```text
//! Content-Length: <byte_length>\r\n
//! \r\n
//! {"jsonrpc":"2.0","id":1,"method":"initialize","params":{...}}
//! ```
//!
//! The framing is the same in both directions (client→server and server→client).

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
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

// ── Wire types ────────────────────────────────────────────────────────────────

/// A raw JSON-RPC 2.0 message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcMessage {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcMessage {
    /// Construct a request message.
    pub fn request(id: u32, method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(id.into())),
            method: Some(method.into()),
            params: Some(params),
            result: None,
            error: None,
        }
    }

    /// Construct a notification (no id, no response expected).
    pub fn notification(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: None,
            method: Some(method.into()),
            params: Some(params),
            result: None,
            error: None,
        }
    }

    pub fn is_request(&self) -> bool {
        self.id.is_some() && self.method.is_some()
    }
    pub fn is_notification(&self) -> bool {
        self.id.is_none() && self.method.is_some()
    }
    pub fn is_response(&self) -> bool {
        self.id.is_some() && self.method.is_none()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── JsonRpcMessage ─────────────────────────────────────────────────────

    #[test]
    fn message_request_construction() {
        let msg = JsonRpcMessage::request(1, "initialize", json!({"rootUri": null}));
        assert_eq!(msg.jsonrpc, "2.0");
        assert_eq!(msg.id, Some(Value::Number(1.into())));
        assert_eq!(msg.method.as_deref(), Some("initialize"));
        assert!(msg.params.is_some());
        assert!(msg.result.is_none());
        assert!(msg.error.is_none());
    }

    #[test]
    fn message_notification_construction() {
        let msg = JsonRpcMessage::notification("textDocument/didOpen", json!({}));
        assert_eq!(msg.jsonrpc, "2.0");
        assert!(msg.id.is_none());
        assert_eq!(msg.method.as_deref(), Some("textDocument/didOpen"));
        assert!(msg.params.is_some());
        assert!(msg.result.is_none());
        assert!(msg.error.is_none());
    }

    #[test]
    fn message_is_request() {
        let msg = JsonRpcMessage::request(1, "method", json!({}));
        assert!(msg.is_request());
        assert!(!msg.is_notification());
        assert!(!msg.is_response());
    }

    #[test]
    fn message_is_notification() {
        let msg = JsonRpcMessage::notification("method", json!({}));
        assert!(!msg.is_request());
        assert!(msg.is_notification());
        assert!(!msg.is_response());
    }

    #[test]
    fn message_is_response() {
        let msg = JsonRpcMessage {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: None,
            params: None,
            result: Some(Value::Null),
            error: None,
        };
        assert!(!msg.is_request());
        assert!(!msg.is_notification());
        assert!(msg.is_response());
    }

    #[test]
    fn message_request_serialization() {
        let msg = JsonRpcMessage::request(1, "test/method", json!({"key": "value"}));
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"method\":\"test/method\""));
        assert!(json.contains("\"params\":{\"key\":\"value\"}"));
        // result and error should be absent
        assert!(!json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn message_request_deserialization() {
        let json_str =
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null}}"#;
        let msg: JsonRpcMessage = serde_json::from_str(json_str).unwrap();
        assert_eq!(msg.jsonrpc, "2.0");
        assert_eq!(msg.id, Some(Value::Number(1.into())));
        assert_eq!(msg.method.as_deref(), Some("initialize"));
        assert!(msg.is_request());
    }

    #[test]
    fn message_response_deserialization() {
        let json_str = r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#;
        let msg: JsonRpcMessage = serde_json::from_str(json_str).unwrap();
        assert!(msg.is_response());
        assert_eq!(msg.id, Some(Value::Number(1.into())));
        assert!(msg.method.is_none());
        assert!(msg.result.is_some());
    }

    #[test]
    fn message_notification_deserialization() {
        let json_str = r#"{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":"file:///test.rs","diagnostics":[]}}"#;
        let msg: JsonRpcMessage = serde_json::from_str(json_str).unwrap();
        assert!(msg.is_notification());
        assert!(msg.id.is_none());
        assert_eq!(
            msg.method.as_deref(),
            Some("textDocument/publishDiagnostics")
        );
    }

    #[test]
    fn message_error_response() {
        let json_str =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let msg: JsonRpcMessage = serde_json::from_str(json_str).unwrap();
        assert!(msg.is_response());
        let err = msg.error.as_ref().unwrap();
        assert_eq!(err.code, -32601);
        assert_eq!(err.message, "Method not found");
        assert!(err.data.is_none());
    }

    #[test]
    fn message_with_null_params_is_notification() {
        // Some LSP servers send notifications with null params
        let json_str = r#"{"jsonrpc":"2.0","method":"exit","params":null}"#;
        let msg: JsonRpcMessage = serde_json::from_str(json_str).unwrap();
        assert!(msg.is_notification());
    }
}

// ── Transport ─────────────────────────────────────────────────────────────────

/// Tracks an in-flight request, waiting for a response.
struct PendingRequest {
    respond: oneshot::Sender<Result<Value>>,
    /// The LSP method name (e.g. "textDocument/completion").
    method: String,
    /// Timestamp when the request was sent (for latency tracking).
    sent_at: std::time::Instant,
}

/// Handle to the running transport.
///
/// Clone-friendly: internally Arc-wrapped. The transport runs two background
/// Tokio tasks: a reader task and a writer task.
#[derive(Clone)]
pub struct LspTransport {
    inner: Arc<TransportInner>,
}

struct TransportInner {
    /// Channel to send outgoing messages to the writer task.
    tx: mpsc::UnboundedSender<Vec<u8>>,
    /// Map of request id → waiting oneshot sender.
    ///
    /// Wrapped in `Arc` so the reader task can hold a reference-counted handle
    /// to the same map without requiring `PendingRequest: Clone` (which would
    /// be impossible since `oneshot::Sender` is not `Clone`).
    pending: Arc<dashmap::DashMap<u32, PendingRequest>>,
    /// Monotonic request ID counter.
    next_id: AtomicU32,
}

impl LspTransport {
    /// Spawn the transport on an already-running LSP process.
    ///
    /// Returns the `LspTransport` handle and an mpsc receiver that will
    /// deliver incoming server→client notifications and requests.
    ///
    /// If `latency_tx` is provided, each completed request's round-trip
    /// latency (in microseconds) plus the method name will be sent there.
    pub fn spawn(
        stdin: ChildStdin,
        stdout: ChildStdout,
        latency_tx: Option<crossbeam_channel::Sender<(u64, String)>>,
    ) -> (Self, mpsc::UnboundedReceiver<JsonRpcMessage>) {
        let (out_tx, out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (in_tx, in_rx) = mpsc::unbounded_channel::<JsonRpcMessage>();

        let inner = Arc::new(TransportInner {
            tx: out_tx,
            pending: Arc::new(dashmap::DashMap::new()),
            next_id: AtomicU32::new(1),
        });

        // ── Writer task ───────────────────────────────────────────────────────
        tokio::spawn(async move {
            run_writer(stdin, out_rx).await;
        });

        // ── Reader task ───────────────────────────────────────────────────────
        // Clone the Arc<DashMap> — both tasks now share the same map.
        let reader_pending = Arc::clone(&inner.pending);
        tokio::spawn(async move {
            run_reader(stdout, reader_pending, in_tx, latency_tx).await;
        });

        (Self { inner }, in_rx)
    }

    /// Send a request and await the response.
    ///
    /// The caller gets back the raw `Value` of the `result` field on success,
    /// or an error if the server returned an error object or the transport fails.
    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        self.request_with_timeout(method, params, None).await
    }

    /// Send a request and await the response with an optional timeout.
    ///
    /// If `timeout` is `Some`, the request will be aborted if no response
    /// arrives within the specified duration.
    pub async fn request_with_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Option<std::time::Duration>,
    ) -> Result<Value> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let msg = JsonRpcMessage::request(id, method, params);

        let (respond_tx, respond_rx) = oneshot::channel();
        let sent_at = std::time::Instant::now();
        self.inner.pending.insert(
            id,
            PendingRequest {
                respond: respond_tx,
                method: method.to_owned(),
                sent_at,
            },
        );

        self.send_raw(&msg)?;

        let result = if let Some(dur) = timeout {
            tokio::time::timeout(dur, respond_rx)
                .await
                .map_err(|_| anyhow!("LSP request {method} timed out after {:?}", dur))?
                .map_err(|_| anyhow!("LSP transport closed while awaiting response to {method}"))?
        } else {
            respond_rx
                .await
                .map_err(|_| anyhow!("LSP transport closed while awaiting response to {method}"))?
        };

        // Clean up pending entry if the response never arrived.
        self.inner.pending.remove(&id);

        result
    }

    /// Send a notification (fire-and-forget, no response expected).
    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        let msg = JsonRpcMessage::notification(method, params);
        self.send_raw(&msg)
    }

    /// Send a response to a server→client request.
    ///
    /// `id` is the request id from the original `JsonRpcMessage`.
    /// `result` is the response body (use `Value::Null` for empty).
    pub fn respond(&self, id: &Value, result: Value) -> Result<()> {
        let msg = JsonRpcMessage {
            jsonrpc: "2.0".into(),
            id: Some(id.clone()),
            method: None,
            params: None,
            result: Some(result),
            error: None,
        };
        self.send_raw(&msg)
    }

    fn send_raw(&self, msg: &JsonRpcMessage) -> Result<()> {
        let json = serde_json::to_vec(msg).context("Serialising JSON-RPC message")?;
        let header = format!("Content-Length: {}\r\n\r\n", json.len());

        let mut frame = Vec::with_capacity(header.len() + json.len());
        frame.extend_from_slice(header.as_bytes());
        frame.extend_from_slice(&json);

        self.inner
            .tx
            .send(frame)
            .map_err(|_| anyhow!("LSP writer task has shut down"))
    }
}

// ── Writer task ───────────────────────────────────────────────────────────────

async fn run_writer(mut stdin: ChildStdin, mut rx: mpsc::UnboundedReceiver<Vec<u8>>) {
    while let Some(frame) = rx.recv().await {
        if let Err(e) = stdin.write_all(&frame).await {
            log::error!("LSP writer: failed to write to stdin: {e}");
            break;
        }
    }
    log::debug!("LSP writer task exited");
}

// ── Reader task ───────────────────────────────────────────────────────────────

async fn run_reader(
    stdout: ChildStdout,
    pending: Arc<dashmap::DashMap<u32, PendingRequest>>,
    in_tx: mpsc::UnboundedSender<JsonRpcMessage>,
    latency_tx: Option<crossbeam_channel::Sender<(u64, String)>>,
) {
    let mut reader = BufReader::new(stdout);

    loop {
        // 1. Read headers until blank line
        let mut content_length: Option<usize> = None;
        let mut header_line = String::new();

        loop {
            header_line.clear();
            match reader.read_line(&mut header_line).await {
                Ok(0) => {
                    log::debug!("LSP reader: EOF on stdout");
                    return;
                }
                Err(e) => {
                    log::error!("LSP reader: error reading header: {e}");
                    return;
                }
                Ok(_) => {}
            }

            let trimmed = header_line.trim();
            if trimmed.is_empty() {
                break; // End of headers
            }

            if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
                match rest.trim().parse::<usize>() {
                    Ok(n) => content_length = Some(n),
                    Err(e) => log::warn!("LSP reader: bad Content-Length: {e}"),
                }
            }
            // Ignore other headers (Content-Type etc.)
        }

        let length = match content_length {
            Some(n) => n,
            None => {
                log::warn!("LSP reader: received header block with no Content-Length; skipping");
                continue;
            }
        };

        // 2. Read exactly `length` bytes
        let mut body = vec![0u8; length];
        if let Err(e) = reader.read_exact(&mut body).await {
            log::error!("LSP reader: error reading body: {e}");
            return;
        }

        // 3. Parse JSON-RPC
        let msg: JsonRpcMessage = match serde_json::from_slice(&body) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("LSP reader: malformed JSON-RPC: {e}");
                continue;
            }
        };

        // 4. Dispatch
        if msg.is_response() {
            // Resolve the pending request
            let id = match msg.id.as_ref().and_then(|v| v.as_u64()) {
                Some(n) => n as u32,
                None => {
                    log::warn!("LSP reader: response with non-integer id: {:?}", msg.id);
                    continue;
                }
            };

            match pending.remove(&id) { Some((_, req)) => {
                let elapsed = req.sent_at.elapsed();
                let dur_us = elapsed.as_micros() as u64;
                let outcome = if let Some(err) = msg.error {
                    Err(anyhow!("LSP error {}: {}", err.code, err.message))
                } else {
                    Ok(msg.result.unwrap_or(Value::Null))
                };
                if elapsed > std::time::Duration::from_millis(100) {
                    log::debug!(
                        "LSP request completed in {:.1} ms (id={})",
                        elapsed.as_secs_f64() * 1000.0,
                        id
                    );
                }
                // Emit latency event if a channel was provided (best-effort).
                if let Some(ref tx) = latency_tx {
                    let _ = tx.send((dur_us, req.method.clone()));
                }
                let _ = req.respond.send(outcome);
            } _ => {
                log::warn!("LSP reader: response for unknown request id {id}");
            }}
        } else {
            // Notification or server→client request — forward to LSP client
            if in_tx.send(msg).is_err() {
                log::debug!("LSP reader: notification channel closed; exiting");
                return;
            }
        }
    }
}
