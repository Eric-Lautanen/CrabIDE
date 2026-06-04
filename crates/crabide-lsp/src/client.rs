//! `LspClient` — LSP protocol client state machine.
//!
//! One `LspClient` instance corresponds to one running language server process.
//! It wraps the [`LspTransport`] and exposes high-level methods that match the
//! LSP request/notification surface. Async request methods spawn Tokio tasks
//! that resolve the response and emit an [`LspEvent`] on the editor event bus.
//!
//! # Threading model
//! - All public methods are `Send + Sync` and cheap to call from any thread.
//! - Request methods fire a Tokio task (`tokio::spawn`) and return immediately.
//!
//! - The UI thread never blocks it drains the crossbeam channel per frame.

use std::collections::HashMap;

use std::sync::{
    atomic::{AtomicBool, AtomicU8, Ordering},
    Arc,
};

use parking_lot::RwLock;
use serde_json::{json, Value};

use crabide_core::{
    error::{crabideError, Result},
    event::{EditorEvent, LspEvent},
    types::{DocumentUri, Language, Position, Range, TextEdit},
};

use crate::{
    config::LspServerConfig,
    convert::{
        self, decode_semantic_tokens, from_lsp_code_action_or_command, from_lsp_code_lens,
        from_lsp_completion_item, from_lsp_diagnostic, from_lsp_inlay_hint, from_lsp_location,
        from_lsp_location_link, from_lsp_workspace_edit, hover_to_string, to_lsp_range, to_lsp_uri,
    },
    transport::{JsonRpcMessage, LspTransport},
};

// Text sync kind

const SYNC_FULL: u8 = 1;
const SYNC_INCREMENTAL: u8 = 2;

// ── LspClient ─────────────────────────────────────────────────────────────────

/// A connected language server client.
///
/// Cheap to clone — all state is behind `Arc`.
#[derive(Clone)]
pub struct LspClient {
    inner: Arc<LspClientInner>,
}

struct LspClientInner {
    transport: LspTransport,
    language: Language,
    event_tx: crossbeam_channel::Sender<EditorEvent>,
    initialized: AtomicBool,
    /// TextDocumentSyncKind negotiated during initialize (0/1/2).
    sync_kind: AtomicU8,
    /// Server capabilities from the initialize response (raw Value for flexibility).
    capabilities: RwLock<Option<Value>>,
    /// Per-document version counter (incremented on each change).
    doc_versions: RwLock<HashMap<DocumentUri, u32>>,
}

impl LspClient {
    pub fn new(
        transport: LspTransport,
        language: Language,
        event_tx: crossbeam_channel::Sender<EditorEvent>,
    ) -> Self {
        Self {
            inner: Arc::new(LspClientInner {
                transport,
                language,
                event_tx,
                initialized: AtomicBool::new(false),
                sync_kind: AtomicU8::new(SYNC_FULL),
                capabilities: RwLock::new(None),
                doc_versions: RwLock::new(HashMap::new()),
            }),
        }
    }

    pub fn language(&self) -> &Language {
        &self.inner.language
    }
    pub fn is_initialized(&self) -> bool {
        self.inner.initialized.load(Ordering::Acquire)
    }

    /// Get the current version counter for a document (0 if not tracked).
    pub fn doc_version(&self, uri: &DocumentUri) -> u32 {
        self.inner
            .doc_versions
            .read()
            .get(uri)
            .copied()
            .unwrap_or(0)
    }

