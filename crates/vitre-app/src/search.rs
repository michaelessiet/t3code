//! Persistent workspace search. Requests are cancellable; writes use CAS revisions.
use gpui::{
    App, Context, Entity, EventEmitter, Focusable as _, Subscription, Task, Window, div,
    prelude::*, uniform_list,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement as _,
    v_flex,
};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use vitre_client::EnvironmentClient;
use vitre_contracts::{
    ProjectReadFileInput, ProjectSearchContentInput, ProjectSearchContentMatch,
    ProjectWriteFileInput, TrimmedNonEmptyString,
    methods::{ProjectsReadFile, ProjectsSearchContent, ProjectsWriteFile},
};
use vitre_state::search::SearchPattern;

pub enum SearchEvent {
    OpenFile { path: String, line: u32 },
}
pub struct SearchPanel {
    pub cwd: String,
    client: Arc<EnvironmentClient>,
    query: Entity<InputState>,
    replacement: Entity<InputState>,
    include: Entity<InputState>,
    exclude: Entity<InputState>,
    pattern: SearchPattern,
    matches: Vec<ProjectSearchContentMatch>,
    status: String,
    truncated: bool,
    searching: bool,
    replacing: bool,
    task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
    selected: usize,
    scroll: gpui::UniformListScrollHandle,
}
impl EventEmitter<SearchEvent> for SearchPanel {}
fn text(value: impl Into<String>) -> TrimmedNonEmptyString {
    TrimmedNonEmptyString(value.into())
}

