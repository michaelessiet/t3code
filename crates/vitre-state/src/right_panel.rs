//! Right-panel dock state: a line-for-line port of the Electron app's
//! `apps/web/src/rightPanelStore.ts` (persisted-state version 10).
//!
//! The dock keeps one `{isOpen, activeSurfaceId, surfaces[]}` record per
//! thread, keyed by `${environmentId}:${threadId}`. Three invariants carry
//! most of the behaviour and are easy to get wrong:
//!
//! - **Pruning**: a thread whose state is exactly
//!   `{isOpen:false, activeSurfaceId:null, surfaces:[]}` is deleted from the
//!   map (and therefore from persistence). `close_all_surfaces` erases the
//!   thread's entry entirely.
//! - **Visibility is independent of surfaces**: `close`/`toggle_visibility`
//!   hide the panel without touching the tab list, and the active-surface
//!   selectors return `None` whenever `isOpen` is false — "is the diff
//!   open?" checks must include visibility.
//! - **Two different active-tab fallbacks**: closing a tab falls back to the
//!   right neighbour (`surfaces[min(index, len-1)]`); the file/search
//!   reconcile falls back to the *last* surface; the browser reconcile falls
//!   back to the first preview surface, else `surfaces[0]`.
//!
//! Persistence and rendering live in the app layer; everything here is pure.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Upper bound on split panes inside one terminal surface (Electron's
/// `MAX_TERMINALS_PER_GROUP`). Enforced by the split *callers*, not the store.
pub const MAX_TERMINALS_PER_GROUP: usize = 4;

/// Persisted-state version (Electron's zustand `persist` version). Version 10
/// introduced root-qualified file-surface ids.
pub const PERSISTED_VERSION: u64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceKind {
    Plan,
    Diff,
    Files,
    File,
    Preview,
    Terminal,
    Search,
    Graph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// One dock tab. The `id` is the identity key throughout; every op that
/// matches surfaces matches on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum RightPanelSurface {
    Plan {
        id: String,
    },
    Diff {
        id: String,
    },
    Files {
        id: String,
    },
    Search {
        id: String,
    },
    Graph {
        id: String,
    },
    /// A browser tab. `resource_id` is the preview session's tab id; `None`
    /// is the `browser:new` placeholder shown before a server tab exists.
    Preview {
        id: String,
        resource_id: Option<String>,
    },
    /// A terminal group. The surface id is fixed to the *founding* pane
    /// (`terminal:${resource_id}`); splitting never changes it.
    Terminal {
        id: String,
        resource_id: String,
        terminal_ids: Vec<String>,
        active_terminal_id: String,
        /// Absent = horizontal (Electron deletes the property rather than
        /// storing `"horizontal"`).
        #[serde(skip_serializing_if = "Option::is_none", default)]
        split_direction: Option<SplitDirection>,
    },
    File {
        id: String,
        relative_path: String,
        /// `None` = the thread's primary root.
        root_path: Option<String>,
        reveal_line: Option<u32>,
        reveal_end_line: Option<u32>,
        /// Bumped on every `open_file` for an already-open path so consumers
        /// re-run the scroll/reveal even when path and line are unchanged.
        reveal_request_id: u64,
    },
}

impl RightPanelSurface {
    pub fn id(&self) -> &str {
        match self {
            Self::Plan { id }
            | Self::Diff { id }
            | Self::Files { id }
            | Self::Search { id }
            | Self::Graph { id } => id,
            Self::Preview { id, .. } | Self::Terminal { id, .. } | Self::File { id, .. } => id,
        }
    }

    pub fn kind(&self) -> SurfaceKind {
        match self {
            Self::Plan { .. } => SurfaceKind::Plan,
            Self::Diff { .. } => SurfaceKind::Diff,
            Self::Files { .. } => SurfaceKind::Files,
            Self::Search { .. } => SurfaceKind::Search,
            Self::Graph { .. } => SurfaceKind::Graph,
            Self::Preview { .. } => SurfaceKind::Preview,
            Self::Terminal { .. } => SurfaceKind::Terminal,
            Self::File { .. } => SurfaceKind::File,
        }
    }

    fn singleton(kind: SurfaceKind) -> Option<Self> {
        let id = match kind {
            SurfaceKind::Plan => "plan",
            SurfaceKind::Diff => "diff",
            SurfaceKind::Files => "files",
            SurfaceKind::Search => "search",
            SurfaceKind::Graph => "graph",
            _ => return None,
        };
        Some(match kind {
            SurfaceKind::Plan => Self::Plan { id: id.into() },
            SurfaceKind::Diff => Self::Diff { id: id.into() },
            SurfaceKind::Files => Self::Files { id: id.into() },
            SurfaceKind::Search => Self::Search { id: id.into() },
            SurfaceKind::Graph => Self::Graph { id: id.into() },
            _ => unreachable!(),
        })
    }
}

