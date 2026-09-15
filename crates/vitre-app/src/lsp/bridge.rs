//! The editor's LSP provider traits backed by the sidecar's `lsp.*` RPCs,
//! porting the Electron bridge's semantics (`lspBridge.ts`, `useLspBridge.ts`):
//! full-text document sync (didOpen at version 0, didChange debounced 200ms
//! with a monotonic version, didClose on switch), completion gated by the
//! `completionAnchor` word/trigger-char rule, lazy `completionItem/resolve`
//! for auto-imports, hover, and definition. Failures degrade to plain editing
//! — a missing language server never surfaces an error.
//!
//! Two position conventions meet here. The wire speaks LSP UTF-16 positions
//! (`super::positions`). The editor's own `lsp_types::Position` handling
//! treats `character` as a *char* index (`rope_ext.rs`), so every position
//! that crosses the boundary is normalized through byte offsets:
//! wire → [`positions::wire_to_offset`] → [`editor_position`], and requests
//! go [`positions::offset_to_wire`] straight from the editor's byte offsets.
//! The one deliberate exception: cross-file definition targets keep their
//! WIRE positions inside the `LocationLink` — the current buffer can't
//! convert offsets for a file it doesn't hold, and the `show_document`
//! handler (files.rs) converts against the target file once it loads.

use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use futures::{FutureExt as _, future::Shared};
use gpui::{App, Task, WeakEntity, Window};
use gpui_component::input::EditorState;
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit,
    Documentation, Hover, HoverContents, LocationLink, MarkupContent, MarkupKind, NumberOrString,
    TextEdit,
};
use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use ropey::{LineType, Rope};
use vitre_client::EnvironmentClient;
use vitre_contracts::methods::{
    LspCompletion, LspDefinition, LspDidChange, LspDidClose, LspDidOpen, LspHover,
    LspResolveCompletion, LspServerStatus,
};
use vitre_contracts::{
    LspCompletionItem as WireCompletionItem, LspDiagnostic as WireDiagnostic, LspDidChangeInput,
    LspDidOpenInput, LspDocumentInput, LspPosition as WireLspPosition, LspPositionInput,
    LspRange as WireRange, LspResolveCompletionInput, LspServerStatusPayload, LspTextEdit,
    NonNegativeInt, TrimmedNonEmptyString,
};
use vitre_state::lsp_gating;

use super::positions::{WirePosition, offset_to_wire, wire_range_to_offsets};

mod semantic;

/// Electron's `DOC_SYNC_DEBOUNCE_MS` (lspBridge.ts): keystrokes coalesce for
/// 200ms before a full-text didChange goes out.
const DOC_SYNC_DEBOUNCE: Duration = Duration::from_millis(200);

fn tnes(text: impl Into<String>) -> TrimmedNonEmptyString {
    TrimmedNonEmptyString(text.into())
}

struct DocState {
    relative_path: String,
    /// Client-owned monotonic version: didOpen is 0, first didChange is 1
    /// (useLspBridge.ts `versionRef`).
    version: i64,
    /// Fingerprint of the text the server was last sent. A flush compares the
    /// buffer against this rather than trusting an edit flag: the editor asks
    /// its completion provider from `on_text_typed`, which runs *before* the
    /// `InputEvent::Change` that drives [`LspBridge::document_edited`], so a
    /// flag would still read "clean" and the server would answer a keystroke
    /// behind — with the cursor past its idea of the line end.
    synced: TextFingerprint,
    /// Debounce generation — each edit bumps it; only the latest timer sends.
    debounce: u64,
}

/// Cheap "does the server already have this text?" check — byte length plus a
/// hash, so the common no-op flush never materializes the rope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TextFingerprint {
    len: usize,
    hash: u64,
}

impl TextFingerprint {
    fn of_rope(text: &Rope) -> Self {
        let mut hasher = DefaultHasher::new();
        for chunk in text.chunks() {
            hasher.write(chunk.as_bytes());
        }
        Self {
            len: text.len(),
            hash: hasher.finish(),
        }
    }

    fn of_str(text: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        hasher.write(text.as_bytes());
        Self {
            len: text.len(),
            hash: hasher.finish(),
        }
    }
}