    /// Get a clone of the underlying transport for graceful shutdown.
    pub fn transport(&self) -> LspTransport {
        self.inner.transport.clone()
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Send the `initialize` request and await the server's response,
    /// then send the `initialized` notification.
    pub async fn initialize(&self, config: &LspServerConfig) -> Result<()> {
        let root_uri = config.root_uri.as_ref().map(to_lsp_uri);

        let workspace_folders: Value = root_uri
            .as_ref()
            .map(|u: &lsp_types::Uri| {
                // Extract last path segment from the URI string as a workspace name.
                let name = u
                    .as_str()
                    .rsplit('/')
                    .find(|s| !s.is_empty())
                    .unwrap_or("workspace")
                    .to_owned();
                json!([{ "uri": u.as_str(), "name": name }])
            })
            .unwrap_or(Value::Null);

        let params = json!({
            "processId": std::process::id(),
            "clientInfo": { "name": "crabide", "version": env!("CARGO_PKG_VERSION") },
            "rootUri": root_uri.as_ref().map(|u: &lsp_types::Uri| u.as_str()),
            "workspaceFolders": workspace_folders,
            "initializationOptions": config.initialization_options,
            "capabilities": {
                "workspace": {
                    "workspaceFolders": true,
                    "didChangeConfiguration": { "dynamicRegistration": true }
                },
                "textDocument": {
                    "synchronization": {
                        "willSave": true,
        "willSaveWaitUntil": true,
                "didSave": true
                    },
                    "completion": {
                        "completionItem": {
                            "documentationFormat": ["plaintext", "markdown"],
                            "snippetSupport": false
                        }
                    },
                    "hover": {
                        "contentFormat": ["plaintext", "markdown"]
                    },
                    "definition": {},
                                    "typeDefinition": {},
                                    "declaration": {},
                                    "references": {},
                                    "implementation": {},
                    "inlayHint": {},
                    "codeAction": {
                        "codeActionLiteralSupport": {
                            "codeActionKind": {
                                "valueSet": ["quickfix", "refactor", "source"]
                            }
                        },
                        "isPreferredSupport": true
                    },
                    "rename": { "prepareSupport": false },
                    "publishDiagnostics": { "relatedInformation": true, "tagSupport": { "valueSet": [1, 2] } },
            "semanticTokens": {
                "full": true,
                "delta": false,
                "tokenTypes": ["namespace","type","class","enum","interface","struct","typeParameter","parameter","variable","property","enumMember","event","function","method","macro","keyword","modifier","comment","string","number","regexp","operator","decorator"],
                "tokenModifiers": ["declaration","definition","readonly","static","deprecated","abstract","async","modification","documentation","defaultLibrary"]
            },
            "codeLens": {}
                }
            }
        });

        let result = self
            .inner
            .transport
            .request("initialize", params)
            .await
            .map_err(|e| crabideError::LspTransport(e.to_string()))?;

        // Store capabilities.
        let caps = result.get("capabilities").cloned().unwrap_or(Value::Null);
        *self.inner.capabilities.write() = Some(caps.clone());

        // Determine sync kind from capabilities.
        let sync_kind = caps
            .get("textDocumentSync")
            .and_then(|s| {
                // Can be a number directly or an object with a "change" field.
                s.as_u64()
                    .map(|n| n as u8)
                    .or_else(|| s.get("change").and_then(|c| c.as_u64()).map(|n| n as u8))
            })
            .unwrap_or(SYNC_FULL);
        self.inner.sync_kind.store(sync_kind, Ordering::Release);

        // Send initialized notification (no response expected).
        self.inner
            .transport
            .notify("initialized", json!({}))
            .map_err(|e| crabideError::LspTransport(e.to_string()))?;

        self.inner.initialized.store(true, Ordering::Release);

        let lang = self.inner.language.clone();
        let _ = self
            .inner
            .event_tx
            .send(LspEvent::ServerReady { language: lang }.into());

        log::info!("LSP initialized for {}", self.inner.language);
        Ok(())
    }

    /// Spawn the notification dispatch loop as a Tokio task.
    ///
    /// `rx` is the incoming-message channel from [`LspTransport::spawn`].
    pub fn run_notifications(&self, rx: tokio::sync::mpsc::UnboundedReceiver<JsonRpcMessage>) {
        let client = self.clone();
        tokio::spawn(async move {
            client.notification_loop(rx).await;
        });
    }

    async fn notification_loop(
        &self,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<JsonRpcMessage>,
    ) {
        while let Some(msg) = rx.recv().await {
            let method = match &msg.method {
                Some(m) => m.clone(),
                None => continue,
            };
            let params = msg.params.clone().unwrap_or(Value::Null);
            self.handle_notification(&method, params);
        }
        log::debug!("LSP notification loop exited for {}", self.inner.language);
    }

    fn handle_notification(&self, method: &str, params: Value) {
        match method {
            "textDocument/publishDiagnostics" => {
                self.handle_publish_diagnostics(params);
            }
            "window/logMessage" | "window/showMessage" => {
                let msg = params
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("")
                    .to_owned();
                let _ = self.inner.event_tx.send(
                    LspEvent::LogMessage {
                        language: self.inner.language.clone(),
                        message: msg,
                    }
                    .into(),
                );
            }
            "$/progress" | "window/workDoneProgress/create" => {
                // Progress notifications silently ignored for now.
            }
            // Server→client requests: respond with empty/unsupported.
            "workspace/applyEdit" => {
                log::debug!("LSP server requested workspace/applyEdit; not supported yet");
                // Respond with `applied: false` if we can extract the request id.
                if let Some(id) = params.get("id") {
                    let _ = self.inner.transport.notify(
                        "workspace/applyEdit",
                        serde_json::json!({ "id": id, "result": { "applied": false } }),
                    );
                }
            }
            "workspace/configuration" => {
                log::debug!("LSP server requested workspace/configuration; returning empty");
                if let Some(id) = params.get("id") {
                    let _ = self.inner.transport.notify(
                        "workspace/configuration",
                        serde_json::json!({ "id": id, "result": [] }),
                    );
                }
            }
            "client/registerCapability" => {
                log::debug!("LSP server requested client/registerCapability; ignoring");
            }
            "telemetry/event" => {
                log::trace!("LSP telemetry/event: {params}");
            }
            "textDocument/willSave" => {
                // Client→server notification; we send it, don't expect it back.
                log::trace!("LSP willSave notification received (unexpected)");
            }
            "textDocument/willSaveWaitUntil" => {
                // Server→client request: respond with empty edits.
                log::debug!("LSP willSaveWaitUntil request; returning empty edits");
                if let Some(id) = params.get("id") {
                    let _ = self.inner.transport.notify(
                        "textDocument/willSaveWaitUntil",
                        serde_json::json!({ "id": id, "result": null }),
                    );
                }
            }
            _ => {
                log::trace!("LSP unhandled notification: {method}");
            }
        }
    }

    fn handle_publish_diagnostics(&self, params: Value) {
        let params: lsp_types::PublishDiagnosticsParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("publishDiagnostics parse error: {e}");
                return;
            }
        };
        let uri = convert::from_lsp_uri(params.uri);
        let diagnostics = params
            .diagnostics
            .into_iter()
            .map(from_lsp_diagnostic)
            .collect();
        let _ = self
            .inner
            .event_tx
            .send(LspEvent::DiagnosticsPublished { uri, diagnostics }.into());
    }

    // ── Text document synchronization ─────────────────────────────────────────

    /// Notify the server that a document was opened.
    pub fn did_open(&self, uri: &DocumentUri, language: &Language, version: u32, text: &str) {
        self.inner.doc_versions.write().insert(uri.clone(), version);
        let params = json!({
            "textDocument": {
                "uri": to_lsp_uri(uri).as_str(),
                "languageId": language.as_str(),
                "version": version,
                "text": text
            }
        });
        if let Err(e) = self.inner.transport.notify("textDocument/didOpen", params) {
            log::error!("didOpen failed: {e}");
        }
    }

    /// Notify the server of document changes.
    ///
    /// Uses full-document sync or incremental depending on negotiated capability.
    /// The version is auto-incremented from the tracked per-document counter.
    pub fn did_change(&self, uri: &DocumentUri, edits: &[TextEdit], full_text: &str) {
        let version = {
            let mut versions = self.inner.doc_versions.write();
            let v = versions.entry(uri.clone()).or_insert(0);
            *v += 1;
            *v
        };
        let content_changes: Value = if self.inner.sync_kind.load(Ordering::Relaxed)
            == SYNC_INCREMENTAL
        {
            Value::Array(edits.iter().map(|e| {
                json!({
                    "range": {
                        "start": { "line": e.range.start.line, "character": e.range.start.character },
                        "end": { "line": e.range.end.line, "character": e.range.end.character }
                    },
                    "text": e.new_text
                })
            }).collect())
        } else {
            json!([{ "text": full_text }])
        };

        let params = json!({
            "textDocument": { "uri": to_lsp_uri(uri).as_str(), "version": version },
            "contentChanges": content_changes
        });
        if let Err(e) = self
            .inner
            .transport
            .notify("textDocument/didChange", params)
        {
            log::error!("didChange failed: {e}");
        }
    }

    /// Notify the server that a document was saved.
    pub fn did_save(&self, uri: &DocumentUri) {
        let params = json!({ "textDocument": { "uri": to_lsp_uri(uri).as_str() } });
        if let Err(e) = self.inner.transport.notify("textDocument/didSave", params) {
            log::error!("didSave failed: {e}");
        }
    }

    /// Notify the server that a document is about to be saved.
    /// If the server supports `willSaveWaitUntil`, it may respond with edits
    /// to apply before the save is finalized.
    pub fn will_save(&self, uri: &DocumentUri) {
        let params = json!({
            "textDocument": { "uri": to_lsp_uri(uri).as_str() },
            "reason": 1 // Manual = 1
        });
        if let Err(e) = self.inner.transport.notify("textDocument/willSave", params) {
            log::error!("willSave failed: {e}");
        }
    }

    /// Notify the server that a document was closed.
    pub fn did_close(&self, uri: &DocumentUri) {
        self.inner.doc_versions.write().remove(uri);
        let params = json!({ "textDocument": { "uri": to_lsp_uri(uri).as_str() } });
        if let Err(e) = self.inner.transport.notify("textDocument/didClose", params) {
            log::error!("didClose failed: {e}");
        }
    }

    // ── Requests (fire-and-forget: response emitted as LspEvent) ──────────────

    /// Request completion at a position. Result arrives as `LspEvent::CompletionReady`.
    pub fn complete(&self, uri: DocumentUri, position: Position, request_id: u32) {
        let client = self.clone();
        tokio::spawn(async move {
            let params = json!({
                "textDocument": { "uri": to_lsp_uri(&uri).as_str() },
                "position": { "line": position.line, "character": position.character },
                "context": { "triggerKind": 1 }
            });
            match client
                .inner
                .transport
                .request("textDocument/completion", params)
                .await
            {
                Ok(result) => {
                    let (items, is_incomplete) = parse_completion_response(result);
                    let _ = client.inner.event_tx.send(
                        LspEvent::CompletionReady {
                            request_id,
                            items,
                            is_incomplete,
                        }
                        .into(),
                    );
                }
                Err(e) => log::warn!("completion failed: {e}"),
            }
        });
    }

    /// Request hover documentation. Result arrives as `LspEvent::HoverReady`.
    pub fn hover(&self, uri: DocumentUri, position: Position, request_id: u32) {
        let client = self.clone();
        tokio::spawn(async move {
            let params = json!({
                "textDocument": { "uri": to_lsp_uri(&uri).as_str() },
                "position": { "line": position.line, "character": position.character }
            });
            match client
                .inner
                .transport
                .request("textDocument/hover", params)
                .await
            {
                Ok(result) => {
                    let (contents, range) = parse_hover_response(result);
                    let _ = client.inner.event_tx.send(
                        LspEvent::HoverReady {
                            request_id,
                            contents,
                            range,
                        }
                        .into(),
                    );
                }
                Err(e) => log::warn!("hover failed: {e}"),
            }
        });
    }

    /// Request go-to-definition. Result arrives as `LspEvent::LocationsReady`.
    pub fn go_to_definition(&self, uri: DocumentUri, position: Position, request_id: u32) {
        self.location_request("textDocument/definition", uri, position, request_id);
    }

    /// Request find-references. Result arrives as `LspEvent::LocationsReady`.
    pub fn references(&self, uri: DocumentUri, position: Position, request_id: u32) {
        let client = self.clone();
        tokio::spawn(async move {
            let params = json!({
                "textDocument": { "uri": to_lsp_uri(&uri).as_str() },
                "position": { "line": position.line, "character": position.character },
                "context": { "includeDeclaration": true }
            });
            match client
                .inner
                .transport
                .request("textDocument/references", params)
                .await
            {
                Ok(result) => {
                    let locations = parse_locations_array(result);
                    let _ = client.inner.event_tx.send(
                        LspEvent::LocationsReady {
                            request_id,
                            locations,
                        }
                        .into(),
                    );
                }
                Err(e) => log::warn!("references failed: {e}"),
            }
        });
    }

    /// Request go-to-implementation. Result arrives as `LspEvent::LocationsReady`.
    pub fn implementation(&self, uri: DocumentUri, position: Position, request_id: u32) {
        self.location_request("textDocument/implementation", uri, position, request_id);
    }

    /// Request go-to-type-definition. Result arrives as `LspEvent::LocationsReady`.
    pub fn type_definition(&self, uri: DocumentUri, position: Position, request_id: u32) {
        self.location_request("textDocument/typeDefinition", uri, position, request_id);
    }

    /// Request go-to-declaration. Result arrives as `LspEvent::LocationsReady`.
    pub fn declaration(&self, uri: DocumentUri, position: Position, request_id: u32) {
        self.location_request("textDocument/declaration", uri, position, request_id);
    }

    /// Request inlay hints for a document range. Result arrives as `LspEvent::InlayHintsUpdated`.
    pub fn inlay_hints(&self, uri: DocumentUri, range: Range) {
        let client = self.clone();
        tokio::spawn(async move {
            let params = json!({
                "textDocument": { "uri": to_lsp_uri(&uri).as_str() },
                "range": {
                    "start": { "line": range.start.line, "character": range.start.character },
                    "end":   { "line": range.end.line,   "character": range.end.character }
                }
            });
            match client
                .inner
                .transport
                .request("textDocument/inlayHint", params)
                .await
            {
                Ok(result) => {
                    let hints = parse_inlay_hints(result);
                    let _ = client
                        .inner
                        .event_tx
                        .send(LspEvent::InlayHintsUpdated { uri, hints }.into());
                }
                Err(e) => log::warn!("inlayHint failed: {e}"),
            }
        });
    }

    /// Request code actions at a range. Result arrives as `LspEvent::CodeActionsReady`.
    pub fn code_actions(
        &self,
        uri: DocumentUri,
        range: Range,
        diagnostics: Vec<crabide_core::event::Diagnostic>,
        request_id: u32,
    ) {
        let client = self.clone();
        tokio::spawn(async move {
            let params = json!({
                "textDocument": { "uri": to_lsp_uri(&uri).as_str() },
                "range": to_lsp_range(range),
                "context": {
                    "diagnostics": serde_json::to_value(&diagnostics).unwrap_or(json!([]))
                }
            });
            match client
                .inner
                .transport
                .request("textDocument/codeAction", params)
                .await
            {
                Ok(result) => {
                    let actions = parse_code_actions(result);
                    let _ = client.inner.event_tx.send(
                        LspEvent::CodeActionsReady {
                            request_id,
                            actions,
                        }
                        .into(),
                    );
                }
                Err(e) => log::warn!("codeAction failed: {e}"),
            }
        });
    }

    /// Request a rename. Result arrives as `LspEvent::RenameReady`.
    pub fn rename(&self, uri: DocumentUri, position: Position, new_name: String, request_id: u32) {
        let client = self.clone();
        tokio::spawn(async move {
            let params = json!({
                "textDocument": { "uri": to_lsp_uri(&uri).as_str() },
                "position": { "line": position.line, "character": position.character },
                "newName": new_name
            });
            match client
                .inner
                .transport
                .request("textDocument/rename", params)
                .await
            {
                Ok(result) => {
                    if result.is_null() {
                        return;
                    }
                    let workspace_edit =
                        match serde_json::from_value::<lsp_types::WorkspaceEdit>(result) {
                            Ok(we) => from_lsp_workspace_edit(we),
                            Err(e) => {
                                log::warn!("rename parse error: {e}");
                                return;
                            }
                        };
                    let _ = client.inner.event_tx.send(
                        LspEvent::RenameReady {
                            request_id,
                            workspace_edit,
                        }
                        .into(),
                    );
                }
                Err(e) => log::warn!("rename failed: {e}"),
            }
        });
    }

    /// Request document formatting. Returns edits directly (caller applies them).
    pub fn format(&self, uri: DocumentUri, tab_size: u32, insert_spaces: bool, request_id: u32) {
        let client = self.clone();
        tokio::spawn(async move {
            let params = json!({
                "textDocument": { "uri": to_lsp_uri(&uri).as_str() },
                "options": { "tabSize": tab_size, "insertSpaces": insert_spaces }
            });
            match client
                .inner
                .transport
                .request("textDocument/formatting", params)
                .await
            {
                Ok(result) => {
                    let workspace_edit = parse_text_edits_as_workspace_edit(result, &uri);
                    let _ = client.inner.event_tx.send(
                        LspEvent::FormattingReady {
                            request_id,
                            workspace_edit,
                        }
                        .into(),
                    );
                }
                Err(e) => log::warn!("formatting failed: {e}"),
            }
        });
    }

    /// Request range formatting. Result arrives as `LspEvent::FormattingReady`.
    pub fn format_range(
        &self,
        uri: DocumentUri,
        range: Range,
        tab_size: u32,
        insert_spaces: bool,
        request_id: u32,
    ) {
        let client = self.clone();
        tokio::spawn(async move {
            let params = json!({
                "textDocument": { "uri": to_lsp_uri(&uri).as_str() },
                "range": to_lsp_range(range),
                "options": { "tabSize": tab_size, "insertSpaces": insert_spaces }
            });
            match client
                .inner
                .transport
                .request("textDocument/rangeFormatting", params)
                .await
            {
                Ok(result) => {
                    let workspace_edit = parse_text_edits_as_workspace_edit(result, &uri);
                    let _ = client.inner.event_tx.send(
                        LspEvent::FormattingReady {
                            request_id,
                            workspace_edit,
                        }
                        .into(),
                    );
                }
                Err(e) => log::warn!("rangeFormatting failed: {e}"),
            }
        });
    }

    /// Request semantic tokens for a document (full). Result arrives as
    /// `LspEvent::SemanticTokensUpdated`.
    pub fn semantic_tokens_full(&self, uri: DocumentUri) {
        let client = self.clone();
        tokio::spawn(async move {
            let params = json!({
                "textDocument": { "uri": to_lsp_uri(&uri).as_str() }
            });
            match client
                .inner
                .transport
                .request("textDocument/semanticTokens/full", params)
                .await
            {
                Ok(result) => {
                    let tokens = parse_semantic_tokens(result);
                    let _ = client
                        .inner
                        .event_tx
                        .send(LspEvent::SemanticTokensUpdated { uri, tokens }.into());
                }
                Err(e) => log::warn!("semanticTokens/full failed: {e}"),
            }
        });
    }

    /// Request code lens for a document. Result arrives as
    /// `LspEvent::CodeLensUpdated`.
    pub fn code_lens(&self, uri: DocumentUri) {
        let client = self.clone();
        tokio::spawn(async move {
            let params = json!({
                "textDocument": { "uri": to_lsp_uri(&uri).as_str() }
            });
            match client
                .inner
                .transport
                .request("textDocument/codeLens", params)
                .await
            {
                Ok(result) => {
                    let items = parse_code_lens(result);
                    let _ = client
                        .inner
                        .event_tx
                        .send(LspEvent::CodeLensUpdated { uri, items }.into());
                }
                Err(e) => log::warn!("codeLens failed: {e}"),
            }
        });
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Shared helper for definition/implementation location requests.
    fn location_request(
        &self,
        method: &'static str,
        uri: DocumentUri,
        position: Position,
        request_id: u32,
    ) {
        let client = self.clone();
        tokio::spawn(async move {
            let params = json!({
                "textDocument": { "uri": to_lsp_uri(&uri).as_str() },
                "position": { "line": position.line, "character": position.character }
            });
            match client.inner.transport.request(method, params).await {
                Ok(result) => {
                    let locations = parse_goto_response(result);
                    let _ = client.inner.event_tx.send(
                        LspEvent::LocationsReady {
                            request_id,
                            locations,
                        }
                        .into(),
                    );
                }
                Err(e) => log::warn!("{method} failed: {e}"),
            }
        });
    }
}