pub const BROWSER_PLACEHOLDER_ID: &str = "browser:new";

pub fn browser_surface_id(tab_id: &str) -> String {
    format!("browser:{tab_id}")
}

pub fn terminal_surface_id(terminal_id: &str) -> String {
    format!("terminal:{terminal_id}")
}

/// `file:${rootPath ?? ""}:${relativePath}` — Electron's `fileSurfaceId()`.
pub fn file_surface_id(root_path: Option<&str>, relative_path: &str) -> String {
    format!("file:{}:{relative_path}", root_path.unwrap_or(""))
}

/// Lowest unused `term-N` starting at `term-1`. `existing` must be the union
/// of server-known terminal ids and every panel surface's pane ids — the
/// server never allocates ids, and forgetting the panel set collides with the
/// bottom drawer.
pub fn next_terminal_id<'a>(existing: impl Iterator<Item = &'a str> + Clone) -> String {
    for n in 1u64.. {
        let candidate = format!("term-{n}");
        if !existing.clone().any(|id| id == candidate) {
            return candidate;
        }
    }
    unreachable!()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRightPanelState {
    pub is_open: bool,
    pub active_surface_id: Option<String>,
    pub surfaces: Vec<RightPanelSurface>,
}

pub static EMPTY_THREAD_STATE: ThreadRightPanelState = ThreadRightPanelState {
    is_open: false,
    active_surface_id: None,
    surfaces: Vec::new(),
};

impl Default for ThreadRightPanelState {
    fn default() -> Self {
        EMPTY_THREAD_STATE.clone()
    }
}

impl ThreadRightPanelState {
    fn is_empty_state(&self) -> bool {
        !self.is_open && self.active_surface_id.is_none() && self.surfaces.is_empty()
    }

    fn surface_index(&self, surface_id: &str) -> Option<usize> {
        self.surfaces.iter().position(|s| s.id() == surface_id)
    }

    /// Electron's `upsertSurface`: opens the panel, appends only when the id
    /// is new (an existing surface keeps its position *and its fields*), and
    /// activates when asked.
    fn upsert(&mut self, surface: RightPanelSurface, activate: bool) {
        self.is_open = true;
        let id = surface.id().to_string();
        if self.surface_index(&id).is_none() {
            self.surfaces.push(surface);
        }
        if activate {
            self.active_surface_id = Some(id);
        }
    }

    /// Fallback used by tab closes: the right neighbour that slid into the
    /// removed slot, else the last tab.
    fn close_fallback(&self, removed_index: usize) -> Option<String> {
        if self.surfaces.is_empty() {
            return None;
        }
        let ix = removed_index.min(self.surfaces.len() - 1);
        Some(self.surfaces[ix].id().to_string())
    }
}

/// The whole dock store: per-thread states keyed by
/// `${environmentId}:${threadId}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RightPanelMap {
    by_thread_key: BTreeMap<String, ThreadRightPanelState>,
}

impl RightPanelMap {
    pub fn thread(&self, key: &str) -> &ThreadRightPanelState {
        self.by_thread_key.get(key).unwrap_or(&EMPTY_THREAD_STATE)
    }

    pub fn is_open(&self, key: &str) -> bool {
        self.thread(key).is_open
    }

    /// Active surface, or `None` whenever the panel is hidden — Electron's
    /// selectors gate on `isOpen`, so a hidden panel has no active kind.
    pub fn active_surface(&self, key: &str) -> Option<&RightPanelSurface> {
        let state = self.thread(key);
        if !state.is_open {
            return None;
        }
        let id = state.active_surface_id.as_deref()?;
        state.surfaces.iter().find(|s| s.id() == id)
    }

    pub fn active_kind(&self, key: &str) -> Option<SurfaceKind> {
        self.active_surface(key).map(|s| s.kind())
    }

    /// Run `f` over the thread's state; `None` means no-op. Applies the
    /// pruning invariant and reports whether anything changed.
    fn update(
        &mut self,
        key: &str,
        f: impl FnOnce(&ThreadRightPanelState) -> Option<ThreadRightPanelState>,
    ) -> bool {
        let current = self.thread(key);
        let Some(next) = f(current) else {
            return false;
        };
        if next == *current {
            return false;
        }
        if next.is_empty_state() {
            self.by_thread_key.remove(key);
        } else {
            self.by_thread_key.insert(key.to_string(), next);
        }
        true
    }