pub struct LspBridge {
    client: Arc<EnvironmentClient>,
    cwd: String,
    editor: WeakEntity<EditorState>,
    doc: RefCell<Option<DocState>>,
    /// Server-reported extensions (leading-dot, lowercase). `None` until the
    /// first `lsp.serverStatus` lands — the static built-in list gates until
    /// then (useLspBridge.ts cold-load behavior).
    supported: RefCell<Option<Vec<String>>>,
    requested_path: RefCell<Option<String>>,
    epoch: Rc<Cell<u64>>,
    /// Serialize open/change/close, including changes still in flight when a
    /// position request sees an already queued fingerprint.
    sync: RefCell<Shared<Task<bool>>>,
}

impl LspBridge {
    pub fn new(
        client: Arc<EnvironmentClient>,
        cwd: String,
        editor: WeakEntity<EditorState>,
    ) -> Rc<Self> {
        Rc::new(Self {
            client,
            cwd,
            editor,
            doc: RefCell::new(None),
            supported: RefCell::new(None),
            requested_path: RefCell::new(None),
            epoch: Rc::new(Cell::new(0)),
            sync: RefCell::new(Task::ready(true).shared()),
        })
    }

    /// Refresh the supported-extension gate. Cheap on the server (it reports
    /// the configured registry without starting anything), so files.rs calls
    /// this on panel creation and again per file open.
    pub fn refresh_server_status(self: &Rc<Self>, cx: &mut App) {
        let client = self.client.clone();
        let payload = LspServerStatusPayload {
            cwd: tnes(&self.cwd),
        };
        let bridge = self.clone();
        cx.spawn(async move |cx| {
            let Ok(result) = client.call::<LspServerStatus>(&payload).await else {
                return;
            };
            // Once loaded, absent decodes as the empty list — the static
            // fallback only covers the not-yet-loaded window.
            let extensions = result.supported_extensions.flatten().unwrap_or_default();
            *bridge.supported.borrow_mut() = Some(extensions);
            let pending = bridge.requested_path.borrow().clone();
            if bridge.current_document().is_none()
                && let Some(path) = pending.filter(|path| bridge.is_supported(path))
                && let Some(editor) = bridge.editor.upgrade()
            {
                cx.update(|cx| {
                    let contents = editor.read(cx).text().to_string();
                    bridge.open_document(&path, contents, cx);
                });
            }
        })
        .detach();
    }

    pub fn is_supported(&self, relative_path: &str) -> bool {
        let supported = self.supported.borrow();
        lsp_gating::is_lsp_supported_path(relative_path, supported.as_deref())
    }

    /// The relative path of the currently synced document, if any.
    pub fn current_document(&self) -> Option<String> {
        self.doc
            .borrow()
            .as_ref()
            .map(|doc| doc.relative_path.clone())
    }

    /// Close the previous document (if any) and open `relative_path` when its
    /// extension is supported. Returns whether LSP attached.
    pub fn open_document(self: &Rc<Self>, relative_path: &str, contents: String, cx: &mut App) {
        self.close_document(cx);
        *self.requested_path.borrow_mut() = Some(relative_path.to_string());
        if !self.is_supported(relative_path) {
            return;
        }
        *self.doc.borrow_mut() = Some(DocState {
            relative_path: relative_path.to_string(),
            version: 0,
            synced: TextFingerprint::of_str(&contents),
            debounce: 0,
        });
        let client = self.client.clone();
        let payload = LspDidOpenInput {
            contents: tnes(contents),
            cwd: tnes(&self.cwd),
            relative_path: tnes(relative_path),
        };
        let previous = self.sync.borrow().clone();
        *self.sync.borrow_mut() = cx
            .spawn(async move |_| {
                previous.await;
                client.call::<LspDidOpen>(&payload).await.is_ok()
            })
            .shared();
    }

    pub fn close_document(&self, cx: &mut App) {
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.requested_path.borrow_mut().take();
        let Some(doc) = self.doc.borrow_mut().take() else {
            return;
        };
        let client = self.client.clone();
        let payload = LspDocumentInput {
            cwd: tnes(&self.cwd),
            relative_path: tnes(doc.relative_path),
        };
        let previous = self.sync.borrow().clone();
        *self.sync.borrow_mut() = cx
            .spawn(async move |_| {
                previous.await;
                client.call::<LspDidClose>(&payload).await.is_ok()
            })
            .shared();
    }

