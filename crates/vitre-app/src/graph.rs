//! Native knowledge graph: service state, search, neighbourhood and source navigation.
use gpui::{
    Context, Entity, EventEmitter, PathBuilder, SharedString, Subscription, Task, Window, canvas,
    div, point, prelude::*, px, relative, uniform_list,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex,
};
use std::{collections::HashMap, sync::Arc};
use vitre_client::EnvironmentClient;
use vitre_contracts::{methods, *};

pub enum GraphEvent {
    OpenFile { path: String, line: Option<u32> },
}
pub struct GraphPanel {
    pub cwd: String,
    client: Arc<EnvironmentClient>,
    query: Entity<InputState>,
    path_target: Entity<InputState>,
    status: Option<GraphStatus>,
    message: String,
    busy: bool,
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    positions: Vec<(f32, f32)>,
    matches: Vec<GraphNode>,
    selected: Option<GraphExplanation>,
    task: Option<Task<()>>,
    _subscription: Subscription,
    installing: bool,
    zoom: f32,
    pan: (f32, f32),
}
impl EventEmitter<GraphEvent> for GraphPanel {}
fn text(s: impl Into<String>) -> TrimmedNonEmptyString {
    TrimmedNonEmptyString(s.into())
}
impl GraphPanel {
    #[cfg(debug_assertions)]
    pub(crate) fn verify_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query
            .update(cx, |s, cx| s.set_value("Workspace", window, cx));
        self.search(cx);
    }
    #[cfg(debug_assertions)]
    pub(crate) fn verify_results(&self) -> Option<Result<usize, String>> {
        (!self.busy).then(|| {
            if self.matches.is_empty() {
                Err(self.message.clone())
            } else {
                Ok(self.matches.len())
            }
        })
    }
    #[cfg(debug_assertions)]
    pub(crate) fn verify_inspect(&mut self, cx: &mut Context<Self>) {
        self.inspect(self.matches[0].id.clone(), cx);
    }
    #[cfg(debug_assertions)]
    pub(crate) fn verify_path(&mut self, cx: &mut Context<Self>) {
        self.find_path("node-5".into(), cx);
    }
    #[cfg(debug_assertions)]
    pub(crate) fn verify_loaded(&self) -> Option<Result<usize, String>> {
        (!self.busy).then(|| {
            if self.nodes.is_empty() {
                Err(self.message.clone())
            } else {
                Ok(self.nodes.len())
            }
        })
    }
    pub fn new(
        client: Arc<EnvironmentClient>,
        cwd: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx
            .new(|cx| InputState::new(window, cx).placeholder("Find a symbol, file, or concept…"));
        let subscription = cx.subscribe(&query, |this: &mut Self, _, e, cx| {
            if matches!(e, InputEvent::Change) {
                this.search(cx);
            }
        });
        let mut this = Self {
            cwd,
            client,
            query,
            path_target: cx
                .new(|cx| InputState::new(window, cx).placeholder("Path to symbol name or ID")),
            status: None,
            message: "Loading graph…".into(),
            busy: false,
            nodes: Vec::new(),
            edges: Vec::new(),
            positions: Vec::new(),
            matches: Vec::new(),
            selected: None,
            task: None,
            _subscription: subscription,
            installing: false,
            zoom: 1.,
            pan: (0., 0.),
        };
        this.refresh(cx);
        this
    }
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.installing {
            return;
        }
        self.selected = None;
        self.nodes.clear();
        self.edges.clear();
        self.positions.clear();
        let client = self.client.clone();
        let cwd = self.cwd.clone();
        self.busy = true;
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            loop {
            let result = client.call::<methods::GraphStatus>(&GraphWorkspaceInput {cwd: text(&cwd)}).await;
            let polling = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Err(vitre_rpc::TypedError::Failed(methods::GraphStatusError::GraphDisabledError(_))) => {
                        this.status = None;
                        this.message = "Explore how this workspace fits together. Enable the graph, install its tools, then build a local code index. Structural builds do not use a model.".into();
                    }
                    Err(e) => this.message = e.user_message().to_string(),
                    Ok(status) => {
                        this.busy = matches!(status.build.state, GraphBuildState::Queued | GraphBuildState::Running);
                        this.message = if !status.enabled {"Enable the knowledge graph to explore relationships in this workspace.".into()}
                            else if this.busy || status.build.state == GraphBuildState::Failed {status.build.detail.as_ref().or(status.build.message.as_ref()).map(|m| m.0.clone()).unwrap_or_else(|| format!("Graph build: {:?}",status.build.state))}
                            else if let Some(snapshot) = &status.snapshot {format!("{} nodes · {} connections{}", snapshot.node_count.0, snapshot.edge_count.0, if snapshot.stale {" · Rebuild to update"} else {""})}
                            else {status.build.message.as_ref().map(|m| m.0.clone()).unwrap_or_else(|| "Build a graph to explore your code.".into())};
                        this.matches = status.snapshot.as_ref().map(|s| s.god_nodes.clone()).unwrap_or_default();
                        this.status = Some(status);
                    }
                } cx.notify(); this.busy
            }).unwrap_or(false);
            if !polling {break;}
            cx.background_executor().timer(std::time::Duration::from_secs(1)).await;
            }
        }));
    }
    fn search(&mut self, cx: &mut Context<Self>) {
        if self.installing {
            return;
        }
        self.selected = None;
        let query = self.query.read(cx).value().trim().to_string();
        if query.is_empty() {
            self.refresh(cx);
            return;
        }
        let client = self.client.clone();
        let cwd = self.cwd.clone();
        let executor = cx.background_executor().clone();
        self.busy = true;
        self.task = Some(cx.spawn(async move |this, cx| {
            executor.timer(std::time::Duration::from_millis(200)).await;
            let result = client
                .call::<methods::GraphQuery>(&GraphQueryInput {
                    cwd: text(cwd),
                    question: text(query),
                    limit: 100,
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(r) => {
                        this.message = format!(
                            "{} results{}",
                            r.total_matches.0,
                            if r.total_matches.0 as usize > r.matches.len() {
                                " · refine your search"
                            } else {
                                ""
                            }
                        );
                        this.matches = r.matches;
                    }
                    Err(e) => this.message = e.user_message().to_string(),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn inspect(&mut self, node: GraphNodeId, cx: &mut Context<Self>) {
        if self.installing {
            return;
        }
        let client = self.client.clone();
        let cwd = self.cwd.clone();
        self.busy = true;
        self.selected = None;
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let explanation = client
                    .call::<methods::GraphExplain>(&GraphNodeQueryInput {
                        cwd: text(&cwd),
                        node: text(&node.0),
                    })
                    .await
                    .map_err(|e| e.user_message().to_string())?;
                let graph = client
                    .call::<methods::GraphSubgraph>(&GraphSubgraphInput {
                        cwd: text(cwd),
                        node_id: Some(node),
                        community_id: None,
                        depth: 2,
                        limit: 120,
                    })
                    .await
                    .map_err(|e| e.user_message().to_string())?;
                let indices: HashMap<_, _> = graph
                    .nodes
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (n.id.clone(), i))
                    .collect();
                let links = graph
                    .edges
                    .iter()
                    .filter_map(|e| Some((*indices.get(&e.source)?, *indices.get(&e.target)?)))
                    .collect::<Vec<_>>();
                let count = graph.nodes.len();
                let positions = cx
                    .background_executor()
                    .spawn(async move { vitre_state::graph_layout::layout(count, &links) })
                    .await;
                Ok::<_, String>((explanation, graph, positions))
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((explanation, graph, positions)) => {
                        this.message = format!(
                            "{} nodes · {} connections{}",
                            graph.nodes.len(),
                            graph.edges.len(),
                            if graph.truncated {
                                " · bounded neighbourhood"
                            } else {
                                ""
                            }
                        );
                        this.matches = explanation
                            .neighbors
                            .iter()
                            .map(|n| n.node.clone())
                            .collect();
                        this.selected = Some(explanation);
                        this.nodes = graph.nodes;
                        this.edges = graph.edges;
                        this.positions = positions;
                        this.zoom = 1.;
                        this.pan = (0., 0.);
                    }
                    Err(e) => this.message = e,
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn find_path(&mut self, target: String, cx: &mut Context<Self>) {
        if self.busy || target.trim().is_empty() {
            return;
        }
        let Some(selected) = &self.selected else {
            return;
        };
        let input = GraphPathInput {
            cwd: text(&self.cwd),
            from: text(&selected.node.id.0),
            to: text(target),
        };
        let client = self.client.clone();
        self.busy = true;
        self.message = "Finding a connection…".into();
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = client.call::<methods::GraphPath>(&input).await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Err(e) => this.message = e.user_message(),
                    Ok(path) => {
                        this.message = if path.nodes.is_empty() {
                            "No connecting path found.".into()
                        } else {
                            format!(
                                "Path: {} nodes{}",
                                path.nodes.len(),
                                if path.nodes.len() > 120 {
                                    " · first 120 drawn; all steps listed below"
                                } else {
                                    ""
                                }
                            )
                        };
                        this.matches = path.nodes.clone();
                        this.nodes = path.nodes.into_iter().take(120).collect();
                        this.edges = path.edges;
                        let len = this.nodes.len().max(1);
                        this.positions = (0..len)
                            .map(|i| {
                                (
                                    0.12 + 0.76 * (i % 6) as f32 / 5.,
                                    0.15 + 0.7 * (i / 6) as f32
                                        / (len.div_ceil(6).max(2) - 1) as f32,
                                )
                            })
                            .collect();
                        this.zoom = 1.;
                        this.pan = (0., 0.);
                    }
                }
                cx.notify();
            });
        }));
    }
    pub fn build(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.message = "Building the workspace graph…".into();
        cx.notify();
        let client = self.client.clone();
        let cwd = self.cwd.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = async {
                client
                    .call::<methods::ServerUpdateSettings>(&ServerUpdateSettingsPayload {
                        patch: serde_json::from_value(
                            serde_json::json!({"knowledgeGraph":{"enabled":true}}),
                        )
                        .unwrap(),
                    })
                    .await
                    .map_err(|e| e.user_message().to_string())?;
                client
                    .call::<methods::GraphBuild>(&GraphBuildInput {
                        cwd: text(cwd),
                        force: true,
                        mode: GraphBuildMode::Structural,
                    })
                    .await
                    .map_err(|e| e.user_message().to_string())
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(_) => this.refresh(cx),
                    Err(e) => this.message = e,
                }
                cx.notify();
            });
        }));
    }
    fn install_runtime(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.installing = true;
        self.message = "Preparing graph tools…".into();
        cx.notify();
        let client = self.client.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = async {
                client
                    .call::<methods::ServerUpdateSettings>(&ServerUpdateSettingsPayload {
                        patch: serde_json::from_value(
                            serde_json::json!({"knowledgeGraph":{"enabled":true}}),
                        )
                        .unwrap(),
                    })
                    .await
                    .map_err(|e| e.user_message().to_string())?;
                let mut stream = client
                    .subscribe::<methods::GraphInstallRuntime>(&GraphInstallRuntimeInput {
                        interpreter_path: None,
                    })
                    .map_err(|e| e.to_string())?;
                while let Some(event) = stream.next().await {
                    match event {
                        vitre_rpc::TypedStreamEvent::Values(events) => {
                            stream.ack().map_err(|e| e.to_string())?;
                            for event in events {
                                match event {
                                    GraphInstallEvent::Progress { detail, stage } => {
                                        let message = detail
                                            .map(|d| d.0)
                                            .unwrap_or_else(|| format!("{stage:?}"));
                                        let _ = this.update(cx, |this, cx| {
                                            this.message = message;
                                            cx.notify();
                                        });
                                    }
                                    GraphInstallEvent::Complete { .. } => {
                                        return Ok::<_, String>(());
                                    }
                                    _ => {}
                                }
                            }
                        }
                        vitre_rpc::TypedStreamEvent::Completed(result) => {
                            result.map_err(|e| e.user_message().to_string())?;
                            break;
                        }
                    }
                }
                Err(
                    "Graph tool installation ended before completion. Retry the installation."
                        .into(),
                )
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.installing = false;
                match result {
                    Ok(()) => this.refresh(cx),
                    Err(e) => this.message = e,
                }
                cx.notify();
            });
        }));
    }
}
impl Render for GraphPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let positions: Vec<_> = self
            .positions
            .iter()
            .map(|(x, y)| {
                (
                    (*x - 0.5) * self.zoom + 0.5 + self.pan.0,
                    (*y - 0.5) * self.zoom + 0.5 + self.pan.1,
                )
            })
            .collect();
        let points = positions.clone();
        let nodes = self.nodes.clone();
        let indices: HashMap<_, _> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        let links = self
            .edges
            .iter()
            .filter_map(|e| Some((*indices.get(&e.source)?, *indices.get(&e.target)?)))
            .collect::<Vec<_>>();
        let stroke = cx.theme().muted_foreground.opacity(0.35);
        let primary = cx.theme().primary;
        let selected = self.selected.as_ref().map(|s| s.node.id.clone());
        let rem_size = window.rem_size();
        let drawing = div()
            .relative()
            .w_full()
            .h_64()
            .flex_shrink_0()
            .overflow_hidden()
            .border_b_1()
            .border_color(cx.theme().border)
            .on_scroll_wheel(
                cx.listener(move |this, event: &gpui::ScrollWheelEvent, _, cx| {
                    let delta = event.delta.pixel_delta(rem_size);
                    if event.modifiers.platform || event.modifiers.control {
                        this.zoom = (this.zoom - f32::from(delta.y) * 0.01).clamp(0.5, 3.);
                    } else {
                        this.pan.0 = (this.pan.0 + f32::from(delta.x) / 600.).clamp(-1., 1.);
                        this.pan.1 = (this.pan.1 + f32::from(delta.y) / 300.).clamp(-1., 1.);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let mut path = PathBuilder::stroke(px(1.));
                        for (a, b) in &links {
                            if let (Some(a), Some(b)) = (points.get(*a), points.get(*b)) {
                                path.move_to(
                                    bounds.origin
                                        + point(bounds.size.width * a.0, bounds.size.height * a.1),
                                );
                                path.line_to(
                                    bounds.origin
                                        + point(bounds.size.width * b.0, bounds.size.height * b.1),
                                );
                            }
                        }
                        if let Ok(path) = path.build() {
                            window.paint_path(path, stroke);
                        }
                    },
                )
                .size_full(),
            )
            .children(nodes.iter().zip(&positions).map(|(n, (x, y))| {
                let id = n.id.clone();
                div()
                    .absolute()
                    .left(relative(*x))
                    .top(relative(*y))
                    .ml(-gpui::rems(0.75))
                    .mt(-gpui::rems(0.75))
                    .child(
                        Button::new(SharedString::from(format!("graph-node:{}", n.id.0)))
                            .ghost()
                            .xsmall()
                            .size_6()
                            .rounded_full()
                            .bg(crate::glass::control(cx))
                            .label("●")
                            .tooltip(n.label.0.clone())
                            .disabled(self.installing)
                            .text_color(if selected.as_ref() == Some(&n.id) {
                                primary
                            } else {
                                cx.theme().foreground
                            })
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.inspect(id.clone(), cx)),
                            ),
                    )
            }));
        let mut view = v_flex().size_full().min_w_0().min_h_0().child(
            v_flex()
                .p_3()
                .gap_2()
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("graph-build")
                                .outline()
                                .small()
                                .label("Build graph")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| this.build(cx))),
                        )
                        .child(
                            Button::new("graph-refresh")
                                .ghost()
                                .small()
                                .label("Refresh")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                        )
                        .when(
                            self.status
                                .as_ref()
                                .is_none_or(|s| s.runtime.state != GraphRuntimeState::Ready),
                            |row| {
                                row.child(
                                    Button::new("graph-install")
                                        .ghost()
                                        .small()
                                        .label("Enable & install tools")
                                        .disabled(self.busy)
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.install_runtime(cx)),
                                        ),
                                )
                            },
                        ),
                )
                .child(Input::new(&self.query).small().disabled(self.installing))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.message.clone()),
                ),
        );
        if !self.nodes.is_empty() {
            view = view.child(drawing).child(
                h_flex()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Scroll to pan · ⌘/Ctrl+scroll to zoom"),
                    )
                    .child(
                        Button::new("graph-reset-view")
                            .ghost()
                            .xsmall()
                            .label("Reset view")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.zoom = 1.;
                                this.pan = (0., 0.);
                                cx.notify();
                            })),
                    ),
            );
        }
        if let Some(explanation) = &self.selected {
            let node = explanation.node.clone();
            let path = node.source_file.clone();
            let line = node.source_line.map(|n| n.0 as u32);
            view = view.child(
                v_flex()
                    .p_3()
                    .gap_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().text_sm().font_medium().child(node.label.0))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} connections{}{}",
                                node.degree.0,
                                explanation
                                    .community
                                    .as_ref()
                                    .map(|c| format!(" · {}", c.label.0))
                                    .unwrap_or_default(),
                                if explanation.truncated {
                                    " · first neighbours shown"
                                } else {
                                    ""
                                }
                            )),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(Input::new(&self.path_target).small().disabled(self.busy))
                            .child(
                                Button::new("graph-find-path")
                                    .outline()
                                    .small()
                                    .label("Find path")
                                    .disabled(self.busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let target = this.path_target.read(cx).value().to_string();
                                        this.find_path(target, cx);
                                    })),
                            ),
                    )
                    .children(path.map(|path| {
                        Button::new("graph-open-source")
                            .ghost()
                            .small()
                            .label(format!("Open {}", path.0))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(GraphEvent::OpenFile {
                                    path: path.0.clone(),
                                    line,
                                })
                            }))
                    })),
            );
        }
        view.child(
            uniform_list(
                "graph-results",
                self.matches.len(),
                cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|i| {
                            let n = &this.matches[i];
                            let id = n.id.clone();
                            let relation = this
                                .selected
                                .as_ref()
                                .and_then(|s| s.neighbors.iter().find(|link| link.node.id == id))
                                .map(|link| format!("{} · {:?}", link.relation.0, link.confidence))
                                .unwrap_or_default();
                            Button::new(SharedString::from(format!("graph-result:{}", id.0)))
                                .ghost()
                                .w_full()
                                .h_10()
                                .justify_start()
                                .tooltip(format!(
                                    "{}\n{}\n{}",
                                    n.label.0,
                                    relation,
                                    n.source_file.as_ref().map(|s| s.0.as_str()).unwrap_or("")
                                ))
                                .child(
                                    v_flex()
                                        .w_full()
                                        .min_w_0()
                                        .child(div().text_sm().truncate().child(n.label.0.clone()))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .truncate()
                                                .child(if relation.is_empty() {
                                                    n.source_file
                                                        .as_ref()
                                                        .map(|s| s.0.clone())
                                                        .unwrap_or_default()
                                                } else {
                                                    relation
                                                }),
                                        ),
                                )
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.inspect(id.clone(), cx)),
                                )
                                .into_any_element()
                        })
                        .collect()
                }),
            )
            .flex_1()
            .min_h_0(),
        )
    }
}
