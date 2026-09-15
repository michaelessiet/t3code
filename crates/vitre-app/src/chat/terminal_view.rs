//! One terminal pane: Electron's `TerminalViewport` (xterm.js instance) as a
//! GPUI entity around an `alacritty_terminal::Term` used PTY-less — the Node
//! sidecar owns the PTY, this view folds the `terminal.attach` stream into a
//! buffer (vitre-state reducer), replays it through the VT parser, paints the
//! grid, and turns keystrokes into `terminal.write` calls.
//!
//! The buffer replay protocol is the Electron one exactly (§2.2 of the port
//! spec): on every version change, delta-append when the new buffer extends
//! the old one, else `ESC c` full reset + rewrite; system messages are
//! written locally as `\r\n[terminal] …\r\n`; `closed`/`exited` transitions
//! fire the exit latch once and bubble to the drawer, which closes the tab.
//!
//! Mouse selection drives Electron's `readSelectionAction` flow: drag (or
//! double/triple-click) selects via alacritty's selection model, and on
//! release a small floating menu offers "Add to chat" / "Copy" — Electron
//! shows a NATIVE context menu here; the in-window popup is a Vitre
//! deviation. "Add to chat" emits a [`TerminalContextSelection`] with xterm's
//! line math (`lineStart = buffer row + 1`, `lineEnd` from the normalized
//! text's line count).
//!
//! Deliberate Vitre deviations (matrix-noted): no link detection, no IME
//! composition (plain `key_char` input), no cursor blink, no mouse-mode
//! reporting, and no drag auto-scroll past the viewport edge. Keyboard
//! passthrough arbitration is gpui's own binding dispatch: chords bound at
//! the app level never reach `on_key_down`, which is Electron's
//! `isTerminalPassthroughShortcut` contract for free.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use alacritty_terminal::event::{Event as AlacEvent, EventListener};
use alacritty_terminal::grid::{Dimensions as _, Scroll};
use alacritty_terminal::index::{Column, Line as AlacLine, Point as AlacPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionRange, SelectionType};
use alacritty_terminal::term::cell::Flags as CellFlags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode, viewport_to_point};
use alacritty_terminal::vte::ansi::{
    Color as AnsiColor, CursorShape, NamedColor, Processor, Rgb as AnsiRgb,
};
use gpui::{
    Bounds, ClipboardItem, Context, EventEmitter, FocusHandle, Focusable, FontStyle, FontWeight,
    Hsla, KeyDownEvent, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, ScrollWheelEvent, SharedString, Size, StrikethroughStyle, TextAlign, TextRun,
    UnderlineStyle, Window, canvas, div, fill, outline, point, prelude::*, px, size,
};
use gpui_component::{ActiveTheme as _, v_flex};
use vitre_client::EnvironmentClient;
use vitre_contracts::methods::{TerminalAttach, TerminalResize, TerminalWrite};
use vitre_contracts::{TerminalAttachInput, TerminalResizeInput, TerminalWriteInput};
use vitre_rpc::TypedStreamEvent;
use vitre_state::terminal::{TerminalBufferState, TerminalClientStatus, apply_attach_event};
use vitre_state::terminal_context::{TerminalContextSelection, normalize_terminal_context_text};

use super::tnes;

/// Payload limit on `terminal.write` (`packages/contracts/src/terminal.ts`).
const MAX_WRITE_CHARS: usize = 65_536;

/// Server-side `DEFAULT_OPEN_COLS/ROWS`, used until the first layout runs.
const DEFAULT_COLS: usize = 120;
const DEFAULT_ROWS: usize = 30;

/// Electron's xterm options: `fontSize: 12`, `scrollback: 5000`.
const FONT_SIZE: f32 = 12.;
const SCROLLBACK_LINES: usize = 5_000;

const WRITE_FAILED: &str = "Terminal write failed";

/// A server-completed attach stream must not resubscribe in a hot loop.
const RESUBSCRIBE_AFTER_COMPLETION: Duration = Duration::from_secs(2);

pub enum TerminalViewEvent {
    OpenLink(super::terminal_links::TerminalLink),
    /// The session reported `closed`/`exited` (Electron's `onSessionExited`);
    /// the drawer responds by running its close flow for this tab.
    SessionExited,
    /// The selection menu's "Add to chat": the drawer bubbles this to the
    /// ChatApp's pending terminal contexts (Electron's `onAddTerminalContext`).
    AddToChat(TerminalContextSelection),
}

/// Everything that, when changed, makes Electron destroy and recreate the
/// xterm instance (the mount-effect deps: cwd, env signature, worktree,
/// ids). The drawer compares launches and recreates the view on mismatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TerminalLaunch {
    pub thread_id: String,
    pub terminal_id: String,
    pub cwd: String,
    /// `None` = omit from the wire (attached-root launches), `Some(None)` =
    /// wire `null`, `Some(Some(path))` = a worktree checkout.
    pub worktree_path: Option<Option<String>>,
    /// Canonical (sorted) — a different order would silently restart the
    /// shell server-side (port-spec gotcha #3).
    pub env: BTreeMap<String, String>,
}

#[derive(Clone)]
struct EventProxy(Rc<RefCell<Vec<AlacEvent>>>);

impl EventListener for EventProxy {
    fn send_event(&self, event: AlacEvent) {
        self.0.borrow_mut().push(event);
    }
}

pub struct TerminalView {
    client: Arc<EnvironmentClient>,
    launch: TerminalLaunch,
    term: Term<EventProxy>,
    processor: Processor,
    term_events: Rc<RefCell<Vec<AlacEvent>>>,
    /// Folded attach-stream state (the source of truth the Term replays).
    buffer: TerminalBufferState,
    /// Electron's `previousSessionRef`.
    prev: TerminalBufferState,
    /// `hasHandledExitRef`: one exit notification per closed/exited episode.
    exit_handled: bool,
    /// Set by the drawer for the active pane; first buffer arrival focuses.
    autofocus: bool,
    wants_focus: bool,
    focus_handle: FocusHandle,
    cell_size: Option<Size<Pixels>>,
    /// Dims already applied to the Term (and requested from the server).
    dims: (usize, usize),
    resize_in_flight: bool,
    pending_resize: Option<(i64, i64)>,
    scroll_accum: f32,
    /// The drawer-supplied tab label ("Add to chat" contexts carry it).
    label: String,
    /// A left-drag selection is in progress.
    selecting: bool,
    /// "Add to chat"/"Copy" popup origin, relative to the grid's bounds.
    selection_menu: Option<Point<Pixels>>,
    /// Grid bounds from the last layout pass (mouse → cell hit testing).
    last_bounds: Option<Bounds<Pixels>>,
    _attach_task: gpui::Task<()>,
}

