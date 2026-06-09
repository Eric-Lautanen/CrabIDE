//! Type conversions between crabide's internal types and `lsp-types`.
//!
//! crabide uses its own lean types (defined in `crabide-core`). At the LSP
//! boundary we convert to/from `lsp_types` structs for serialisation.

use crabide_core::{
    event::{
        CodeAction, CodeLens, CompletionItem, CompletionKind, Diagnostic, DiagnosticRelated,
        DiagnosticSeverity, DiagnosticTag, DocumentEdit, InlayHint, InlayHintKind, Location,
        ParameterInformation, ParameterLabel, SemanticToken, SignatureHelp, SignatureInformation,
        WorkspaceEdit,
    },
    types::{DocumentUri, Position, Range, TextEdit},
};

// ── Position / Range ──────────────────────────────────────────────────────────

pub fn to_lsp_pos(p: Position) -> lsp_types::Position {
    lsp_types::Position {
        line: p.line,
        character: p.character,
    }
}

pub fn from_lsp_pos(p: lsp_types::Position) -> Position {
    Position::new(p.line, p.character)
}

pub fn to_lsp_range(r: Range) -> lsp_types::Range {
    lsp_types::Range {
        start: to_lsp_pos(r.start),
        end: to_lsp_pos(r.end),
    }
}

pub fn from_lsp_range(r: lsp_types::Range) -> Range {
    Range::new(from_lsp_pos(r.start), from_lsp_pos(r.end))
}

// ── URI ───────────────────────────────────────────────────────────────────────
// lsp-types 0.97 uses its own `Uri` type (backed by fluent_uri), not url::Url.

pub fn to_lsp_uri(uri: &DocumentUri) -> lsp_types::Uri {
    uri.as_str().parse::<lsp_types::Uri>().unwrap_or_else(|_| {
        "untitled://error"
            .parse::<lsp_types::Uri>()
            .expect("hardcoded fallback URI is always valid")
    })
}

pub fn from_lsp_uri(uri: lsp_types::Uri) -> DocumentUri {
    DocumentUri::parse(uri.as_str()).unwrap_or_else(|_| {
        DocumentUri::parse("untitled://error").expect("hardcoded fallback URI is always valid")
    })
}

// ── TextEdit ──────────────────────────────────────────────────────────────────

pub fn from_lsp_text_edit(e: lsp_types::TextEdit) -> TextEdit {
    TextEdit {
        range: from_lsp_range(e.range),
        new_text: e.new_text,
    }
}

// ── Diagnostic ────────────────────────────────────────────────────────────────

pub fn from_lsp_diagnostic(d: lsp_types::Diagnostic) -> Diagnostic {
    Diagnostic {
        range: from_lsp_range(d.range),
        severity: d
            .severity
            .map(|s| match s {
                lsp_types::DiagnosticSeverity::ERROR => DiagnosticSeverity::Error,
                lsp_types::DiagnosticSeverity::WARNING => DiagnosticSeverity::Warning,
                lsp_types::DiagnosticSeverity::INFORMATION => DiagnosticSeverity::Information,
                _ => DiagnosticSeverity::Hint,
            })
            .unwrap_or(DiagnosticSeverity::Hint),
        code: d.code.map(|c| match c {
            lsp_types::NumberOrString::Number(n) => n.to_string(),
            lsp_types::NumberOrString::String(s) => s,
        }),
        source: d.source,
        message: d.message,
        related_information: d
            .related_information
            .unwrap_or_default()
            .into_iter()
            .map(|ri| DiagnosticRelated {
                location: Location {
                    uri: from_lsp_uri(ri.location.uri),
                    range: from_lsp_range(ri.location.range),
                },
                message: ri.message,
            })
            .collect(),
        tags: d
            .tags
            .unwrap_or_default()
            .into_iter()
            .filter_map(|t| match t {
                lsp_types::DiagnosticTag::UNNECESSARY => Some(DiagnosticTag::Unnecessary),
                lsp_types::DiagnosticTag::DEPRECATED => Some(DiagnosticTag::Deprecated),
                _ => None,
            })
            .collect(),
    }
}

// ── CompletionItem ────────────────────────────────────────────────────────────

pub fn from_lsp_completion_item(item: lsp_types::CompletionItem) -> CompletionItem {
    let documentation = item.documentation.map(|d| match d {
        lsp_types::Documentation::String(s) => s,
        lsp_types::Documentation::MarkupContent(m) => m.value,
    });
    let deprecated = item.deprecated.unwrap_or(false)
        || item
            .tags
            .as_deref()
            .map(|tags| tags.contains(&lsp_types::CompletionItemTag::DEPRECATED))
            .unwrap_or(false);

    CompletionItem {
        label: item.label,
        kind: item.kind.map(from_lsp_completion_kind),
        detail: item.detail,
        documentation,
        insert_text: item.insert_text,
        sort_text: item.sort_text,
        filter_text: item.filter_text,
        preselect: item.preselect.unwrap_or(false),
        deprecated,
    }
}