    /// A user edit landed in the editor: coalesce for 200ms, then send the
    /// full text as a didChange.
    pub fn document_edited(self: &Rc<Self>, cx: &mut App) {
        let generation = {
            let mut doc = self.doc.borrow_mut();
            let Some(doc) = doc.as_mut() else {
                return;
            };
            doc.debounce += 1;
            doc.debounce
        };
        let bridge = self.clone();
        let epoch = self.epoch.get();
        cx.spawn(async move |cx| {
            cx.background_executor().timer(DOC_SYNC_DEBOUNCE).await;
            let latest = bridge
                .doc
                .borrow()
                .as_ref()
                .is_some_and(|doc| doc.debounce == generation && bridge.epoch.get() == epoch);
            if latest {
                cx.update(|cx| bridge.flush_from_editor(cx).detach());
            }
        })
        .detach();
    }

    /// Send a didChange now if `text` has drifted from the server's copy.
    /// Position requests await the returned task so the server never answers
    /// against stale text, including mouse hover.
    ///
    /// The buffer is passed in rather than read back from the editor entity:
    /// the provider callbacks that flush run *inside* an editor update, where
    /// reading the entity again panics.
    fn flush(&self, text: &Rope, cx: &mut App) -> Task<()> {
        let previous = self.sync.borrow().clone();
        let (payload, changed) = {
            let mut doc = self.doc.borrow_mut();
            let Some(doc) = doc.as_mut() else {
                return Task::ready(());
            };
            let fingerprint = TextFingerprint::of_rope(text);
            let changed = fingerprint != doc.synced;
            if !changed && previous.peek() == Some(&true) {
                return Task::ready(());
            }
            doc.synced = fingerprint;
            doc.version += 1;
            (
                LspDidChangeInput {
                    contents: tnes(text.to_string()),
                    cwd: tnes(&self.cwd),
                    relative_path: tnes(&doc.relative_path),
                    version: NonNegativeInt(doc.version),
                },
                changed,
            )
        };
        let client = self.client.clone();
        let task = cx
            .spawn(async move |_| {
                let synced = previous.await;
                if !changed && synced {
                    return true;
                }
                client.call::<LspDidChange>(&payload).await.is_ok()
            })
            .shared();
        *self.sync.borrow_mut() = task.clone();
        cx.spawn(async move |_| {
            task.await;
        })
    }

    /// [`Self::flush`] for callers outside this module — the format command,
    /// which must not ask the server to format text it has not seen.
    pub fn flush_document(&self, text: &Rope, cx: &mut App) -> Task<()> {
        self.flush(text, cx)
    }

    /// The debounce timer's flush, which runs outside any editor update and so
    /// may read the buffer back from the editor entity.
    fn flush_from_editor(&self, cx: &mut App) -> Task<()> {
        let Some(editor) = self.editor.upgrade() else {
            return Task::ready(());
        };
        let text = editor.read(cx).text().clone();
        self.flush(&text, cx)
    }

    pub fn position_payload(
        &self,
        relative_path: &str,
        position: WirePosition,
    ) -> LspPositionInput {
        LspPositionInput {
            cwd: tnes(&self.cwd),
            position: WireLspPosition {
                character: NonNegativeInt(position.character as i64),
                line: NonNegativeInt(position.line as i64),
            },
            relative_path: tnes(relative_path),
        }
    }
}

impl gpui_component::input::CompletionProvider for LspBridge {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        trigger: CompletionContext,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<CompletionResponse>> {
        let empty = || Task::ready(Ok(CompletionResponse::Array(vec![])));
        let Some(relative_path) = self.current_document() else {
            return empty();
        };
        // The Electron gate (lspBridge.ts `completionAnchor`): a trailing
        // word anchors the query at the word start; otherwise only a
        // member/trigger character fires — unless the user asked for the menu
        // (`mod+i`), which anchors at the bare cursor.
        let explicit = trigger.trigger_kind == lsp_types::CompletionTriggerKind::INVOKED;
        let line_start =
            text.line_to_byte_idx(text.byte_to_line_idx(offset, LineType::LF), LineType::LF);
        let line_before = text.slice(line_start..offset).to_string();
        let Some(anchor_in_line) = completion_anchor(&line_before, explicit) else {
            return empty();
        };
        let anchor = editor_position(text, line_start + anchor_in_line);
        let cursor = editor_position(text, offset);
        let query = line_before[anchor_in_line..].to_string();
        let payload = self.position_payload(&relative_path, offset_to_wire(text, offset));
        let snapshot = text.clone();
        let client = self.client.clone();
        let flush = self.flush(text, cx);
        let epoch = self.epoch.clone();
        let generation = epoch.get();
        cx.spawn(async move |_| {
            flush.await;
            let result = client
                .call::<LspCompletion>(&payload)
                .await
                .map_err(|error| anyhow!("lsp.completion failed: {error}"))?;
            if epoch.get() != generation {
                return Ok(CompletionResponse::Array(vec![]));
            }
            // The server answers a member position with every member; the
            // typed prefix narrows it here because gpui-component's menu
            // renders the provider's list verbatim (completion_menu.rs), while
            // CodeMirror filtered it for the Electron bridge.
            let items = ranked_completions(result.items, &snapshot, anchor, cursor, &query);
            // `isIncomplete` is ignored, as in the Electron bridge — the menu
            // re-queries on every keystroke anyway.
            Ok(CompletionResponse::Array(items))
        })
    }