// ── Response parsers ──────────────────────────────────────────────────────────

fn parse_completion_response(result: Value) -> (Vec<crabide_core::event::CompletionItem>, bool) {
    if result.is_null() {
        return (Vec::new(), false);
    }
    match serde_json::from_value::<lsp_types::CompletionResponse>(result) {
        Ok(resp) => match resp {
            lsp_types::CompletionResponse::Array(items) => (
                items.into_iter().map(from_lsp_completion_item).collect(),
                false,
            ),
            lsp_types::CompletionResponse::List(list) => {
                let items = list
                    .items
                    .into_iter()
                    .map(from_lsp_completion_item)
                    .collect();
                (items, list.is_incomplete)
            }
        },
        Err(e) => {
            log::warn!("completion parse error: {e}");
            (Vec::new(), false)
        }
    }
}

fn parse_hover_response(result: Value) -> (Option<String>, Option<Range>) {
    if result.is_null() {
        return (None, None);
    }
    match serde_json::from_value::<lsp_types::Hover>(result) {
        Ok(hover) => {
            let range = hover.range.map(convert::from_lsp_range);
            let text = hover_to_string(hover);
            (Some(text), range)
        }
        Err(e) => {
            log::warn!("hover parse error: {e}");
            (None, None)
        }
    }
}