    /// Open one of the singleton kinds (or reuse/create a preview surface).
    pub fn open(&mut self, key: &str, kind: SurfaceKind) -> bool {
        self.update(key, |current| {
            let mut next = current.clone();
            if kind == SurfaceKind::Preview {
                // Reuse the first existing preview surface; the placeholder
                // is only minted when none exists.
                match current
                    .surfaces
                    .iter()
                    .find(|s| s.kind() == SurfaceKind::Preview)
                {
                    Some(existing) => {
                        let id = existing.id().to_string();
                        next.is_open = true;
                        next.active_surface_id = Some(id);
                    }
                    None => next.upsert(
                        RightPanelSurface::Preview {
                            id: BROWSER_PLACEHOLDER_ID.into(),
                            resource_id: None,
                        },
                        true,
                    ),
                }
            } else {
                next.upsert(RightPanelSurface::singleton(kind)?, true);
            }
            Some(next)
        })
    }

    /// `tab_id` non-null: drop the placeholder, upsert `browser:${tab_id}`.
    /// Null: upsert the placeholder.
    pub fn open_browser(&mut self, key: &str, tab_id: Option<&str>) -> bool {
        self.update(key, |current| {
            let mut next = current.clone();
            let surface = match tab_id {
                Some(tab_id) => {
                    next.surfaces.retain(|s| s.id() != BROWSER_PLACEHOLDER_ID);
                    RightPanelSurface::Preview {
                        id: browser_surface_id(tab_id),
                        resource_id: Some(tab_id.to_string()),
                    }
                }
                None => RightPanelSurface::Preview {
                    id: BROWSER_PLACEHOLDER_ID.into(),
                    resource_id: None,
                },
            };
            next.upsert(surface, true);
            Some(next)
        })
    }

    /// Open (or re-reveal) a file tab. Always bumps `reveal_request_id`, so
    /// re-opening an already-open file re-triggers the reveal; replaces an
    /// existing surface in place (the tab keeps its position); removes the
    /// standalone `files` explorer tab (file tabs replace it).
    pub fn open_file(
        &mut self,
        key: &str,
        relative_path: &str,
        line: Option<u32>,
        end_line: Option<u32>,
        root_path: Option<&str>,
    ) -> bool {
        self.update(key, |current| {
            let mut next = current.clone();
            next.surfaces.retain(|s| s.kind() != SurfaceKind::Files);
            let id = file_surface_id(root_path, relative_path);
            let reveal_line = line.map(|l| l.max(1));
            let reveal_end_line = match (reveal_line, end_line) {
                (Some(start), Some(end)) if end > start => Some(end),
                _ => None,
            };
            let previous_request = next.surfaces.iter().find_map(|s| match s {
                RightPanelSurface::File {
                    id: existing,
                    reveal_request_id,
                    ..
                } if *existing == id => Some(*reveal_request_id),
                _ => None,
            });
            let surface = RightPanelSurface::File {
                id: id.clone(),
                relative_path: relative_path.to_string(),
                root_path: root_path.map(str::to_string),
                reveal_line,
                reveal_end_line,
                reveal_request_id: previous_request.unwrap_or(0) + 1,
            };
            match next.surface_index(&id) {
                Some(ix) => next.surfaces[ix] = surface,
                None => next.surfaces.push(surface),
            }
            next.is_open = true;
            next.active_surface_id = Some(id);
            Some(next)
        })
    }

    pub fn open_terminal(&mut self, key: &str, terminal_id: &str) -> bool {
        self.update(key, |current| {
            let mut next = current.clone();
            next.upsert(
                RightPanelSurface::Terminal {
                    id: terminal_surface_id(terminal_id),
                    resource_id: terminal_id.to_string(),
                    terminal_ids: vec![terminal_id.to_string()],
                    active_terminal_id: terminal_id.to_string(),
                    split_direction: None,
                },
                true,
            );
            Some(next)
        })
    }