    fn resolve_completion(
        &self,
        item: CompletionItem,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<CompletionItem>> {
        let Some(relative_path) = self.current_document() else {
            return Task::ready(Ok(item));
        };
        let Some(serde_json::Value::String(data)) = item.data.clone() else {
            return Task::ready(Ok(item));
        };
        let payload = LspResolveCompletionInput {
            cwd: tnes(&self.cwd),
            relative_path: tnes(relative_path),
            resolve_data: tnes(data),
        };
        let client = self.client.clone();
        let editor = self.editor.clone();
        let epoch = self.epoch.clone();
        let generation = epoch.get();
        cx.spawn(async move |cx| {
            let resolved = client
                .call::<LspResolveCompletion>(&payload)
                .await
                .map_err(|error| anyhow!("lsp.resolveCompletion failed: {error}"))?;
            if epoch.get() != generation {
                return Err(anyhow!("Document changed during completion"));
            }
            // Auto-import ranges convert against the buffer as it stands now;
            // they never overlap the completion range (Electron makes the
            // same assumption).
            let snapshot = editor
                .upgrade()
                .map(|editor| cx.update(|cx| editor.read(cx).text().clone()));
            let mut item = item;
            if let (Some(text), Some(Some(edits))) = (&snapshot, &resolved.additional_text_edits) {
                item.additional_text_edits = Some(
                    edits
                        .iter()
                        .map(|edit| editor_text_edit(text, edit))
                        .collect(),
                );
            }
            if item.detail.is_none()
                && let Some(Some(detail)) = resolved.detail
            {
                item.detail = Some(detail.0);
            }
            if item.documentation.is_none()
                && let Some(Some(documentation)) = resolved.documentation
            {
                item.documentation = Some(markdown(documentation.0));
            }
            Ok(item)
        })
    }

    fn is_completion_trigger(&self, _offset: usize, new_text: &str, _cx: &mut App) -> bool {
        if self.current_document().is_none() {
            return false;
        }
        // Cheap per-keystroke gate; `completions` re-checks with full line
        // context. Word chars keep an open query alive, trigger chars start
        // member access (lspBridge.ts COMPLETION_TRIGGER_CHARS).
        completion_anchor(new_text, false).is_some()
    }
}

impl gpui_component::input::HoverProvider for LspBridge {
    fn hover(
        &self,
        text: &Rope,
        offset: usize,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Option<Hover>>> {
        let Some(relative_path) = self.current_document() else {
            return Task::ready(Ok(None));
        };
        let payload = self.position_payload(&relative_path, offset_to_wire(text, offset));
        let snapshot = text.clone();
        let client = self.client.clone();
        let flush = self.flush(text, cx);
        let epoch = self.epoch.clone();
        let generation = epoch.get();
        cx.spawn(async move |_| {
            flush.await;
            let result = client
                .call::<LspHover>(&payload)
                .await
                .map_err(|error| anyhow!("lsp.hover failed: {error}"))?;
            if epoch.get() != generation {
                return Ok(None);
            }
            Ok(result.map(|inline| Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: inline.contents.0,
                }),
                range: inline
                    .range
                    .flatten()
                    .map(|range| editor_range(&snapshot, &range)),
            }))
        })
    }
}