fn from_lsp_completion_kind(k: lsp_types::CompletionItemKind) -> CompletionKind {
    use CompletionKind as V;
    use lsp_types::CompletionItemKind as L;
    match k {
        L::TEXT => V::Text,
        L::METHOD => V::Method,
        L::FUNCTION => V::Function,
        L::CONSTRUCTOR => V::Constructor,
        L::FIELD => V::Field,
        L::VARIABLE => V::Variable,
        L::CLASS => V::Class,
        L::INTERFACE => V::Interface,
        L::MODULE => V::Module,
        L::PROPERTY => V::Property,
        L::UNIT => V::Unit,
        L::VALUE => V::Value,
        L::ENUM => V::Enum,
        L::KEYWORD => V::Keyword,
        L::SNIPPET => V::Snippet,
        L::COLOR => V::Color,
        L::FILE => V::File,
        L::REFERENCE => V::Reference,
        L::FOLDER => V::Folder,
        L::ENUM_MEMBER => V::EnumMember,
        L::CONSTANT => V::Constant,
        L::STRUCT => V::Struct,
        L::EVENT => V::Event,
        L::OPERATOR => V::Operator,
        L::TYPE_PARAMETER => V::TypeParameter,
        _ => V::Text,
    }
}

// ── InlayHint ─────────────────────────────────────────────────────────────────

pub fn from_lsp_inlay_hint(h: lsp_types::InlayHint) -> InlayHint {
    let label = match h.label {
        lsp_types::InlayHintLabel::String(s) => s,
        lsp_types::InlayHintLabel::LabelParts(parts) => parts
            .into_iter()
            .map(|p| p.value)
            .collect::<Vec<_>>()
            .join(""),
    };
    let tooltip = h.tooltip.map(|t| match t {
        lsp_types::InlayHintTooltip::String(s) => s,
        lsp_types::InlayHintTooltip::MarkupContent(m) => m.value,
    });
    InlayHint {
        position: from_lsp_pos(h.position),
        label,
        kind: h.kind.map(|k| match k {
            lsp_types::InlayHintKind::TYPE => InlayHintKind::Type,
            lsp_types::InlayHintKind::PARAMETER => InlayHintKind::Parameter,
            _ => InlayHintKind::Type,
        }),
        tooltip,
        padding_left: h.padding_left.unwrap_or(false),
        padding_right: h.padding_right.unwrap_or(false),
    }
}

// ── Location ──────────────────────────────────────────────────────────────────

pub fn from_lsp_location(l: lsp_types::Location) -> Location {
    Location {
        uri: from_lsp_uri(l.uri),
        range: from_lsp_range(l.range),
    }
}

pub fn from_lsp_location_link(l: lsp_types::LocationLink) -> Location {
    Location {
        uri: from_lsp_uri(l.target_uri),
        range: from_lsp_range(l.target_range),
    }
}

// ── CodeAction ────────────────────────────────────────────────────────────────

pub fn from_lsp_code_action_or_command(item: lsp_types::CodeActionOrCommand) -> CodeAction {
    match item {
        lsp_types::CodeActionOrCommand::Command(cmd) => CodeAction {
            title: cmd.title,
            kind: None,
            diagnostics: Vec::new(),
            edit: None,
            command: Some(cmd.command),
            is_preferred: false,
        },
        lsp_types::CodeActionOrCommand::CodeAction(ca) => CodeAction {
            title: ca.title,
            kind: ca.kind.map(|k| k.as_str().to_owned()),
            diagnostics: ca
                .diagnostics
                .unwrap_or_default()
                .into_iter()
                .map(from_lsp_diagnostic)
                .collect(),
            edit: ca.edit.map(from_lsp_workspace_edit),
            command: ca.command.map(|c| c.command),
            is_preferred: ca.is_preferred.unwrap_or(false),
        },
    }
}

// ── WorkspaceEdit ─────────────────────────────────────────────────────────────