    pub fn split_terminal(
        &mut self,
        key: &str,
        surface_id: &str,
        terminal_id: &str,
        direction: SplitDirection,
    ) -> bool {
        self.update(key, |current| {
            let mut next = current.clone();
            let ix = next.surface_index(surface_id)?;
            let RightPanelSurface::Terminal {
                terminal_ids,
                active_terminal_id,
                split_direction,
                ..
            } = &mut next.surfaces[ix]
            else {
                return None;
            };
            if !terminal_ids.iter().any(|id| id == terminal_id) {
                terminal_ids.push(terminal_id.to_string());
            }
            *active_terminal_id = terminal_id.to_string();
            // Horizontal is stored as *absence*, mirroring Electron deleting
            // the property before conditionally re-adding "vertical".
            *split_direction = match direction {
                SplitDirection::Horizontal => None,
                SplitDirection::Vertical => Some(SplitDirection::Vertical),
            };
            next.is_open = true;
            next.active_surface_id = Some(surface_id.to_string());
            Some(next)
        })
    }

    /// Activates the surface unconditionally; moves the active pane only when
    /// the surface is a terminal that actually contains `terminal_id`.
    pub fn activate_terminal(&mut self, key: &str, surface_id: &str, terminal_id: &str) -> bool {
        self.update(key, |current| {
            let mut next = current.clone();
            next.active_surface_id = Some(surface_id.to_string());
            if let Some(ix) = next.surface_index(surface_id)
                && let RightPanelSurface::Terminal {
                    terminal_ids,
                    active_terminal_id,
                    ..
                } = &mut next.surfaces[ix]
                && terminal_ids.iter().any(|id| id == terminal_id)
            {
                *active_terminal_id = terminal_id.to_string();
            }
            Some(next)
        })
    }

    /// Remove one pane; removes the whole surface when it was the last pane.
    pub fn close_terminal(&mut self, key: &str, surface_id: &str, terminal_id: &str) -> bool {
        self.update(key, |current| {
            let mut next = current.clone();
            let ix = next.surface_index(surface_id)?;
            let RightPanelSurface::Terminal {
                terminal_ids,
                active_terminal_id,
                ..
            } = &mut next.surfaces[ix]
            else {
                return None;
            };
            terminal_ids.retain(|id| id != terminal_id);
            if terminal_ids.is_empty() {
                let was_active = current.active_surface_id.as_deref() == Some(surface_id);
                next.surfaces.remove(ix);
                next.is_open = !next.surfaces.is_empty() && current.is_open;
                if was_active {
                    next.active_surface_id = next.close_fallback(ix);
                }
            } else if active_terminal_id == terminal_id {
                // The closed pane was focused: fall back to the last
                // remaining pane.
                *active_terminal_id = terminal_ids.last().cloned().unwrap_or_default();
            }
            Some(next)
        })
    }

    /// Only when the surface exists: open the panel and activate it.
    pub fn activate_surface(&mut self, key: &str, surface_id: &str) -> bool {
        self.update(key, |current| {
            current.surface_index(surface_id)?;
            let mut next = current.clone();
            next.is_open = true;
            next.active_surface_id = Some(surface_id.to_string());
            Some(next)
        })
    }

    pub fn close_surface(&mut self, key: &str, surface_id: &str) -> bool {
        self.update(key, |current| {
            let ix = current.surface_index(surface_id)?;
            let mut next = current.clone();
            next.surfaces.remove(ix);
            // A hidden panel stays hidden even when tabs remain.
            next.is_open = !next.surfaces.is_empty() && current.is_open;
            if current.active_surface_id.as_deref() == Some(surface_id) {
                next.active_surface_id = next.close_fallback(ix);
            }
            Some(next)
        })
    }

    /// Force-shows the panel (unlike `close_surface`).
    pub fn close_other_surfaces(&mut self, key: &str, surface_id: &str) -> bool {
        self.update(key, |current| {
            let ix = current.surface_index(surface_id)?;
            if current.surfaces.len() <= 1 {
                return None;
            }
            let mut next = current.clone();
            next.surfaces = vec![current.surfaces[ix].clone()];
            next.active_surface_id = Some(surface_id.to_string());
            next.is_open = true;
            Some(next)
        })
    }

    /// Does not touch visibility.
    pub fn close_surfaces_to_right(&mut self, key: &str, surface_id: &str) -> bool {
        self.update(key, |current| {
            let ix = current.surface_index(surface_id)?;
            if ix + 1 == current.surfaces.len() {
                return None;
            }
            let mut next = current.clone();
            next.surfaces.truncate(ix + 1);
            let active_survived = current
                .active_surface_id
                .as_deref()
                .is_some_and(|active| next.surface_index(active).is_some());
            if !active_survived {
                next.active_surface_id = Some(surface_id.to_string());
            }
            Some(next)
        })
    }

    /// Results in the exact-empty state, which the pruning invariant then
    /// deletes from the map entirely.
    pub fn close_all_surfaces(&mut self, key: &str) -> bool {
        self.update(key, |current| {
            if current.surfaces.is_empty() {
                return None;
            }
            Some(EMPTY_THREAD_STATE.clone())
        })
    }