impl gpui_component::input::DefinitionProvider for LspBridge {
    fn definitions(
        &self,
        text: &Rope,
        offset: usize,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Vec<LocationLink>>> {
        let Some(relative_path) = self.current_document() else {
            return Task::ready(Ok(vec![]));
        };
        let payload = self.position_payload(&relative_path, offset_to_wire(text, offset));
        let snapshot = text.clone();
        let cwd = self.cwd.clone();
        let client = self.client.clone();
        let flush = self.flush(text, cx);
        let epoch = self.epoch.clone();
        let generation = epoch.get();
        cx.spawn(async move |_| {
            flush.await;
            let result = client
                .call::<LspDefinition>(&payload)
                .await
                .map_err(|error| anyhow!("lsp.definition failed: {error}"))?;
            if epoch.get() != generation {
                return Ok(vec![]);
            }
            let links = result
                .locations
                .into_iter()
                .filter_map(|location| {
                    // Out-of-workspace targets (absolutePath only) are
                    // dropped, as in the Electron editor.
                    let target = location.relative_path.flatten()?.0;
                    let uri = file_uri(&cwd, &target)?;
                    // Same-file ranges are normalized to the editor's char
                    // convention (the editor jumps internally); cross-file
                    // ranges stay in WIRE units for the show_document
                    // handler, which converts against the target file.
                    let range = if target == relative_path {
                        editor_range(&snapshot, &location.range)
                    } else {
                        verbatim_range(&location.range)
                    };
                    Some(LocationLink {
                        origin_selection_range: None,
                        target_uri: uri,
                        target_range: range,
                        target_selection_range: range,
                    })
                })
                .collect();
            Ok(links)
        })
    }
}

/// The editor's internal `Position` convention: `character` is a char index
/// within the line (`rope_ext.rs` `offset_to_position`), not UTF-16.
pub fn editor_position(text: &Rope, offset: usize) -> lsp_types::Position {
    let offset = text.floor_char_boundary(offset.min(text.len()));
    let line = text.byte_to_line_idx(offset, LineType::LF);
    let line_start = text.line_to_byte_idx(line, LineType::LF);
    let character = text.slice(line_start..offset).chars().count();
    lsp_types::Position::new(line as u32, character as u32)
}

/// Wire UTF-16 range → editor-convention range, via byte offsets.
pub fn editor_range(text: &Rope, range: &WireRange) -> lsp_types::Range {
    let offsets =
        wire_range_to_offsets(text, wire_position(&range.start), wire_position(&range.end));
    lsp_types::Range {
        start: editor_position(text, offsets.start),
        end: editor_position(text, offsets.end),
    }
}

/// Wire range carried through verbatim (still UTF-16) — only for cross-file
/// definition targets, where the current buffer can't convert.
fn verbatim_range(range: &WireRange) -> lsp_types::Range {
    let position = |p: &WireLspPosition| {
        lsp_types::Position::new(p.line.0.max(0) as u32, p.character.0.max(0) as u32)
    };
    lsp_types::Range {
        start: position(&range.start),
        end: position(&range.end),
    }
}

pub fn wire_position(position: &WireLspPosition) -> WirePosition {
    WirePosition {
        line: position.line.0.max(0) as u32,
        character: position.character.0.max(0) as u32,
    }
}

/// A buffer edit already expressed in byte offsets — the git gutter's hunk
/// revert, which computes its range against the buffer itself rather than
/// receiving one from a server.
pub fn editor_offset_edit(
    text: &Rope,
    range: std::ops::Range<usize>,
    new_text: String,
) -> TextEdit {
    TextEdit {
        range: lsp_types::Range {
            start: editor_position(text, range.start),
            end: editor_position(text, range.end),
        },
        new_text,
    }
}

pub fn editor_text_edit(text: &Rope, edit: &LspTextEdit) -> TextEdit {
    TextEdit {
        range: editor_range(text, &edit.range),
        new_text: edit.new_text.0.clone(),
    }
}

fn markdown(value: String) -> Documentation {
    Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value,
    })
}

/// How well `label` answers the typed `query`, lower being better, or `None`
/// when it does not match at all and the menu should drop it.
///
/// Electron leans on CodeMirror's own `FuzzyMatcher` here (`lspCompletionSource`
/// returns the anchor as `from` and leaves the default filter on), which is
/// case-insensitive and subsequence-based, ranking prefix matches first. This
/// is that shape in miniature. Callers use LSP `filterText` where supplied,
/// so decorated labels and auto-import suggestions remain discoverable.
fn query_rank(label: &str, query: &str) -> Option<u8> {
    if query.is_empty() {
        return Some(0);
    }
    if label.starts_with(query) {
        return Some(0);
    }
    let label = label.to_lowercase();
    let query = query.to_lowercase();
    if label.starts_with(&query) {
        return Some(1);
    }
    let mut rest = label.chars();
    query
        .chars()
        .all(|needle| rest.any(|candidate| candidate == needle))
        .then_some(2)
}