impl EventEmitter<TerminalViewEvent> for TerminalView {}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn grid_link_at(
    term: &Term<EventProxy>,
    point: AlacPoint,
    cwd: &str,
) -> Option<super::terminal_links::TerminalLink> {
    let grid = term.grid();
    if let Some(link) = grid[point].hyperlink() {
        return super::terminal_links::parse_link(link.uri(), cwd);
    }
    let last = Column(grid.columns().saturating_sub(1));
    let mut start = point.line;
    while start > grid.topmost_line()
        && point.line.0 - start.0 < 32
        && grid[AlacLine(start.0 - 1)][last]
            .flags
            .contains(CellFlags::WRAPLINE)
    {
        start -= 1;
    }
    let mut text = String::new();
    let mut byte = 0;
    let mut line = start;
    loop {
        for col in 0..grid.columns() {
            let cell = &grid[line][Column(col)];
            if cell
                .flags
                .intersects(CellFlags::WIDE_CHAR_SPACER | CellFlags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            if line == point.line && col <= point.column.0 {
                byte = text.len();
            }
            text.push(cell.c);
            if let Some(extra) = cell.zerowidth() {
                text.extend(extra);
            }
        }
        if line >= grid.bottommost_line()
            || line.0 - start.0 >= 64
            || !grid[line][last].flags.contains(CellFlags::WRAPLINE)
        {
            break;
        }
        line += 1;
    }
    super::terminal_links::link_at(&text, byte, cwd)
}

impl TerminalView {
    pub(super) fn new(
        client: Arc<EnvironmentClient>,
        launch: TerminalLaunch,
        cx: &mut Context<Self>,
    ) -> Self {
        let term_events = Rc::new(RefCell::new(Vec::new()));
        let term = Term::new(
            TermConfig {
                scrolling_history: SCROLLBACK_LINES,
                ..TermConfig::default()
            },
            &TermSize::new(DEFAULT_COLS, DEFAULT_ROWS),
            EventProxy(term_events.clone()),
        );
        let attach_task = Self::spawn_attach_loop(client.clone(), launch.clone(), cx);
        Self {
            client,
            launch,
            term,
            processor: Processor::new(),
            term_events,
            buffer: TerminalBufferState::default(),
            prev: TerminalBufferState::default(),
            exit_handled: false,
            autofocus: false,
            wants_focus: false,
            focus_handle: cx.focus_handle(),
            cell_size: None,
            dims: (DEFAULT_COLS, DEFAULT_ROWS),
            resize_in_flight: false,
            pending_resize: None,
            scroll_accum: 0.,
            label: String::new(),
            selecting: false,
            selection_menu: None,
            last_bounds: None,
            _attach_task: attach_task,
        }
    }

    pub(super) fn launch(&self) -> &TerminalLaunch {
        &self.launch
    }

    /// The drawer marks the active pane; also arms focus when the drawer
    /// wants it (activation, split, re-show).
    pub(super) fn set_active(&mut self, active: bool) {
        self.autofocus = active;
    }

    /// The drawer's tab label (server label or `Terminal N`), refreshed every
    /// render so "Add to chat" snapshots the label the tab shows.
    pub(super) fn set_label(&mut self, label: String) {
        self.label = label;
    }

    pub(super) fn request_focus(&mut self, cx: &mut Context<Self>) {
        self.wants_focus = true;
        cx.notify();
    }

    // ---- attach stream ----------------------------------------------------

    fn attach_payload(launch: &TerminalLaunch) -> TerminalAttachInput {
        let env: serde_json::Map<String, serde_json::Value> = launch
            .env
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();
        TerminalAttachInput {
            cols: None,
            rows: None,
            // cwd present ⇒ attach auto-opens a missing session server-side.
            cwd: Some(Some(tnes(&launch.cwd))),
            env: (!env.is_empty()).then_some(Some(env)),
            restart_if_not_running: None,
            terminal_id: tnes(&launch.terminal_id),
            thread_id: tnes(&launch.thread_id),
            worktree_path: launch
                .worktree_path
                .as_ref()
                .map(|path| Some(path.as_ref().map(tnes))),
        }
    }

    /// Durable attach loop, the same session-watch shape as the diff panel's
    /// vcs status stream: every new session re-issues `terminal.attach` and
    /// the fresh snapshot resets the scanned buffer (there is no resume
    /// cursor by design).
    fn spawn_attach_loop(
        client: Arc<EnvironmentClient>,
        launch: TerminalLaunch,
        cx: &mut Context<Self>,
    ) -> gpui::Task<()> {
        let payload = Self::attach_payload(&launch);
        cx.spawn(async move |this, cx| {
            let mut sessions = client.sessions();
            loop {
                let Some(handle) = sessions.borrow_and_update().clone() else {
                    if sessions.changed().await.is_err() {
                        return;
                    }
                    continue;
                };
                let Ok(mut subscription) =
                    handle.session.subscribe_typed::<TerminalAttach>(&payload)
                else {
                    if sessions.changed().await.is_err() {
                        return;
                    }
                    continue;
                };
                let completed = loop {
                    tokio::select! {
                        event = subscription.next() => match event {
                            Some(TypedStreamEvent::Values(events)) => {
                                if this
                                    .update(cx, |view, cx| view.apply_attach_batch(&events, cx))
                                    .is_err()
                                {
                                    return;
                                }
                                if subscription.ack().is_err() {
                                    break false;
                                }
                            }
                            Some(TypedStreamEvent::Completed(result)) => {
                                // Non-transport failures surface in-terminal,
                                // like the atom error → status:"error" path.
                                if let Err(error) = &result
                                    && !error.is_transport()
                                    && this
                                        .update(cx, |view, cx| {
                                            view.buffer.status = TerminalClientStatus::Error;
                                            view.buffer.error =
                                                Some("The environment request failed.".into());
                                            view.buffer.version += 1;
                                            view.reconcile(cx);
                                        })
                                        .is_err()
                                {
                                    return;
                                }
                                break result.is_ok();
                            }
                            None => break false,
                        },
                        changed = sessions.changed() => {
                            if changed.is_err() {
                                return;
                            }
                            let replaced = sessions
                                .borrow()
                                .as_ref()
                                .is_none_or(|current| current.generation != handle.generation);
                            if replaced {
                                break false;
                            }
                        }
                    }
                };
                if completed {
                    cx.background_executor()
                        .timer(RESUBSCRIBE_AFTER_COMPLETION)
                        .await;
                }
            }
        })
    }

    fn apply_attach_batch(
        &mut self,
        events: &[vitre_contracts::TerminalAttachStreamEvent],
        cx: &mut Context<Self>,
    ) {
        for event in events {
            apply_attach_event(&mut self.buffer, event);
        }
        self.reconcile(cx);
    }

    /// §2.2: apply the folded buffer to the Term when the version moved.
    fn reconcile(&mut self, cx: &mut Context<Self>) {
        let current = self.buffer.clone();
        if current.version == self.prev.version {
            return;
        }
        let prev = std::mem::take(&mut self.prev);
        if current.buffer.len() >= prev.buffer.len() && current.buffer.starts_with(&prev.buffer) {
            let delta = current.buffer[prev.buffer.len()..].to_string();
            if !delta.is_empty() {
                self.advance(delta.as_bytes(), cx);
            }
        } else {
            // RIS: full reset (also drops scrollback, like xterm's reset()).
            self.advance(b"\x1bc", cx);
            if !current.buffer.is_empty() {
                let full = current.buffer.clone();
                self.advance(full.as_bytes(), cx);
            }
        }
        if let Some(error) = &current.error
            && current.error != prev.error
        {
            let message = error.clone();
            self.system_message(&message, cx);
        }
        match current.status {
            TerminalClientStatus::Running => self.exit_handled = false,
            TerminalClientStatus::Closed | TerminalClientStatus::Exited
                if current.status != prev.status && !self.exit_handled =>
            {
                self.exit_handled = true;
                let copy = if current.status == TerminalClientStatus::Closed {
                    "Terminal closed"
                } else {
                    "Process exited"
                };
                self.system_message(copy, cx);
                cx.emit(TerminalViewEvent::SessionExited);
            }
            _ => {}
        }
        if prev.version == 0 && self.autofocus {
            self.wants_focus = true;
        }
        self.prev = current;
        cx.notify();
    }

    /// Feed bytes through the VT parser, then service the events the Term
    /// emitted (query responses, OSC 52 copies).
    fn advance(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.processor.advance(&mut self.term, bytes);
        let events: Vec<AlacEvent> = self.term_events.borrow_mut().drain(..).collect();
        for event in events {
            match event {
                // DA/DSR/etc. responses xterm would send via onData.
                AlacEvent::PtyWrite(text) => self.send_input(text, WRITE_FAILED, cx),
                AlacEvent::ClipboardStore(_, text) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
                AlacEvent::ColorRequest(index, format) => {
                    let rgb = self.osc_color(index, cx.theme().is_dark());
                    self.send_input(format(rgb), WRITE_FAILED, cx);
                }
                _ => {}
            }
        }
    }

    fn system_message(&mut self, message: &str, cx: &mut Context<Self>) {
        let bytes = format!("\r\n[terminal] {message}\r\n");
        self.advance(bytes.as_bytes(), cx);
    }

    /// The color a `ColorRequest` (OSC 4/10/11) should report: runtime
    /// overrides first, then the theme palette.
    fn osc_color(&self, index: usize, dark: bool) -> AnsiRgb {
        if let Some(rgb) = self.term.colors()[index] {
            return rgb;
        }
        let palette = TerminalPalette::of(dark);
        let hsla = match index {
            256 => palette.foreground,
            257 => palette.background,
            258 => palette.cursor,
            _ if index < 256 => palette.indexed(index as u8),
            _ => palette.foreground,
        };
        hsla_to_rgb(hsla)
    }

    // ---- outbound RPCs ----------------------------------------------------

    /// `terminal.write` with `reportFailure: false` semantics: failures land
    /// as in-terminal system messages, never toasts. Oversized payloads are
    /// chunked (Electron lets them fail schema validation; chunking is the
    /// port-spec's sanctioned improvement).
    fn send_input(&self, data: String, fallback: &'static str, cx: &mut Context<Self>) {
        if data.is_empty() {
            return;
        }
        let client = self.client.clone();
        let thread_id = self.launch.thread_id.clone();
        let terminal_id = self.launch.terminal_id.clone();
        cx.spawn(async move |this, cx| {
            let mut rest = data.as_str();
            while !rest.is_empty() {
                let mut end = rest.len().min(MAX_WRITE_CHARS);
                while !rest.is_char_boundary(end) {
                    end -= 1;
                }
                let payload = TerminalWriteInput {
                    data: rest[..end].to_string(),
                    terminal_id: tnes(&terminal_id),
                    thread_id: tnes(&thread_id),
                };
                if client.call::<TerminalWrite>(&payload).await.is_err() {
                    let _ = this.update(cx, |view, cx| view.system_message(fallback, cx));
                    return;
                }
                rest = &rest[end..];
            }
        })
        .detach();
    }

    /// Latest-wins resize: one in flight, newer requests coalesce.
    fn send_resize(&mut self, cols: i64, rows: i64, cx: &mut Context<Self>) {
        self.pending_resize = Some((cols, rows));
        if self.resize_in_flight {
            return;
        }
        self.resize_in_flight = true;
        self.flush_resize(cx);
    }

    fn flush_resize(&mut self, cx: &mut Context<Self>) {
        let Some((cols, rows)) = self.pending_resize.take() else {
            self.resize_in_flight = false;
            return;
        };
        let client = self.client.clone();
        let payload = TerminalResizeInput {
            cols,
            rows,
            terminal_id: tnes(&self.launch.terminal_id),
            thread_id: tnes(&self.launch.thread_id),
        };
        cx.spawn(async move |this, cx| {
            // Failures are silently ignored (Electron sends with
            // reportFailure: false and never observes the result).
            let _ = client.call::<TerminalResize>(&payload).await;
            let _ = this.update(cx, |view, cx| view.flush_resize(cx));
        })
        .detach();
    }

    // ---- input ------------------------------------------------------------

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        // Local viewport scrolling (xterm handles shift+page keys itself).
        if keystroke.modifiers.shift {
            let scroll = match keystroke.key.as_str() {
                "pageup" => Some(Scroll::PageUp),
                "pagedown" => Some(Scroll::PageDown),
                "home" => Some(Scroll::Top),
                "end" => Some(Scroll::Bottom),
                _ => None,
            };
            if let Some(scroll) = scroll
                && !self.term.mode().contains(TermMode::ALT_SCREEN)
            {
                cx.stop_propagation();
                self.term.scroll_display(scroll);
                cx.notify();
                return;
            }
        }
        match encode_keystroke(keystroke, *self.term.mode(), cfg!(target_os = "macos")) {
            Some(TerminalInput::Write { data, fallback }) => {
                cx.stop_propagation();
                // xterm's scrollOnUserInput: typing snaps to the bottom.
                self.term.scroll_display(Scroll::Bottom);
                self.send_input(data, fallback, cx);
                cx.notify();
            }
            Some(TerminalInput::Paste) => {
                cx.stop_propagation();
                self.paste(window, cx);
            }
            None => {}
        }
    }

    fn paste(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        // xterm's prepareTextForTerminal: newlines become carriage returns.
        let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
        let data = if self.term.mode().contains(TermMode::BRACKETED_PASTE) {
            format!("\x1b[200~{}\x1b[201~", normalized.replace('\x1b', ""))
        } else {
            normalized
        };
        self.term.scroll_display(Scroll::Bottom);
        self.send_input(data, WRITE_FAILED, cx);
        cx.notify();
    }

    fn on_scroll_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let line_height = self
            .cell_size
            .map(|cell| cell.height)
            .unwrap_or(px(FONT_SIZE * 1.25));
        let delta_y = event.delta.pixel_delta(line_height).y / line_height;
        self.scroll_accum += delta_y;
        let lines = self.scroll_accum.trunc() as i32;
        if lines == 0 {
            return;
        }
        self.scroll_accum -= lines as f32;
        if self.term.mode().contains(TermMode::ALT_SCREEN) {
            // Full-screen apps get arrow keys (alacritty's alt-screen wheel).
            let up = lines > 0;
            let seq = arrow_sequence(up, self.term.mode().contains(TermMode::APP_CURSOR));
            self.send_input(seq.repeat(lines.unsigned_abs() as usize), WRITE_FAILED, cx);
        } else {
            self.term.scroll_display(Scroll::Delta(lines));
        }
        cx.notify();
    }

    // ---- selection ---------------------------------------------------------

    /// Window position → buffer point + cell side, via the last grid bounds.
    fn grid_point(&self, position: Point<Pixels>) -> Option<(AlacPoint, Side)> {
        let bounds = self.last_bounds?;
        let cell = self.cell_size?;
        let (cols, rows) = self.dims;
        let x = f32::from(position.x - bounds.origin.x).max(0.);
        let y = f32::from(position.y - bounds.origin.y).max(0.);
        let col_exact = x / f32::from(cell.width);
        let col = (col_exact.floor() as usize).min(cols.saturating_sub(1));
        let row = ((y / f32::from(cell.height)).floor() as usize).min(rows.saturating_sub(1));
        let side = if col_exact.fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        let display_offset = self.term.grid().display_offset();
        let point = viewport_to_point(display_offset, AlacPoint::new(row, Column(col)));
        Some((point, side))
    }

    fn link_at_point(&self, point: AlacPoint) -> Option<super::terminal_links::TerminalLink> {
        grid_link_at(&self.term, point, &self.launch.cwd)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        self.selection_menu = None;
        let Some((point, side)) = self.grid_point(event.position) else {
            return;
        };
        // OSC-8 and logical wrapped lines; never mutate selection to detect a link.
        if (event.modifiers.platform || event.modifiers.control)
            && let Some(link) = self.link_at_point(point)
        {
            cx.emit(TerminalViewEvent::OpenLink(link));
            self.selecting = false;
            cx.notify();
            return;
        }
        // xterm: click starts, double-click selects the word, triple the line.
        let ty = match event.click_count {
            1 => SelectionType::Simple,
            2 => SelectionType::Semantic,
            _ => SelectionType::Lines,
        };
        self.term.selection = Some(Selection::new(ty, point, side));
        self.selecting = true;
        cx.notify();
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.selecting || event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some((point, side)) = self.grid_point(event.position) else {
            return;
        };
        if let Some(selection) = self.term.selection.as_mut() {
            selection.update(point, side);
            cx.notify();
        }
    }

    /// Selection release: Electron's xterm fires `readSelectionAction` and
    /// pops the native Add to chat / Copy menu near the pointer.
    fn on_mouse_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.selecting) {
            return;
        }
        let has_selection = self
            .selection_text()
            .is_some_and(|text| !normalize_terminal_context_text(&text).is_empty());
        if !has_selection {
            self.term.selection = None;
            cx.notify();
            return;
        }
        let bounds = self.last_bounds.unwrap_or_default();
        // Near the pointer (+4px like Electron), clamped into the grid.
        let menu = point(
            (event.position.x - bounds.origin.x + px(4.))
                .max(px(0.))
                .min((bounds.size.width - px(150.)).max(px(0.))),
            (event.position.y - bounds.origin.y + px(4.))
                .max(px(0.))
                .min((bounds.size.height - px(64.)).max(px(0.))),
        );
        self.selection_menu = Some(menu);
        cx.notify();
    }

    fn selection_text(&self) -> Option<String> {
        self.term.selection.as_ref()?.to_range(&self.term)?;
        self.term.selection_to_string()
    }

    fn dismiss_selection_menu(&mut self, cx: &mut Context<Self>) {
        self.selection_menu = None;
        cx.notify();
    }

    /// "Copy": clipboard write; the selection stays (xterm keeps it).
    fn copy_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.selection_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        self.dismiss_selection_menu(cx);
    }

    /// "Add to chat": Electron's `readSelectionAction` payload — buffer-based
    /// 1-based `lineStart`, `lineEnd` from the normalized text's line count —
    /// then clear the selection and refocus.
    fn add_selection_to_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let payload = self.selection_text().and_then(|raw| {
            let range = self.term.selection.as_ref()?.to_range(&self.term)?;
            let text = normalize_terminal_context_text(&raw);
            if text.is_empty() {
                return None;
            }
            let history = self.term.grid().history_size() as i32;
            let line_start = (range.start.line.0 + history).max(0) as u32 + 1;
            let line_end = line_start + text.split('\n').count() as u32 - 1;
            Some(TerminalContextSelection {
                terminal_id: self.launch.terminal_id.clone(),
                terminal_label: self.label.clone(),
                line_start,
                line_end,
                text,
            })
        });
        if let Some(selection) = payload {
            cx.emit(TerminalViewEvent::AddToChat(selection));
        }
        self.term.selection = None;
        self.dismiss_selection_menu(cx);
        window.focus(&self.focus_handle, cx);
    }

    /// The floating Add to chat / Copy popup (Electron uses a native context
    /// menu here — the in-window popup is the Vitre stand-in).
    fn render_selection_menu(
        &self,
        origin: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let item = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .px_2()
                .py_1()
                .rounded(px(4.))
                .text_size(px(12.))
                .text_color(cx.theme().popover_foreground)
                .cursor_pointer()
                .hover(|style| style.bg(cx.theme().accent))
                .child(label)
        };
        v_flex()
            .id("terminal-selection-menu")
            .occlude()
            .absolute()
            .left(origin.x)
            .top(origin.y)
            .p_1()
            .gap_0p5()
            .rounded(px(6.))
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .shadow_md()
            .child(
                item("terminal-selection-add", "Add to chat").on_click(cx.listener(
                    |this, _, window, cx| {
                        cx.stop_propagation();
                        this.add_selection_to_chat(window, cx);
                    },
                )),
            )
            .child(
                item("terminal-selection-copy", "Copy").on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.copy_selection(cx);
                })),
            )
    }

    // ---- layout & paint ---------------------------------------------------

    fn mono_font(&self, cx: &Context<Self>) -> gpui::Font {
        gpui::font(cx.theme().mono_font_family.clone())
    }

    fn measure_cell(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Size<Pixels> {
        if let Some(cell) = self.cell_size {
            return cell;
        }
        let font = self.mono_font(cx);
        let font_size = px(FONT_SIZE);
        let width = window
            .text_system()
            .resolve_font(&font)
            .pipe(|font_id| window.text_system().advance(font_id, font_size, 'm'))
            .map(|advance| advance.width)
            .unwrap_or(px(FONT_SIZE * 0.6));
        let cell = size(width, px((FONT_SIZE * 1.25).round()));
        self.cell_size = Some(cell);
        cell
    }

    /// Canvas prepaint: fit the grid to the bounds (resizing the Term and the
    /// server session on change), then shape every visible row.
    fn layout(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TerminalLayout {
        let cell = self.measure_cell(window, cx);
        self.last_bounds = Some(bounds);
        let cols = ((bounds.size.width / cell.width).floor() as usize).clamp(1, 1000);
        let rows = ((bounds.size.height / cell.height).floor() as usize).clamp(1, 500);
        if (cols, rows) != self.dims && bounds.size.width > px(0.) && bounds.size.height > px(0.) {
            self.dims = (cols, rows);
            self.term.resize(TermSize::new(cols, rows));
            self.send_resize(cols as i64, rows as i64, cx);
        }

        let theme_bg = cx.theme().background;
        let theme_fg = cx.theme().foreground;
        let palette = TerminalPalette::of(cx.theme().is_dark());
        let font = self.mono_font(cx);
        let font_size = px(FONT_SIZE);
        let focused = self.focus_handle.is_focused(window);

        struct StyledChar {
            c: char,
            fg: Hsla,
            bg: Option<Hsla>,
            flags: CellFlags,
        }
        let mut grid_rows: Vec<Vec<StyledChar>> = (0..rows).map(|_| Vec::new()).collect();

        let content = self.term.renderable_content();
        let display_offset = content.display_offset;
        let colors = content.colors;
        let selection_range = content.selection;
        for indexed in content.display_iter {
            if indexed.cell.flags.contains(CellFlags::WIDE_CHAR_SPACER)
                || indexed
                    .cell
                    .flags
                    .contains(CellFlags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let row = indexed.point.line.0 + display_offset as i32;
            if row < 0 || row as usize >= rows {
                continue;
            }
            let cell = &indexed.cell;
            let mut fg = resolve_color(cell.fg, colors, palette, theme_fg, theme_bg, true);
            let mut bg = resolve_color(cell.bg, colors, palette, theme_fg, theme_bg, false);
            if cell.flags.intersects(CellFlags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if cell.flags.intersects(CellFlags::DIM) {
                fg = fg.opacity(0.66);
            }
            if cell.flags.contains(CellFlags::HIDDEN) {
                fg = bg;
            }
            grid_rows[row as usize].push(StyledChar {
                c: cell.c,
                fg,
                bg: (bg != theme_bg).then_some(bg),
                flags: cell.flags,
            });
        }

        // Selection overlay rectangles (painted over cell backgrounds, under
        // glyphs — xterm's selection layer).
        let selection_rects =
            selection_rects(selection_range, rows, cols, display_offset, bounds, cell);

        // Cursor (grid coords; hidden while scrolled out of the viewport).
        let cursor = content.cursor;
        let cursor_row = cursor.point.line.0 + display_offset as i32;
        let cursor_layout = (cursor.shape != CursorShape::Hidden
            && cursor_row >= 0
            && (cursor_row as usize) < rows)
            .then(|| {
                let origin = point(
                    bounds.origin.x + cell.width * cursor.point.column.0 as f32,
                    bounds.origin.y + cell.height * cursor_row as f32,
                );
                let cursor_char = self.term.grid()[cursor.point].c;
                let char_line = (focused && cursor_char != ' ').then(|| {
                    let text: SharedString = cursor_char.to_string().into();
                    let run = TextRun {
                        len: text.len(),
                        font: font.clone(),
                        color: theme_bg,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    window
                        .text_system()
                        .shape_line(text, font_size, &[run], None)
                });
                CursorLayout {
                    bounds: Bounds::new(origin, cell),
                    filled: focused,
                    color: palette.cursor,
                    char_line,
                }
            });

        let mut lines = Vec::with_capacity(rows);
        for (row, chars) in grid_rows.iter().enumerate() {
            // Trim trailing default-background blanks: nothing to paint.
            let mut chars = chars.as_slice();
            while let Some(last) = chars.last() {
                if last.c == ' '
                    && last.bg.is_none()
                    && !last.flags.intersects(CellFlags::UNDERLINE)
                {
                    chars = &chars[..chars.len() - 1];
                } else {
                    break;
                }
            }
            if chars.is_empty() {
                continue;
            }
            let mut text = String::new();
            let mut runs: Vec<TextRun> = Vec::new();
            for styled in chars {
                let mut len_buffer = [0u8; 4];
                let encoded = styled.c.encode_utf8(&mut len_buffer);
                text.push_str(encoded);
                let bold = styled.flags.intersects(CellFlags::BOLD);
                let italic = styled.flags.intersects(CellFlags::ITALIC);
                let underline =
                    styled
                        .flags
                        .intersects(CellFlags::UNDERLINE)
                        .then(|| UnderlineStyle {
                            thickness: px(1.),
                            color: Some(styled.fg),
                            wavy: false,
                        });
                let strikethrough =
                    styled
                        .flags
                        .intersects(CellFlags::STRIKEOUT)
                        .then(|| StrikethroughStyle {
                            thickness: px(1.),
                            color: Some(styled.fg),
                        });
                let mut font = font.clone();
                if bold {
                    font.weight = FontWeight::BOLD;
                }
                if italic {
                    font.style = FontStyle::Italic;
                }
                let matches_last = runs.last().is_some_and(|run| {
                    run.color == styled.fg
                        && run.background_color == styled.bg
                        && run.font == font
                        && run.underline == underline
                        && run.strikethrough == strikethrough
                });
                if matches_last {
                    runs.last_mut().expect("just matched").len += encoded.len();
                } else {
                    runs.push(TextRun {
                        len: encoded.len(),
                        font,
                        color: styled.fg,
                        background_color: styled.bg,
                        underline,
                        strikethrough,
                    });
                }
            }
            let shaped =
                window
                    .text_system()
                    .shape_line(SharedString::from(text), font_size, &runs, None);
            let origin = point(bounds.origin.x, bounds.origin.y + cell.height * row as f32);
            lines.push((shaped, origin));
        }

        TerminalLayout {
            lines,
            cursor: cursor_layout,
            line_height: cell.height,
            selection_rects,
            selection_color: palette.selection,
        }
    }
}

/// Per-row selection rectangles for the visible viewport.
fn selection_rects(
    range: Option<SelectionRange>,
    rows: usize,
    cols: usize,
    display_offset: usize,
    bounds: Bounds<Pixels>,
    cell: Size<Pixels>,
) -> Vec<Bounds<Pixels>> {
    let Some(range) = range else {
        return Vec::new();
    };
    let mut rects = Vec::new();
    for row in 0..rows {
        let line = AlacLine(row as i32 - display_offset as i32);
        if line < range.start.line || line > range.end.line {
            continue;
        }
        let start_col = if range.is_block || line == range.start.line {
            range.start.column.0
        } else {
            0
        };
        let end_col = if range.is_block || line == range.end.line {
            range.end.column.0
        } else {
            cols.saturating_sub(1)
        };
        if end_col < start_col {
            continue;
        }
        let origin = point(
            bounds.origin.x + cell.width * start_col as f32,
            bounds.origin.y + cell.height * row as f32,
        );
        rects.push(Bounds::new(
            origin,
            size(cell.width * (end_col - start_col + 1) as f32, cell.height),
        ));
    }
    rects
}

struct CursorLayout {
    bounds: Bounds<Pixels>,
    filled: bool,
    color: Hsla,
    char_line: Option<gpui::ShapedLine>,
}

struct TerminalLayout {
    lines: Vec<(gpui::ShapedLine, Point<Pixels>)>,
    cursor: Option<CursorLayout>,
    line_height: Pixels,
    selection_rects: Vec<Bounds<Pixels>>,
    selection_color: Hsla,
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.wants_focus) {
            window.focus(&self.focus_handle, cx);
        }
        let entity = cx.entity();
        let grid = canvas(
            {
                let entity = entity.clone();
                move |bounds, window, cx| {
                    entity.update(cx, |view, cx| view.layout(bounds, window, cx))
                }
            },
            move |_bounds, layout: TerminalLayout, window, cx| {
                for (line, origin) in &layout.lines {
                    let _ = line.paint_background(
                        *origin,
                        layout.line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                }
                for rect in &layout.selection_rects {
                    window.paint_quad(fill(*rect, layout.selection_color));
                }
                for (line, origin) in &layout.lines {
                    let _ = line.paint(
                        *origin,
                        layout.line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                }
                if let Some(cursor) = layout.cursor {
                    if cursor.filled {
                        window.paint_quad(fill(cursor.bounds, cursor.color));
                        if let Some(char_line) = &cursor.char_line {
                            let _ = char_line.paint(
                                cursor.bounds.origin,
                                layout.line_height,
                                TextAlign::Left,
                                None,
                                window,
                                cx,
                            );
                        }
                    } else {
                        window.paint_quad(outline(
                            cursor.bounds,
                            cursor.color,
                            gpui::BorderStyle::default(),
                        ));
                    }
                }
            },
        )
        .size_full();

        div()
            .id(SharedString::from(format!(
                "terminal-view-{}",
                self.launch.terminal_id
            )))
            .key_context("Terminal")
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .overflow_hidden()
            .rounded(px(4.))
            .bg(cx.theme().background)
            .font_family(cx.theme().mono_font_family.clone())
            .on_key_down(cx.listener(Self::on_key_down))
            .on_scroll_wheel(cx.listener(|this, event, _, cx| this.on_scroll_wheel(event, cx)))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(|this, event, _, cx| this.on_mouse_move(event, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event, _, cx| this.on_mouse_up(event, cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, event, _, cx| this.on_mouse_up(event, cx)),
            )
            .child(grid)
            .when_some(self.selection_menu, |this, origin| {
                this.child(self.render_selection_menu(origin, cx))
            })
    }
}

/// Tiny `tap` helper so `measure_cell` reads left-to-right.
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

// ---- key encoding ----------------------------------------------------------

pub(super) enum TerminalInput {
    Write {
        data: String,
        fallback: &'static str,
    },
    Paste,
}

fn write(data: impl Into<String>, fallback: &'static str) -> Option<TerminalInput> {
    Some(TerminalInput::Write {
        data: data.into(),
        fallback,
    })
}

fn arrow_sequence(up: bool, app_cursor: bool) -> String {
    let letter = if up { 'A' } else { 'B' };
    if app_cursor {
        format!("\x1bO{letter}")
    } else {
        format!("\x1b[{letter}")
    }
}

/// Keystroke → PTY bytes. Runs only for keystrokes gpui's binding dispatch
/// left unclaimed, which is exactly Electron's passthrough set. Ports the
/// arbitration order of `apps/web/src/keybindings.ts` §2.5: word/line
/// navigation, cmd+backspace delete-to-start, ctrl+l / cmd+k clear — then
/// standard xterm encoding.
fn encode_keystroke(keystroke: &Keystroke, mode: TermMode, mac: bool) -> Option<TerminalInput> {
    let mods = keystroke.modifiers;
    let key = keystroke.key.as_str();

    // 1. Navigation (left/right only; shift disables).
    if (key == "left" || key == "right") && !mods.shift {
        let left = key == "left";
        let word = if left { "\x1bb" } else { "\x1bf" };
        let line = if left { "\x01" } else { "\x05" };
        if mac {
            if mods.alt && !mods.control && !mods.platform {
                return write(word, "Failed to move cursor");
            }
            if mods.platform && !mods.control && !mods.alt {
                return write(line, "Failed to move cursor");
            }
        } else if (mods.control ^ mods.alt) && !mods.platform {
            return write(word, "Failed to move cursor");
        }
    }
    // 2. Delete to line start (mac only).
    if mac && key == "backspace" && mods.platform && !mods.control && !mods.alt && !mods.shift {
        return write("\x15", "Failed to delete terminal input");
    }
    // 3. Clear: writes form-feed to the shell (NOT the terminal.clear RPC).
    if key == "l" && mods.control && !mods.platform && !mods.alt && !mods.shift {
        return write("\x0c", "Failed to clear terminal");
    }
    if mac && key == "k" && mods.platform && !mods.control && !mods.alt && !mods.shift {
        return write("\x0c", "Failed to clear terminal");
    }
    // Paste (xterm's textarea paste event in Electron).
    if key == "v"
        && ((mac && mods.platform && !mods.control && !mods.alt && !mods.shift)
            || (!mac && mods.control && mods.shift && !mods.alt))
    {
        return Some(TerminalInput::Paste);
    }
    // Remaining platform-modified chords are app territory, not PTY bytes.
    if mods.platform {
        return None;
    }

    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    let modifier_code =
        || 1 + mods.shift as u8 + ((mods.alt as u8) << 1) + ((mods.control as u8) << 2);
    match key {
        "enter" => {
            return write(if mods.alt { "\x1b\r" } else { "\r" }, WRITE_FAILED);
        }
        "backspace" => {
            let seq = if mods.alt {
                "\x1b\x7f"
            } else if mods.control {
                "\x08"
            } else {
                "\x7f"
            };
            return write(seq, WRITE_FAILED);
        }
        "tab" => {
            return write(if mods.shift { "\x1b[Z" } else { "\t" }, WRITE_FAILED);
        }
        "escape" => return write("\x1b", WRITE_FAILED),
        "up" | "down" | "left" | "right" => {
            let letter = match key {
                "up" => 'A',
                "down" => 'B',
                "right" => 'C',
                _ => 'D',
            };
            let code = modifier_code();
            let seq = if code > 1 {
                format!("\x1b[1;{code}{letter}")
            } else if app_cursor {
                format!("\x1bO{letter}")
            } else {
                format!("\x1b[{letter}")
            };
            return write(seq, WRITE_FAILED);
        }
        "home" | "end" => {
            let letter = if key == "home" { 'H' } else { 'F' };
            let code = modifier_code();
            let seq = if code > 1 {
                format!("\x1b[1;{code}{letter}")
            } else if app_cursor {
                format!("\x1bO{letter}")
            } else {
                format!("\x1b[{letter}")
            };
            return write(seq, WRITE_FAILED);
        }
        "pageup" => return write("\x1b[5~", WRITE_FAILED),
        "pagedown" => return write("\x1b[6~", WRITE_FAILED),
        "insert" => return write("\x1b[2~", WRITE_FAILED),
        "delete" => {
            let code = modifier_code();
            let seq = if code > 1 {
                format!("\x1b[3;{code}~")
            } else {
                "\x1b[3~".to_string()
            };
            return write(seq, WRITE_FAILED);
        }
        "f1" => return write("\x1bOP", WRITE_FAILED),
        "f2" => return write("\x1bOQ", WRITE_FAILED),
        "f3" => return write("\x1bOR", WRITE_FAILED),
        "f4" => return write("\x1bOS", WRITE_FAILED),
        "f5" => return write("\x1b[15~", WRITE_FAILED),
        "f6" => return write("\x1b[17~", WRITE_FAILED),
        "f7" => return write("\x1b[18~", WRITE_FAILED),
        "f8" => return write("\x1b[19~", WRITE_FAILED),
        "f9" => return write("\x1b[20~", WRITE_FAILED),
        "f10" => return write("\x1b[21~", WRITE_FAILED),
        "f11" => return write("\x1b[23~", WRITE_FAILED),
        "f12" => return write("\x1b[24~", WRITE_FAILED),
        _ => {}
    }

    if mods.control && !mods.alt {
        let byte: Option<u8> = match key {
            k if k.len() == 1 && k.as_bytes()[0].is_ascii_lowercase() => {
                Some(k.as_bytes()[0] - b'a' + 1)
            }
            "space" | "2" | "@" => Some(0x00),
            "[" => Some(0x1b),
            "\\" => Some(0x1c),
            "]" => Some(0x1d),
            "6" | "^" => Some(0x1e),
            "-" | "_" | "7" | "/" => Some(0x1f),
            "8" => Some(0x7f),
            _ => None,
        };
        if let Some(byte) = byte {
            return write((byte as char).to_string(), WRITE_FAILED);
        }
        return None;
    }

    // Non-mac terminals treat alt as meta (ESC prefix); on mac, option
    // composes characters that arrive in key_char (xterm's default
    // macOptionIsMeta: false).
    if mods.alt && !mac {
        if let Some(ch) = &keystroke.key_char
            && !ch.is_empty()
        {
            return write(format!("\x1b{ch}"), WRITE_FAILED);
        }
        return None;
    }

    if let Some(ch) = &keystroke.key_char
        && !ch.is_empty()
    {
        return write(ch.clone(), WRITE_FAILED);
    }
    if key == "space" {
        return write(" ", WRITE_FAILED);
    }
    None
}

// ---- theme ------------------------------------------------------------------

/// §3.6 verbatim palettes (the Electron `buildTerminalTheme` tables).
struct TerminalPalette {
    ansi: [Hsla; 16],
    cursor: Hsla,
    foreground: Hsla,
    background: Hsla,
    /// Electron's xterm `selectionBackground` (semi-transparent overlay).
    selection: Hsla,
}

fn c(hex: u32) -> Hsla {
    gpui::rgb(hex).into()
}

impl TerminalPalette {
    fn of(dark: bool) -> &'static TerminalPalette {
        use std::sync::OnceLock;
        static DARK: OnceLock<TerminalPalette> = OnceLock::new();
        static LIGHT: OnceLock<TerminalPalette> = OnceLock::new();
        if dark {
            DARK.get_or_init(|| TerminalPalette {
                ansi: [
                    c(0x181E26),
                    c(0xFF7A8E),
                    c(0x86E795),
                    c(0xF4CD72),
                    c(0x89BEFF),
                    c(0xD0B0FF),
                    c(0x7CE8ED),
                    c(0xD2DAE6),
                    c(0x6E7888),
                    c(0xFFA8B4),
                    c(0xB0F5BA),
                    c(0xFFE095),
                    c(0xAED2FF),
                    c(0xE5CBFF),
                    c(0xA7F4F7),
                    c(0xF4F7FC),
                ],
                cursor: c(0xB4CBFF),
                foreground: c(0xEDF1F7),
                background: c(0x0E1218),
                // rgba(180, 203, 255, 0.25)
                selection: c(0xB4CBFF).opacity(0.25),
            })
        } else {
            LIGHT.get_or_init(|| TerminalPalette {
                ansi: [
                    c(0x2C3542),
                    c(0xBF4657),
                    c(0x3C7E56),
                    c(0x927023),
                    c(0x4866A3),
                    c(0x845695),
                    c(0x357F8D),
                    c(0xD2D7DF),
                    c(0x707B8C),
                    c(0xD45F70),
                    c(0x55946F),
                    c(0xAD852D),
                    c(0x5B7CC2),
                    c(0x996BAC),
                    c(0x4695A4),
                    c(0xECF0F6),
                ],
                cursor: c(0x26384E),
                foreground: c(0x1C2129),
                background: c(0xFFFFFF),
                // rgba(37, 63, 99, 0.2)
                selection: c(0x253F63).opacity(0.2),
            })
        }
    }

    /// 256-color lookup: 0-15 palette, 16-231 the 6×6×6 cube, 232-255 the
    /// grayscale ramp.
    fn indexed(&self, index: u8) -> Hsla {
        match index {
            0..=15 => self.ansi[index as usize],
            16..=231 => {
                let index = index as u32 - 16;
                let component =
                    |value: u32| -> u32 { if value == 0 { 0 } else { 55 + value * 40 } };
                let r = component(index / 36);
                let g = component((index / 6) % 6);
                let b = component(index % 6);
                c((r << 16) | (g << 8) | b)
            }
            _ => {
                let level = 8 + (index as u32 - 232) * 10;
                c((level << 16) | (level << 8) | level)
            }
        }
    }
}

/// Resolve a cell color: runtime (OSC-set) overrides first, then the theme
/// palette. `is_fg` picks the default for `NamedColor::Foreground/Background`
/// (which map to the app theme, not the palette fallbacks). Bold cells using
/// the base 0-7 palette brighten, xterm's `drawBoldTextInBrightColors`.
fn resolve_color(
    color: AnsiColor,
    runtime: &alacritty_terminal::term::color::Colors,
    palette: &TerminalPalette,
    theme_fg: Hsla,
    theme_bg: Hsla,
    is_fg: bool,
) -> Hsla {
    match color {
        AnsiColor::Spec(rgb) => rgb_to_hsla(rgb),
        AnsiColor::Indexed(index) => runtime[index as usize]
            .map(rgb_to_hsla)
            .unwrap_or_else(|| palette.indexed(index)),
        AnsiColor::Named(named) => {
            if let Some(rgb) = runtime[named] {
                return rgb_to_hsla(rgb);
            }
            match named {
                NamedColor::Foreground | NamedColor::BrightForeground => theme_fg,
                NamedColor::Background => theme_bg,
                NamedColor::Cursor => palette.cursor,
                NamedColor::DimForeground => theme_fg.opacity(0.66),
                _ => {
                    let index = named as usize;
                    if index < 16 {
                        palette.ansi[index]
                    } else if (NamedColor::DimBlack as usize..=NamedColor::DimWhite as usize)
                        .contains(&index)
                    {
                        palette.ansi[index - NamedColor::DimBlack as usize].opacity(0.66)
                    } else if is_fg {
                        theme_fg
                    } else {
                        theme_bg
                    }
                }
            }
        }
    }
}

fn rgb_to_hsla(rgb: AnsiRgb) -> Hsla {
    c(((rgb.r as u32) << 16) | ((rgb.g as u32) << 8) | rgb.b as u32)
}

fn hsla_to_rgb(hsla: Hsla) -> AnsiRgb {
    let rgba: gpui::Rgba = hsla.into();
    AnsiRgb {
        r: (rgba.r * 255.).round() as u8,
        g: (rgba.g * 255.).round() as u8,
        b: (rgba.b * 255.).round() as u8,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn wrapped_vt_links_with_wide_characters_and_osc8() {
        use super::super::terminal_links::TerminalLink;
        use super::*;
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut term = Term::new(
            TermConfig::default(),
            &TermSize::new(12, 6),
            EventProxy(events),
        );
        let mut processor: Processor = Processor::new();
        processor.advance(&mut term, "界 http://localhost:5173/a?q=x#part".as_bytes());
        assert_eq!(
            grid_link_at(&term, AlacPoint::new(AlacLine(1), Column(3)), "/repo"),
            Some(TerminalLink::Url("http://localhost:5173/a?q=x#part".into()))
        );
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut term = Term::new(
            TermConfig::default(),
            &TermSize::new(30, 6),
            EventProxy(events),
        );
        processor.advance(
            &mut term,
            b"\x1b]8;;file:///repo/main.rs:12:4\x07open file\x1b]8;;\x07",
        );
        assert_eq!(
            grid_link_at(&term, AlacPoint::new(AlacLine(0), Column(4)), "/repo"),
            Some(TerminalLink::File {
                path: "/repo/main.rs".into(),
                line: Some(12),
                column: Some(4)
            })
        );
        assert!(term.selection.is_none());
    }

    use super::*;

    fn stroke(key: &str, control: bool, alt: bool, shift: bool, platform: bool) -> Keystroke {
        Keystroke {
            modifiers: gpui::Modifiers {
                control,
                alt,
                shift,
                platform,
                function: false,
            },
            key: key.into(),
            key_char: (key.len() == 1 && !control && !platform).then(|| key.to_string()),
        }
    }

    fn encoded(keystroke: &Keystroke, mode: TermMode, mac: bool) -> Option<String> {
        match encode_keystroke(keystroke, mode, mac) {
            Some(TerminalInput::Write { data, .. }) => Some(data),
            _ => None,
        }
    }

    #[test]
    fn navigation_shortcuts_follow_the_electron_table() {
        let empty = TermMode::empty();
        // mac: option = word, cmd = line.
        assert_eq!(
            encoded(&stroke("left", false, true, false, false), empty, true),
            Some("\x1bb".into())
        );
        assert_eq!(
            encoded(&stroke("right", false, false, false, true), empty, true),
            Some("\x05".into())
        );
        // shift disables the mapping (falls through to modified-arrow CSI).
        assert_eq!(
            encoded(&stroke("left", false, true, true, false), empty, true),
            Some("\x1b[1;4D".into())
        );
        // non-mac: ctrl-only word move.
        assert_eq!(
            encoded(&stroke("left", true, false, false, false), empty, false),
            Some("\x1bb".into())
        );
        // ctrl+alt together is not a navigation chord.
        assert_ne!(
            encoded(&stroke("left", true, true, false, false), empty, false),
            Some("\x1bb".into())
        );
    }

    #[test]
    fn clear_and_delete_shortcuts() {
        let empty = TermMode::empty();
        assert_eq!(
            encoded(&stroke("l", true, false, false, false), empty, true),
            Some("\x0c".into())
        );
        assert_eq!(
            encoded(&stroke("k", false, false, false, true), empty, true),
            Some("\x0c".into())
        );
        // cmd+k is mac-only.
        assert_eq!(
            encoded(&stroke("k", false, false, false, true), empty, false),
            None
        );
        assert_eq!(
            encoded(&stroke("backspace", false, false, false, true), empty, true),
            Some("\x15".into())
        );
    }

    #[test]
    fn arrows_respect_app_cursor_mode() {
        let empty = TermMode::empty();
        assert_eq!(
            encoded(&stroke("up", false, false, false, false), empty, true),
            Some("\x1b[A".into())
        );
        assert_eq!(
            encoded(
                &stroke("up", false, false, false, false),
                TermMode::APP_CURSOR,
                true
            ),
            Some("\x1bOA".into())
        );
        assert_eq!(
            encoded(&stroke("up", false, false, true, false), empty, true),
            Some("\x1b[1;2A".into())
        );
    }

    #[test]
    fn control_bytes_and_plain_text() {
        let empty = TermMode::empty();
        assert_eq!(
            encoded(&stroke("c", true, false, false, false), empty, true),
            Some("\x03".into())
        );
        assert_eq!(
            encoded(&stroke("a", false, false, false, false), empty, true),
            Some("a".into())
        );
        // Unclaimed cmd chords propagate to the app.
        assert_eq!(
            encoded(&stroke("p", false, false, false, true), empty, true),
            None
        );
        assert_eq!(
            encoded(&stroke("enter", false, false, false, false), empty, true),
            Some("\r".into())
        );
        assert_eq!(
            encoded(&stroke("tab", false, false, true, false), empty, true),
            Some("\x1b[Z".into())
        );
        assert_eq!(
            encoded(
                &stroke("backspace", false, false, false, false),
                empty,
                true
            ),
            Some("\x7f".into())
        );
    }

    #[test]
    fn indexed_palette_covers_cube_and_grays() {
        let palette = TerminalPalette::of(true);
        assert_eq!(palette.indexed(1), c(0xFF7A8E));
        // Cube corner 16 = rgb(0,0,0); 231 = rgb(255,255,255).
        assert_eq!(palette.indexed(16), c(0x000000));
        assert_eq!(palette.indexed(231), c(0xFFFFFF));
        // Gray ramp: 232 = rgb(8,8,8), 255 = rgb(238,238,238).
        assert_eq!(palette.indexed(232), c(0x080808));
        assert_eq!(palette.indexed(255), c(0xEEEEEE));
    }
}