    pub fn show(&mut self, key: &str) -> bool {
        self.update(key, |current| {
            if current.is_open {
                return None;
            }
            let mut next = current.clone();
            next.is_open = true;
            Some(next)
        })
    }

    pub fn close(&mut self, key: &str) -> bool {
        self.update(key, |current| {
            if !current.is_open {
                return None;
            }
            let mut next = current.clone();
            next.is_open = false;
            Some(next)
        })
    }

    /// Flips visibility; opening with zero surfaces shows the empty state.
    pub fn toggle_visibility(&mut self, key: &str) -> bool {
        self.update(key, |current| {
            let mut next = current.clone();
            next.is_open = !next.is_open;
            Some(next)
        })
    }

    /// `toggle(kind)`: hides the panel (tabs retained) when it is open on
    /// that kind; otherwise same as `open(kind)`.
    pub fn toggle(&mut self, key: &str, kind: SurfaceKind) -> bool {
        let active_matches = self.active_kind(key) == Some(kind);
        if active_matches {
            self.close(key)
        } else {
            self.open(key, kind)
        }
    }

    pub fn remove_thread(&mut self, key: &str) -> bool {
        self.by_thread_key.remove(key).is_some()
    }

    /// Rebuild the tab list against the authoritative preview-session set:
    /// `[non-browser…, existing valid browser tabs…, new browser tabs…]`.
    /// Kills the placeholder; active falls back to the first preview surface,
    /// else the first tab. Visibility untouched.
    pub fn reconcile_browser_surfaces(&mut self, key: &str, tab_ids: &[String]) -> bool {
        self.update(key, |current| {
            let valid: Vec<String> = tab_ids.iter().map(|id| browser_surface_id(id)).collect();
            let mut surfaces: Vec<RightPanelSurface> = current
                .surfaces
                .iter()
                .filter(|s| s.kind() != SurfaceKind::Preview)
                .cloned()
                .collect();
            surfaces.extend(
                current
                    .surfaces
                    .iter()
                    .filter(|s| {
                        s.kind() == SurfaceKind::Preview
                            && s.id() != BROWSER_PLACEHOLDER_ID
                            && valid.iter().any(|id| id == s.id())
                    })
                    .cloned(),
            );
            for tab_id in tab_ids {
                let id = browser_surface_id(tab_id);
                if !surfaces.iter().any(|s| s.id() == id) {
                    surfaces.push(RightPanelSurface::Preview {
                        id,
                        resource_id: Some(tab_id.clone()),
                    });
                }
            }
            let mut next = current.clone();
            next.surfaces = surfaces;
            let active_survived = current
                .active_surface_id
                .as_deref()
                .is_some_and(|active| next.surface_index(active).is_some());
            if !active_survived {
                next.active_surface_id = next
                    .surfaces
                    .iter()
                    .find(|s| s.kind() == SurfaceKind::Preview)
                    .or(next.surfaces.first())
                    .map(|s| s.id().to_string());
            }
            Some(next)
        })
    }

    /// Remove every files/file/search surface when no workspace is available.
    /// Callers must gate this on environment bootstrap being complete, or
    /// persisted tabs get wiped during reconnect.
    pub fn reconcile_file_surfaces(&mut self, key: &str, workspace_available: bool) -> bool {
        if workspace_available {
            return false;
        }
        self.update(key, |current| {
            let mut next = current.clone();
            next.surfaces.retain(|s| {
                !matches!(
                    s.kind(),
                    SurfaceKind::Files | SurfaceKind::File | SurfaceKind::Search
                )
            });
            if next.surfaces.len() == current.surfaces.len() {
                return None;
            }
            if next.surfaces.is_empty() {
                next.is_open = false;
            }
            let active_survived = current
                .active_surface_id
                .as_deref()
                .is_some_and(|active| next.surface_index(active).is_some());
            if !active_survived {
                // Unlike tab closes, this falls back to the *last* survivor.
                next.active_surface_id = next.surfaces.last().map(|s| s.id().to_string());
            }
            Some(next)
        })
    }

    /// Load from the persisted `byThreadKey` blob, applying Electron's
    /// version-10 migration (`migratePersistedRightPanelState`): file
    /// surfaces are re-id'd and their reveal fields sanitized, terminal
    /// surfaces are dropped unless their id matches their founding pane, and
    /// anything unreadable degrades to the empty state rather than failing.
    pub fn from_persisted(by_thread_key: &Value) -> Self {
        let mut map = RightPanelMap::default();
        let Some(entries) = by_thread_key.as_object() else {
            return map;
        };
        for (key, stored) in entries {
            let state = sanitize_thread(stored);
            if !state.is_empty_state() {
                map.by_thread_key.insert(key.clone(), state);
            }
        }
        map
    }