fn completion_anchor(line: &str, explicit: bool) -> Option<usize> {
    static WORD: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    WORD.get_or_init(|| regex::Regex::new(r"[\p{XID_Continue}$]+$").unwrap())
        .find(line)
        .map(|word| word.start())
        .or_else(|| lsp_gating::completion_anchor(line, explicit))
}

fn ranked_completions(
    items: Vec<WireCompletionItem>,
    text: &Rope,
    anchor: lsp_types::Position,
    cursor: lsp_types::Position,
    query: &str,
) -> Vec<CompletionItem> {
    let mut ranked: Vec<_> = items
        .into_iter()
        .filter_map(|item| {
            let filter = item
                .filter_text
                .as_ref()
                .and_then(|v| v.as_ref())
                .map_or(item.label.0.as_str(), |v| v.0.as_str());
            let rank = query_rank(filter, query)?;
            Some((rank, completion_item(item, text, anchor, cursor)))
        })
        .collect();
    ranked.sort_by(|(a_rank, a), (b_rank, b)| {
        a_rank.cmp(b_rank).then_with(|| {
            a.sort_text
                .as_ref()
                .unwrap_or(&a.label)
                .cmp(b.sort_text.as_ref().unwrap_or(&b.label))
        })
    });
    ranked.into_iter().map(|(_, item)| item).collect()
}

/// Wire completion item → `lsp_types`. When the server names no range, the
/// edit replaces anchor..cursor — the Electron apply path (`toCompletion`),
/// and it sidesteps the editor's insert-at-cursor fallback that would keep
/// the typed prefix.
fn completion_item(
    item: WireCompletionItem,
    text: &Rope,
    anchor: lsp_types::Position,
    cursor: lsp_types::Position,
) -> CompletionItem {
    let insert = item
        .insert_text
        .flatten()
        .map(|text| text.0)
        .unwrap_or_else(|| item.label.0.clone());
    let range = item
        .range
        .flatten()
        .map(|range| editor_range(text, &range))
        .unwrap_or(lsp_types::Range {
            start: anchor,
            end: cursor,
        });
    CompletionItem {
        label: item.label.0,
        kind: item.kind.flatten().and_then(|kind| completion_kind(kind.0)),
        detail: item.detail.flatten().map(|detail| detail.0),
        documentation: item
            .documentation
            .flatten()
            .map(|documentation| markdown(documentation.0)),
        sort_text: item.sort_text.flatten().map(|sort| sort.0),
        filter_text: item.filter_text.flatten().map(|filter| filter.0),
        data: item
            .resolve_data
            .flatten()
            .map(|data| serde_json::Value::String(data.0)),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range,
            new_text: insert,
        })),
        additional_text_edits: item.additional_text_edits.flatten().map(|edits| {
            edits
                .iter()
                .map(|edit| editor_text_edit(text, edit))
                .collect()
        }),
        ..Default::default()
    }
}

/// LSP `CompletionItemKind` numbers 1–25.
fn completion_kind(kind: i64) -> Option<CompletionItemKind> {
    Some(match kind {
        1 => CompletionItemKind::TEXT,
        2 => CompletionItemKind::METHOD,
        3 => CompletionItemKind::FUNCTION,
        4 => CompletionItemKind::CONSTRUCTOR,
        5 => CompletionItemKind::FIELD,
        6 => CompletionItemKind::VARIABLE,
        7 => CompletionItemKind::CLASS,
        8 => CompletionItemKind::INTERFACE,
        9 => CompletionItemKind::MODULE,
        10 => CompletionItemKind::PROPERTY,
        11 => CompletionItemKind::UNIT,
        12 => CompletionItemKind::VALUE,
        13 => CompletionItemKind::ENUM,
        14 => CompletionItemKind::KEYWORD,
        15 => CompletionItemKind::SNIPPET,
        16 => CompletionItemKind::COLOR,
        17 => CompletionItemKind::FILE,
        18 => CompletionItemKind::REFERENCE,
        19 => CompletionItemKind::FOLDER,
        20 => CompletionItemKind::ENUM_MEMBER,
        21 => CompletionItemKind::CONSTANT,
        22 => CompletionItemKind::STRUCT,
        23 => CompletionItemKind::EVENT,
        24 => CompletionItemKind::OPERATOR,
        25 => CompletionItemKind::TYPE_PARAMETER,
        _ => return None,
    })
}