pub fn from_lsp_workspace_edit(we: lsp_types::WorkspaceEdit) -> WorkspaceEdit {
    let mut document_changes: Vec<DocumentEdit> = Vec::new();

    // Prefer document_changes over changes (document_changes is more expressive).
    if let Some(dc) = we.document_changes {
        match dc {
            lsp_types::DocumentChanges::Edits(edits) => {
                for edit in edits {
                    let uri = from_lsp_uri(edit.text_document.uri);
                    let text_edits = edit
                        .edits
                        .into_iter()
                        .filter_map(|e| match e {
                            lsp_types::OneOf::Left(te) => Some(from_lsp_text_edit(te)),
                            lsp_types::OneOf::Right(_) => None, // annotated edits: ignore annotation
                        })
                        .collect();
                    document_changes.push(DocumentEdit {
                        uri,
                        edits: text_edits,
                    });
                }
            }
            lsp_types::DocumentChanges::Operations(_) => {
                // Resource operations (create/rename/delete) — not yet handled.
            }
        }
    } else if let Some(changes) = we.changes {
        for (url, edits) in changes {
            let uri = from_lsp_uri(url);
            let text_edits = edits.into_iter().map(from_lsp_text_edit).collect();
            document_changes.push(DocumentEdit {
                uri,
                edits: text_edits,
            });
        }
    }

    WorkspaceEdit { document_changes }
}

// ── Hover contents → plain string ─────────────────────────────────────────────

pub fn hover_to_string(hover: lsp_types::Hover) -> String {
    match hover.contents {
        lsp_types::HoverContents::Scalar(ms) => marked_string_to_str(ms),
        lsp_types::HoverContents::Array(items) => items
            .into_iter()
            .map(marked_string_to_str)
            .collect::<Vec<_>>()
            .join("\n\n"),
        lsp_types::HoverContents::Markup(mc) => mc.value,
    }
}

fn marked_string_to_str(ms: lsp_types::MarkedString) -> String {
    match ms {
        lsp_types::MarkedString::String(s) => s,
        lsp_types::MarkedString::LanguageString(ls) => ls.value,
    }
}

// SemanticTokens

/// Decode LSP delta-encoded semantic tokens into absolute-position tokens.
///
/// LSP semantic tokens are delta-encoded: each token's line/character is
/// relative to the previous token. This function decodes them into absolute
/// positions and converts to our `SemanticToken` type.
pub fn decode_semantic_tokens(st: lsp_types::SemanticTokens) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_char = 0u32;

    for t in st.data {
        let line = prev_line + t.delta_line;
        let character = if t.delta_line == 0 {
            prev_char + t.delta_start
        } else {
            t.delta_start
        };
        prev_line = line;
        prev_char = character;

        let start = Position::new(line, character);
        // Length is in UTF-16 code units; we approximate as character count
        // for the end position. Full UTF-16 handling happens in the UI layer.
        let end = Position::new(line, character + t.length);

        tokens.push(SemanticToken {
            range: Range::new(start, end),
            token_type: t.token_type,
            token_modifiers: t.token_modifiers_bitset,
        });
    }

    tokens
}

// CodeLens

pub fn from_lsp_code_lens(cl: lsp_types::CodeLens) -> CodeLens {
    CodeLens {
        range: from_lsp_range(cl.range),
        title: cl
            .command
            .as_ref()
            .map(|c| c.title.clone())
            .unwrap_or_default(),
        command: cl.command.map(|c| c.command),
    }
}

// ── SignatureHelp ────────────────────────────────────────────────────────────

/// Convert an lsp_types::SignatureHelp into our internal type.
pub fn from_lsp_signature_help(sh: lsp_types::SignatureHelp) -> SignatureHelp {
    SignatureHelp {
        signatures: sh.signatures.into_iter().map(from_lsp_si).collect(),
        active_signature: sh.active_signature,
        active_parameter: sh.active_parameter,
    }
}

fn from_lsp_si(si: lsp_types::SignatureInformation) -> SignatureInformation {
    let doc = si.documentation.map(|d| match d {
        lsp_types::Documentation::String(s) => s,
        lsp_types::Documentation::MarkupContent(m) => m.value,
    });
    SignatureInformation {
        label: si.label,
        documentation: doc,
        parameters: si
            .parameters
            .unwrap_or_default()
            .into_iter()
            .map(from_lsp_pi)
            .collect(),
    }
}

fn from_lsp_pi(pi: lsp_types::ParameterInformation) -> ParameterInformation {
    let doc = pi.documentation.map(|d| match d {
        lsp_types::Documentation::String(s) => s,
        lsp_types::Documentation::MarkupContent(m) => m.value,
    });
    ParameterInformation {
        label: match pi.label {
            lsp_types::ParameterLabel::Simple(s) => ParameterLabel::Simple(s),
            lsp_types::ParameterLabel::LabelOffsets([start, end]) => {
                ParameterLabel::Offsets(start, end)
            }
        },
        documentation: doc,
    }
}