fn parse_goto_response(result: Value) -> Vec<crabide_core::event::Location> {
    if result.is_null() {
        return Vec::new();
    }
    // Can be: Location | Location[] | LocationLink[]
    if let Ok(resp) = serde_json::from_value::<lsp_types::GotoDefinitionResponse>(result.clone()) {
        return match resp {
            lsp_types::GotoDefinitionResponse::Scalar(loc) => vec![from_lsp_location(loc)],
            lsp_types::GotoDefinitionResponse::Array(locs) => {
                locs.into_iter().map(from_lsp_location).collect()
            }
            lsp_types::GotoDefinitionResponse::Link(links) => {
                links.into_iter().map(from_lsp_location_link).collect()
            }
        };
    }
    Vec::new()
}

fn parse_locations_array(result: Value) -> Vec<crabide_core::event::Location> {
    if result.is_null() {
        return Vec::new();
    }
    match serde_json::from_value::<Vec<lsp_types::Location>>(result) {
        Ok(locs) => locs.into_iter().map(from_lsp_location).collect(),
        Err(e) => {
            log::warn!("references parse error: {e}");
            Vec::new()
        }
    }
}

fn parse_inlay_hints(result: Value) -> Vec<crabide_core::event::InlayHint> {
    if result.is_null() {
        return Vec::new();
    }
    match serde_json::from_value::<Vec<lsp_types::InlayHint>>(result) {
        Ok(hints) => hints.into_iter().map(from_lsp_inlay_hint).collect(),
        Err(e) => {
            log::warn!("inlayHint parse error: {e}");
            Vec::new()
        }
    }
}