/// Wire diagnostics → editor `lsp_types::Diagnostic`s, sorted by start
/// position (`DiagnosticSet` appends in order). Severity numbers follow the
/// Electron `severityToCm` mapping: 1 error, 2 warning, 4 hint, default info.
pub fn editor_diagnostics(text: &Rope, wire: &[WireDiagnostic]) -> Vec<lsp_types::Diagnostic> {
    let mut mapped: Vec<lsp_types::Diagnostic> = wire
        .iter()
        .map(|diagnostic| lsp_types::Diagnostic {
            range: editor_range(text, &diagnostic.range),
            severity: Some(match diagnostic.severity.0 {
                1 => lsp_types::DiagnosticSeverity::ERROR,
                2 => lsp_types::DiagnosticSeverity::WARNING,
                4 => lsp_types::DiagnosticSeverity::HINT,
                _ => lsp_types::DiagnosticSeverity::INFORMATION,
            }),
            code: diagnostic
                .code
                .clone()
                .flatten()
                .map(|code| NumberOrString::String(code.0)),
            source: diagnostic.source.clone().flatten().map(|source| source.0),
            message: diagnostic.message.0.clone(),
            ..Default::default()
        })
        .collect();
    mapped.sort_by_key(|diagnostic| diagnostic.range.start);
    mapped
}

/// Characters percent-encoded inside a `file://` URI path: everything the
/// RFC 3986 path grammar doesn't allow raw. `/` stays literal.
const FILE_URI_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// `file://` URI for a workspace-relative path.
pub fn file_uri(cwd: &str, relative_path: &str) -> Option<lsp_types::Uri> {
    let absolute = format!("{}/{}", cwd.trim_end_matches('/'), relative_path);
    let encoded = utf8_percent_encode(&absolute, FILE_URI_SET).to_string();
    format!("file://{encoded}").parse().ok()
}