    pub fn to_persisted(&self) -> Value {
        serde_json::to_value(&self.by_thread_key).unwrap_or(Value::Null)
    }
}

fn sanitize_thread(stored: &Value) -> ThreadRightPanelState {
    let Some(entry) = stored.as_object() else {
        return EMPTY_THREAD_STATE.clone();
    };
    let mut id_remap: Vec<(String, String)> = Vec::new();
    let mut surfaces: Vec<RightPanelSurface> = Vec::new();
    for raw in entry
        .get("surfaces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(surface) = sanitize_surface(raw, &mut id_remap) {
            surfaces.push(surface);
        }
    }
    let active_surface_id = entry
        .get("activeSurfaceId")
        .and_then(Value::as_str)
        .map(|active| {
            id_remap
                .iter()
                .find(|(old, _)| old == active)
                .map(|(_, new)| new.clone())
                .unwrap_or_else(|| active.to_string())
        })
        .filter(|active| surfaces.iter().any(|s| s.id() == active));
    let is_open = entry
        .get("isOpen")
        .and_then(Value::as_bool)
        .unwrap_or(active_surface_id.is_some());
    ThreadRightPanelState {
        is_open,
        active_surface_id,
        surfaces,
    }
}

fn sanitize_surface(
    raw: &Value,
    id_remap: &mut Vec<(String, String)>,
) -> Option<RightPanelSurface> {
    let kind = raw.get("kind").and_then(Value::as_str)?;
    match kind {
        "file" => {
            let stored_id = raw.get("id").and_then(Value::as_str)?;
            let relative_path = raw.get("relativePath").and_then(Value::as_str)?;
            let root_path = raw
                .get("rootPath")
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty());
            let reveal_line = raw
                .get("revealLine")
                .and_then(Value::as_f64)
                .filter(|line| line.is_finite())
                .map(|line| (line.trunc() as i64).max(1) as u32);
            let reveal_end_line = reveal_line.and_then(|start| {
                raw.get("revealEndLine")
                    .and_then(Value::as_f64)
                    .filter(|line| line.is_finite())
                    .map(|line| line.trunc() as i64)
                    .filter(|&end| end > start as i64)
                    .map(|end| end as u32)
            });
            let reveal_request_id = raw
                .get("revealRequestId")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let id = file_surface_id(root_path, relative_path);
            if id != stored_id {
                id_remap.push((stored_id.to_string(), id.clone()));
            }
            Some(RightPanelSurface::File {
                id,
                relative_path: relative_path.to_string(),
                root_path: root_path.map(str::to_string),
                reveal_line,
                reveal_end_line,
                reveal_request_id,
            })
        }
        "terminal" => {
            let id = raw.get("id").and_then(Value::as_str)?;
            let resource_id = raw.get("resourceId").and_then(Value::as_str)?;
            // Kills the legacy singleton `"terminal"` surface.
            if id != terminal_surface_id(resource_id) {
                return None;
            }
            let mut terminal_ids: Vec<String> = Vec::new();
            for value in raw
                .get("terminalIds")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(pane) = value.as_str()
                    && !terminal_ids.iter().any(|id| id == pane)
                {
                    terminal_ids.push(pane.to_string());
                }
            }
            if terminal_ids.is_empty() {
                terminal_ids.push(resource_id.to_string());
            }
            let active_terminal_id = raw
                .get("activeTerminalId")
                .and_then(Value::as_str)
                .filter(|active| terminal_ids.iter().any(|id| id == active))
                .unwrap_or(&terminal_ids[0])
                .to_string();
            let split_direction = match raw.get("splitDirection").and_then(Value::as_str) {
                Some("vertical") => Some(SplitDirection::Vertical),
                _ => None,
            };
            Some(RightPanelSurface::Terminal {
                id: id.to_string(),
                resource_id: resource_id.to_string(),
                terminal_ids,
                active_terminal_id,
                split_direction,
            })
        }
        // Other kinds pass through (Electron keeps them verbatim; here that
        // means re-decoding just the fields each kind carries).
        _ => serde_json::from_value(raw.clone()).ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const KEY: &str = "env-1:thread-1";

    #[test]
    fn close_all_prunes_the_thread_key() {
        let mut map = RightPanelMap::default();
        assert!(map.open(KEY, SurfaceKind::Diff));
        assert!(map.close_all_surfaces(KEY));
        assert_eq!(map.to_persisted(), json!({}));
        // close on a never-opened thread is a no-op (pruning keeps it clean).
        assert!(!map.close(KEY));
    }

    #[test]
    fn hidden_panel_has_no_active_kind() {
        let mut map = RightPanelMap::default();
        map.open(KEY, SurfaceKind::Diff);
        assert_eq!(map.active_kind(KEY), Some(SurfaceKind::Diff));
        map.close(KEY);
        assert_eq!(map.active_kind(KEY), None);
        // Tabs are retained while hidden.
        assert_eq!(map.thread(KEY).surfaces.len(), 1);
        // Closing a tab on a hidden panel keeps it hidden.
        map.open(KEY, SurfaceKind::Files);
        map.close(KEY);
        map.close_surface(KEY, "files");
        assert!(!map.is_open(KEY));
    }

    #[test]
    fn close_surface_falls_back_to_right_neighbour() {
        let mut map = RightPanelMap::default();
        map.open(KEY, SurfaceKind::Diff);
        map.open(KEY, SurfaceKind::Files);
        map.open(KEY, SurfaceKind::Search);
        map.activate_surface(KEY, "files");
        map.close_surface(KEY, "files");
        assert_eq!(map.thread(KEY).active_surface_id.as_deref(), Some("search"));
        map.close_surface(KEY, "search");
        assert_eq!(map.thread(KEY).active_surface_id.as_deref(), Some("diff"));
    }

    #[test]
    fn close_others_force_opens_and_close_right_keeps_visibility() {
        let mut map = RightPanelMap::default();
        map.open(KEY, SurfaceKind::Diff);
        map.open(KEY, SurfaceKind::Files);
        map.close(KEY);
        assert!(map.close_other_surfaces(KEY, "diff"));
        assert!(map.is_open(KEY));
        assert_eq!(map.thread(KEY).surfaces.len(), 1);

        map.open(KEY, SurfaceKind::Files);
        map.close(KEY);
        assert!(map.close_surfaces_to_right(KEY, "diff"));
        assert!(!map.is_open(KEY));
    }

    #[test]
    fn open_file_bumps_reveal_and_replaces_in_place() {
        let mut map = RightPanelMap::default();
        map.open(KEY, SurfaceKind::Files);
        map.open_file(KEY, "src/a.rs", Some(3), Some(2), None);
        // The standalone explorer tab is replaced by file tabs.
        assert!(map.thread(KEY).surface_index("files").is_none());
        map.open(KEY, SurfaceKind::Diff);
        map.open_file(KEY, "src/a.rs", Some(7), Some(9), None);
        let state = map.thread(KEY);
        assert_eq!(state.surfaces[0].id(), "file::src/a.rs");
        let RightPanelSurface::File {
            reveal_line,
            reveal_end_line,
            reveal_request_id,
            ..
        } = &state.surfaces[0]
        else {
            panic!("expected file surface");
        };
        assert_eq!(*reveal_line, Some(7));
        assert_eq!(*reveal_end_line, Some(9));
        assert_eq!(*reveal_request_id, 2);
    }

    #[test]
    fn terminal_pane_lifecycle() {
        let mut map = RightPanelMap::default();
        map.open_terminal(KEY, "term-1");
        let surface_id = terminal_surface_id("term-1");
        map.split_terminal(KEY, &surface_id, "term-2", SplitDirection::Vertical);
        map.split_terminal(KEY, &surface_id, "term-3", SplitDirection::Horizontal);
        let RightPanelSurface::Terminal {
            terminal_ids,
            active_terminal_id,
            split_direction,
            ..
        } = &map.thread(KEY).surfaces[0]
        else {
            panic!("expected terminal surface");
        };
        assert_eq!(terminal_ids, &["term-1", "term-2", "term-3"]);
        assert_eq!(active_terminal_id, "term-3");
        // Horizontal is stored as absence.
        assert_eq!(*split_direction, None);

        map.close_terminal(KEY, &surface_id, "term-3");
        let RightPanelSurface::Terminal {
            active_terminal_id, ..
        } = &map.thread(KEY).surfaces[0]
        else {
            panic!("expected terminal surface");
        };
        assert_eq!(active_terminal_id, "term-2");
        map.close_terminal(KEY, &surface_id, "term-1");
        map.close_terminal(KEY, &surface_id, "term-2");
        assert!(map.thread(KEY).surfaces.is_empty());
    }

    #[test]
    fn next_terminal_id_fills_gaps() {
        let existing = ["term-1", "term-3"];
        assert_eq!(next_terminal_id(existing.iter().copied()), "term-2");
        assert_eq!(next_terminal_id([].iter().copied()), "term-1");
    }

    #[test]
    fn toggle_hides_on_matching_kind_only() {
        let mut map = RightPanelMap::default();
        map.toggle(KEY, SurfaceKind::Diff);
        assert!(map.is_open(KEY));
        map.toggle(KEY, SurfaceKind::Files);
        assert_eq!(map.active_kind(KEY), Some(SurfaceKind::Files));
        map.toggle(KEY, SurfaceKind::Files);
        assert!(!map.is_open(KEY));
        assert_eq!(map.thread(KEY).surfaces.len(), 2);
    }

    #[test]
    fn browser_reconcile_reorders_and_drops_placeholder() {
        let mut map = RightPanelMap::default();
        map.open_browser(KEY, None);
        map.open(KEY, SurfaceKind::Diff);
        map.open_browser(KEY, Some("tab-1"));
        map.reconcile_browser_surfaces(KEY, &["tab-1".into(), "tab-2".into()]);
        let ids: Vec<&str> = map.thread(KEY).surfaces.iter().map(|s| s.id()).collect();
        assert_eq!(ids, vec!["diff", "browser:tab-1", "browser:tab-2"]);
        // Stale sessions drop tabs; active falls back to the first preview.
        map.activate_surface(KEY, "browser:tab-1");
        map.reconcile_browser_surfaces(KEY, &["tab-2".into()]);
        assert_eq!(
            map.thread(KEY).active_surface_id.as_deref(),
            Some("browser:tab-2")
        );
    }

    #[test]
    fn file_reconcile_falls_back_to_last_survivor() {
        let mut map = RightPanelMap::default();
        map.open(KEY, SurfaceKind::Diff);
        map.open(KEY, SurfaceKind::Graph);
        map.open_file(KEY, "a.rs", None, None, None);
        assert!(!map.reconcile_file_surfaces(KEY, true));
        assert!(map.reconcile_file_surfaces(KEY, false));
        assert_eq!(map.thread(KEY).active_surface_id.as_deref(), Some("graph"));
    }

    #[test]
    fn migration_sanitizes_files_and_drops_legacy_terminals() {
        let persisted = json!({
            "env:thread": {
                "isOpen": true,
                "activeSurfaceId": "file:src/a.rs",
                "surfaces": [
                    // Pre-version-10 file id: rewritten in place, active follows.
                    {"kind": "file", "id": "file:src/a.rs", "relativePath": "src/a.rs",
                     "revealLine": 0.9, "revealEndLine": 12.7, "revealRequestId": 4},
                    // Legacy singleton terminal: dropped.
                    {"kind": "terminal", "id": "terminal", "resourceId": "term-1",
                     "terminalIds": ["term-1"], "activeTerminalId": "term-1"},
                    {"kind": "terminal", "id": "terminal:term-2", "resourceId": "term-2",
                     "terminalIds": ["term-2", "term-2", "term-9"], "activeTerminalId": "gone"},
                    {"kind": "diff", "id": "diff"}
                ]
            },
            "env:junk": 7
        });
        let map = RightPanelMap::from_persisted(&persisted);
        let state = map.thread("env:thread");
        assert_eq!(state.surfaces.len(), 3);
        let RightPanelSurface::File {
            id,
            reveal_line,
            reveal_end_line,
            ..
        } = &state.surfaces[0]
        else {
            panic!("expected file surface");
        };
        assert_eq!(id, "file::src/a.rs");
        assert_eq!(state.active_surface_id.as_deref(), Some("file::src/a.rs"));
        // revealLine 0.9 truncates to 0 then clamps to 1; endLine 12 > 1 kept.
        assert_eq!(*reveal_line, Some(1));
        assert_eq!(*reveal_end_line, Some(12));
        let RightPanelSurface::Terminal {
            terminal_ids,
            active_terminal_id,
            ..
        } = &state.surfaces[1]
        else {
            panic!("expected terminal surface");
        };
        assert_eq!(terminal_ids, &["term-2", "term-9"]);
        assert_eq!(active_terminal_id, "term-2");
        // Junk thread entries degrade to empty and are pruned.
        assert_eq!(map.thread("env:junk"), &EMPTY_THREAD_STATE);

        // Round-trip: what we persist re-loads identically.
        let reloaded = RightPanelMap::from_persisted(&map.to_persisted());
        assert_eq!(reloaded, map);
    }
}