impl SearchPanel {
    #[cfg(debug_assertions)]
    pub(crate) fn verify_query(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.query
            .update(cx, |q, cx| q.set_value(query, window, cx));
        self.search(cx);
    }
    #[cfg(debug_assertions)]
    pub(crate) fn verification_result(&self) -> Option<Result<usize, String>> {
        (!self.searching).then(|| {
            if self.matches.is_empty() {
                Err(self.status.clone())
            } else {
                Ok(self.matches.len())
            }
        })
    }
    #[cfg(debug_assertions)]
    pub(crate) fn verify_replace(&mut self, cx: &mut Context<Self>) {
        self.replace(
            self.matches.iter().map(|m| m.path.0.clone()).collect(),
            self.pattern.clone(),
            "Studio".into(),
            cx,
        );
    }
    #[cfg(debug_assertions)]
    pub(crate) fn verification_replace_done(&self) -> Option<String> {
        (!self.replacing).then(|| self.status.clone())
    }
    pub fn new(
        client: Arc<EnvironmentClient>,
        cwd: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find in files"));
        let replacement = cx.new(|cx| InputState::new(window, cx).placeholder("Replace with"));
        let include =
            cx.new(|cx| InputState::new(window, cx).placeholder("Include files, e.g. src/**"));
        let exclude =
            cx.new(|cx| InputState::new(window, cx).placeholder("Exclude files, e.g. **/*.lock"));
        let subscriptions = [&query, &include, &exclude]
            .into_iter()
            .map(|input| {
                cx.subscribe(input, |this: &mut Self, _, event, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.search(cx);
                    }
                })
            })
            .collect();
        Self {
            client,
            cwd,
            query,
            replacement,
            include,
            exclude,
            pattern: SearchPattern::default(),
            matches: Vec::new(),
            status: "Search this workspace by text or regular expression.".into(),
            truncated: false,
            searching: false,
            replacing: false,
            task: None,
            _subscriptions: subscriptions,
            selected: 0,
            scroll: Default::default(),
        }
    }
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.query.update(cx, |s, cx| s.focus(window, cx));
    }
    fn search(&mut self, cx: &mut Context<Self>) {
        self.task = None;
        self.pattern.query = self.query.read(cx).value().to_string();
        self.matches.clear();
        self.truncated = false;
        self.searching = false;
        self.selected = 0;
        if self.pattern.query.is_empty() {
            self.status = "Search this workspace by text or regular expression.".into();
            cx.notify();
            return;
        }
        if let Err(error) = self.pattern.compile() {
            self.status = format!("Invalid search: {error}");
            cx.notify();
            return;
        }
        let input = ProjectSearchContentInput {
            cwd: text(&self.cwd),
            query: self.pattern.query.clone(),
            case_sensitive: Some(Some(self.pattern.case_sensitive)),
            whole_word: Some(Some(self.pattern.whole_word)),
            regex: Some(Some(self.pattern.regex)),
            include_glob: (!self.include.read(cx).value().trim().is_empty())
                .then(|| Some(text(self.include.read(cx).value().trim()))),
            exclude_glob: (!self.exclude.read(cx).value().trim().is_empty())
                .then(|| Some(text(self.exclude.read(cx).value().trim()))),
            max_results: Some(Some(2000)),
        };
        let client = self.client.clone();
        let executor = cx.background_executor().clone();
        self.searching = true;
        self.status = "Searching…".into();
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            executor.timer(Duration::from_millis(180)).await;
            let result = client.call::<ProjectsSearchContent>(&input).await;
            let _ = this.update(cx, |this, cx| {
                this.searching = false;
                match result {
                    Ok(result) => {
                        this.status = format!(
                            "{} matches in {} files{}",
                            result.matches.len(),
                            result.file_count.0,
                            if result.truncated {
                                " · Narrow the search to see all matches"
                            } else {
                                ""
                            }
                        );
                        this.truncated = result.truncated;
                        this.matches = result.matches;
                    }
                    Err(e) => this.status = e.user_message().to_string(),
                }
                cx.notify();
            });
        }));
    }
    fn confirm_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.replacing || self.searching || self.matches.is_empty() || self.truncated {
            return;
        }
        let paths: BTreeSet<_> = self.matches.iter().map(|m| m.path.0.clone()).collect();
        let pattern = self.pattern.clone();
        let replacement = self.replacement.read(cx).value().to_string();
        let owner = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone(); let paths = paths.clone(); let pattern = pattern.clone(); let replacement = replacement.clone();
            dialog.title(format!("Replace matches in {} files?", paths.len())).confirm()
                .button_props(gpui_component::dialog::DialogButtonProps::default().ok_text("Replace").cancel_text("Cancel").show_cancel(true))
                .description("Files changed since reading are skipped. Review source-control changes before committing.")
                .on_ok(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| this.replace(paths.clone(), pattern.clone(), replacement.clone(), cx)); true
                })
        });
    }
    fn replace(
        &mut self,
        paths: BTreeSet<String>,
        pattern: SearchPattern,
        replacement: String,
        cx: &mut Context<Self>,
    ) {
        if self.replacing {
            return;
        }
        self.replacing = true;
        self.status = "Replacing…".into();
        cx.notify();
        let client = self.client.clone();
        let cwd = self.cwd.clone();
        cx.spawn(async move |this, cx| {
            let mut changed = 0;
            let mut failures = Vec::new();
            for path in paths {
                let result = async {
                    let file = client
                        .call::<ProjectsReadFile>(&ProjectReadFileInput {
                            cwd: text(&cwd),
                            relative_path: text(&path),
                        })
                        .await
                        .map_err(|e| e.user_message().to_string())?;
                    if file.truncated {
                        return Err("File exceeds the safe editing limit".into());
                    }
                    let revision = file
                        .revision
                        .flatten()
                        .ok_or("No file revision available")?;
                    let (contents, count) = pattern.replace(&file.contents.0, &replacement)?;
                    if count == 0 {
                        return Ok(0);
                    }
                    client
                        .call::<ProjectsWriteFile>(&ProjectWriteFileInput {
                            cwd: text(&cwd),
                            relative_path: text(&path),
                            contents: text(contents),
                            base_revision: Some(Some(revision)),
                        })
                        .await
                        .map_err(|e| e.user_message().to_string())?;
                    Ok::<_, String>(count)
                }
                .await;
                match result {
                    Ok(n) => changed += n,
                    Err(e) => failures.push(format!("{path}: {e}")),
                }
            }
            let _ = this.update(cx, |this, cx| {
                this.replacing = false;
                this.matches.clear();
                this.status = format!(
                    "Replaced {changed} matches.{}",
                    if failures.is_empty() {
                        " Search again to refresh results.".into()
                    } else {
                        format!(" Skipped {} files: {}", failures.len(), failures.join("; "))
                    }
                );
                cx.notify();
            });
        })
        .detach();
    }
}
impl Render for SearchPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let options = [
            ("Aa", "Match case", self.pattern.case_sensitive),
            ("Ab", "Whole word", self.pattern.whole_word),
            (".*", "Regular expression", self.pattern.regex),
        ];
        v_flex()
            .id("workspace-search")
            .size_full()
            .min_w_0()
            .min_h_0()
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.matches.is_empty()
                    || this.replacing
                    || !this.query.read(cx).focus_handle(cx).is_focused(window)
                {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "up" => this.selected = this.selected.saturating_sub(1),
                    "down" => this.selected = (this.selected + 1).min(this.matches.len() - 1),
                    "enter" => {
                        let m = &this.matches[this.selected];
                        cx.emit(SearchEvent::OpenFile {
                            path: m.path.0.clone(),
                            line: m.line.0 as u32,
                        });
                    }
                    _ => return,
                }
                this.scroll
                    .scroll_to_item(this.selected, gpui::ScrollStrategy::Nearest);
                cx.stop_propagation();
                cx.notify();
            }))
            .child(
                v_flex()
                    .p_3()
                    .gap_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(Input::new(&self.query).small().disabled(self.replacing))
                    .child(h_flex().gap_1().children(options.into_iter().map(
                        |(label, hint, on)| {
                            Button::new(label)
                                .ghost()
                                .small()
                                .label(label)
                                .tooltip(hint)
                                .when(on, |b| b.bg(cx.theme().accent))
                                .disabled(self.replacing)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    match label {
                                        "Aa" => this.pattern.case_sensitive = !on,
                                        "Ab" => this.pattern.whole_word = !on,
                                        _ => this.pattern.regex = !on,
                                    };
                                    this.search(cx);
                                }))
                        },
                    )))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Input::new(&self.replacement)
                                    .small()
                                    .disabled(self.replacing),
                            )
                            .child(
                                Button::new("replace-all")
                                    .outline()
                                    .small()
                                    .label("Replace all…")
                                    .disabled(
                                        self.replacing
                                            || self.searching
                                            || self.matches.is_empty()
                                            || self.truncated,
                                    )
                                    .on_click(
                                        cx.listener(|this, _, w, cx| this.confirm_replace(w, cx)),
                                    ),
                            ),
                    )
                    .when(self.pattern.regex, |panel| {
                        panel.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Groups: $1, ${name} · Full match: $& · Literal $: $$"),
                        )
                    })
                    .child(Input::new(&self.include).small().disabled(self.replacing))
                    .child(Input::new(&self.exclude).small().disabled(self.replacing)),
            )
            .child(
                div()
                    .id("search-status")
                    .max_h(gpui::rems(8.))
                    .overflow_y_scrollbar()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.status.clone()),
            )
            .child(
                uniform_list(
                    "search-results",
                    self.matches.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|i| {
                                let m = &this.matches[i];
                                let path = m.path.0.clone();
                                let line = m.line.0 as u32;
                                Button::new(gpui::SharedString::from(format!(
                                    "{}:{line}:{}",
                                    path, m.match_start.0
                                )))
                                .ghost()
                                .w_full()
                                .h_12()
                                .when(i == this.selected, |row| row.bg(cx.theme().list_active))
                                .justify_start()
                                .tooltip(format!("{}:{line}\n{}", path, m.line_text.0))
                                .child(
                                    v_flex()
                                        .w_full()
                                        .min_w_0()
                                        .gap_1()
                                        .child(
                                            h_flex()
                                                .gap_2()
                                                .child(crate::icons::file_icon(&path, cx))
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .truncate()
                                                        .child(format!("{path}:{line}")),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_family(cx.theme().mono_font_family.clone())
                                                .truncate()
                                                .child(
                                                    gpui::StyledText::new(m.line_text.0.clone())
                                                        .with_highlights([(
                                                            vitre_state::search::match_range(
                                                                &m.line_text.0,
                                                                m.match_start.0 as usize,
                                                                m.match_end.0 as usize,
                                                            ),
                                                            gpui::HighlightStyle {
                                                                color: Some(cx.theme().primary),
                                                                font_weight: Some(
                                                                    gpui::FontWeight::SEMIBOLD,
                                                                ),
                                                                ..Default::default()
                                                            },
                                                        )]),
                                                ),
                                        ),
                                )
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.emit(SearchEvent::OpenFile {
                                        path: path.clone(),
                                        line,
                                    })
                                }))
                                .into_any_element()
                            })
                            .collect()
                    }),
                )
                .track_scroll(&self.scroll)
                .flex_1()
                .min_h_0(),
            )
            .child(
                h_flex()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("search-refresh")
                            .ghost()
                            .small()
                            .label("Refresh results")
                            .disabled(self.replacing || self.searching)
                            .on_click(cx.listener(|this, _, _, cx| this.search(cx))),
                    ),
            )
    }
}