/// Workspace-relative path for a `file://` URI, or None when the target lies
/// outside `cwd` (dropped, matching the Electron editor).
pub fn relative_path_from_uri(cwd: &str, uri: &lsp_types::Uri) -> Option<String> {
    if uri.scheme().is_some_and(|scheme| scheme.as_str() != "file") {
        return None;
    }
    let path = percent_decode_str(uri.path().as_str())
        .decode_utf8()
        .ok()?
        .into_owned();
    let root = format!("{}/", cwd.trim_end_matches('/'));
    path.strip_prefix(&root)
        .map(|relative| relative.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::positions::wire_to_offset;

    fn wire_range(
        start_line: i64,
        start_character: i64,
        end_line: i64,
        end_character: i64,
    ) -> WireRange {
        WireRange {
            end: WireLspPosition {
                character: NonNegativeInt(end_character),
                line: NonNegativeInt(end_line),
            },
            start: WireLspPosition {
                character: NonNegativeInt(start_character),
                line: NonNegativeInt(start_line),
            },
        }
    }

    #[test]
    fn editor_position_uses_char_columns() {
        // 🚀 is 2 UTF-16 units but 1 char: wire column 3 (after the emoji)
        // must land on editor char column 2.
        let text = Rope::from("a🚀b");
        let offset = wire_to_offset(
            &text,
            WirePosition {
                line: 0,
                character: 3,
            },
        );
        assert_eq!(
            editor_position(&text, offset),
            lsp_types::Position::new(0, 2)
        );
    }

    #[test]
    fn editor_range_normalizes_utf16_to_chars() {
        let text = Rope::from("x = 🚀🚀\ny");
        // Wire columns 4..8 span both emojis (2 units each).
        let range = editor_range(&text, &wire_range(0, 4, 0, 8));
        assert_eq!(range.start, lsp_types::Position::new(0, 4));
        assert_eq!(range.end, lsp_types::Position::new(0, 6));
    }

    #[test]
    fn completion_honors_filter_text_sort_text_and_unicode_identifiers() {
        assert_eq!(completion_anchor("obj.café", false), Some(4));
        assert_eq!(completion_anchor("obj.cafe\u{301}", false), Some(4));
        assert_eq!(completion_anchor("obj.世界", false), Some(4));
        assert_eq!(completion_anchor("obj.", false), Some(4));
        let items = [
            serde_json::json!({"label":"First display", "filterText":"Button", "sortText":"02"}),
            serde_json::json!({"label":"Second display", "filterText":"Button", "sortText":"01"}),
            serde_json::json!({"label":"Button", "filterText":"unrelated", "sortText":"00"}),
        ]
        .into_iter()
        .map(|value| serde_json::from_value(value).unwrap())
        .collect();
        let items = ranked_completions(
            items,
            &Rope::from("But"),
            lsp_types::Position::new(0, 0),
            lsp_types::Position::new(0, 3),
            "But",
        );
        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            ["Second display", "First display"]
        );
    }

    #[test]
    fn query_rank_ranks_prefixes_over_subsequences() {
        // The `message.to` case: string members narrow to the `to*` ones.
        assert_eq!(query_rank("toString", "to"), Some(0));
        assert_eq!(query_rank("ToString", "to"), Some(1));
        assert_eq!(query_rank("localeCompare", "to"), None);
        assert_eq!(query_rank("at", "to"), None);
        // Subsequence still matches, as CodeMirror's fuzzy filter does.
        assert_eq!(query_rank("toLocaleUpperCase", "tlu"), Some(2));
        // An empty query (a bare trigger character) keeps every item.
        assert_eq!(query_rank("charAt", ""), Some(0));
    }

    #[test]
    fn fingerprint_matches_across_rope_and_str() {
        // didOpen fingerprints a `String`, every later flush fingerprints the
        // editor's `Rope`: the two must agree or the first flush would resend
        // an unchanged document.
        let text = "export function greet(name: string) {\n  return name;\n}\n";
        assert_eq!(
            TextFingerprint::of_str(text),
            TextFingerprint::of_rope(&Rope::from(text))
        );
        assert_ne!(
            TextFingerprint::of_str(text),
            TextFingerprint::of_rope(&Rope::from(format!("{text}// edited\n")))
        );
    }

    #[test]
    fn file_uri_round_trip() {
        let cwd = "/tmp/work space";
        let uri = file_uri(cwd, "src/lib.rs").expect("uri parses");
        assert_eq!(
            relative_path_from_uri(cwd, &uri).as_deref(),
            Some("src/lib.rs")
        );
    }

    #[test]
    fn uri_outside_workspace_is_dropped() {
        let uri = file_uri("/other/root", "src/lib.rs").expect("uri parses");
        assert_eq!(relative_path_from_uri("/tmp/workspace", &uri), None);
    }

    #[test]
    fn completion_item_synthesizes_text_edit_at_anchor() {
        let text = Rope::from("con");
        let item = completion_item(
            WireCompletionItem {
                additional_text_edits: None,
                detail: None,
                documentation: None,
                filter_text: None,
                insert_text: Some(Some(TrimmedNonEmptyString("console".into()))),
                kind: Some(Some(NonNegativeInt(6))),
                label: TrimmedNonEmptyString("console".into()),
                range: None,
                resolve_data: Some(Some(TrimmedNonEmptyString("{}".into()))),
                sort_text: None,
            },
            &text,
            lsp_types::Position::new(0, 0),
            lsp_types::Position::new(0, 3),
        );
        let Some(CompletionTextEdit::Edit(edit)) = item.text_edit else {
            panic!("expected a plain text edit");
        };
        assert_eq!(edit.new_text, "console");
        assert_eq!(edit.range.start, lsp_types::Position::new(0, 0));
        assert_eq!(edit.range.end, lsp_types::Position::new(0, 3));
        assert_eq!(item.kind, Some(CompletionItemKind::VARIABLE));
        assert_eq!(item.data, Some(serde_json::Value::String("{}".into())));
    }

    #[test]
    fn diagnostics_sort_and_map_severity() {
        let text = Rope::from("line one\nline two\n");
        let diagnostic = |line: i64, severity: i64, message: &str| WireDiagnostic {
            code: None,
            message: TrimmedNonEmptyString(message.into()),
            range: wire_range(line, 0, line, 4),
            severity: NonNegativeInt(severity),
            source: None,
        };
        let mapped = editor_diagnostics(
            &text,
            &[
                diagnostic(1, 2, "warn"),
                diagnostic(0, 1, "error"),
                diagnostic(0, 9, "odd"),
            ],
        );
        assert_eq!(mapped.len(), 3);
        assert_eq!(mapped[0].message, "error");
        assert_eq!(
            mapped[0].severity,
            Some(lsp_types::DiagnosticSeverity::ERROR)
        );
        assert_eq!(
            mapped[1].severity,
            Some(lsp_types::DiagnosticSeverity::INFORMATION)
        );
        assert_eq!(mapped[2].message, "warn");
    }
}