fn parse_code_actions(result: Value) -> Vec<crabide_core::event::CodeAction> {
    if result.is_null() {
        return Vec::new();
    }
    match serde_json::from_value::<Vec<lsp_types::CodeActionOrCommand>>(result) {
        Ok(actions) => actions
            .into_iter()
            .map(from_lsp_code_action_or_command)
            .collect(),
        Err(e) => {
            log::warn!("codeAction parse error: {e}");
            Vec::new()
        }
    }
}

fn parse_text_edits_as_workspace_edit(
    result: Value,
    uri: &DocumentUri,
) -> crabide_core::event::WorkspaceEdit {
    if result.is_null() {
        return crabide_core::event::WorkspaceEdit {
            document_changes: Vec::new(),
        };
    }
    match serde_json::from_value::<Vec<lsp_types::TextEdit>>(result) {
        Ok(edits) => crabide_core::event::WorkspaceEdit {
            document_changes: vec![crabide_core::event::DocumentEdit {
                uri: uri.clone(),
                edits: edits.into_iter().map(convert::from_lsp_text_edit).collect(),
            }],
        },
        Err(e) => {
            log::warn!("formatting parse error: {e}");
            crabide_core::event::WorkspaceEdit {
                document_changes: Vec::new(),
            }
        }
    }
}

fn parse_semantic_tokens(result: Value) -> Vec<crabide_core::event::SemanticToken> {
    if result.is_null() {
        return Vec::new();
    }
    match serde_json::from_value::<lsp_types::SemanticTokens>(result) {
        Ok(st) => decode_semantic_tokens(st),
        Err(e) => {
            log::warn!("semanticTokens parse error: {e}");
            Vec::new()
        }
    }
}

fn parse_code_lens(result: Value) -> Vec<crabide_core::event::CodeLens> {
    if result.is_null() {
        return Vec::new();
    }
    match serde_json::from_value::<Vec<lsp_types::CodeLens>>(result) {
        Ok(items) => items.into_iter().map(from_lsp_code_lens).collect(),
        Err(e) => {
            log::warn!("codeLens parse error: {e}");
            Vec::new()
        }
    }
}
