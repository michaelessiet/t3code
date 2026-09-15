//! Composer state belongs to its thread, including attachments and context.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct ComposerDraft {
    text: String,
    model: Option<ModelSelection>,
    mode: Option<ProviderInteractionMode>,
    attachments: Vec<PendingAttachment>,
    review_comments: Vec<ReviewCommentContext>,
    terminal_contexts: Vec<terminal_contexts::PendingTerminalContext>,
}

enum DraftWrite {
    Save(String, Box<ComposerDraft>),
    Flush(std::sync::mpsc::Sender<()>),
}
pub(super) struct DraftStore {
    directory: PathBuf,
    writes: std::sync::mpsc::Sender<DraftWrite>,
    error: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}
impl DraftStore {
    pub fn new(home: &Path) -> Self {
        let directory = home.join("composer-drafts");
        let (writes, requests) = std::sync::mpsc::channel();
        let error = std::sync::Arc::new(std::sync::Mutex::new(None));
        let worker_dir = directory.clone();
        let worker_error = error.clone();
        // Serialize writes in order, off the render thread. In particular,
        // image attachments must not freeze typing while encoding JSON.
        std::thread::Builder::new()
            .name("vitre-drafts".into())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    match request {
                        DraftWrite::Save(id, draft) => {
                            if let Err(e) = Self::write(&worker_dir, &id, &draft) {
                                *worker_error.lock().unwrap() = Some(e.to_string());
                            }
                        }
                        DraftWrite::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            })
            .expect("start draft persistence worker");
        Self {
            directory,
            writes,
            error,
        }
    }
    fn path(&self, id: &str) -> PathBuf {
        // IDs are opaque and may contain slashes in imported environments.
        self.directory.join(format!(
            "{}.json",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(id)
        ))
    }
    pub fn load(&self, id: &str) -> ComposerDraft {
        self.flush();
        std::fs::read(self.path(id))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, id: &str, draft: &ComposerDraft) -> std::io::Result<()> {
        self.writes
            .send(DraftWrite::Save(id.into(), Box::new(draft.clone())))
            .map_err(|e| std::io::Error::other(e.to_string()))
    }
    pub fn flush(&self) {
        let (done, result) = std::sync::mpsc::channel();
        if self.writes.send(DraftWrite::Flush(done)).is_ok() {
            let _ = result.recv();
        }
    }
    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap().take()
    }
    fn write(directory: &Path, id: &str, draft: &ComposerDraft) -> std::io::Result<()> {
        use std::io::Write as _;
        std::fs::create_dir_all(directory)?;
        let path = directory.join(format!(
            "{}.json",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(id)
        ));
        let temp = path.with_extension("tmp");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        serde_json::to_writer(&mut file, draft)?;
        file.flush()?;
        std::fs::rename(temp, path)
    }
}

impl ChatApp {
    pub(super) fn render_composer_suggestions(
        &self,
        cx: &mut Context<Self>,
    ) -> (Option<AnyElement>, Option<AnyElement>) {
        // @-mention autocomplete rows (top of the composer shell). Rows are
        // plain-text inserts: `@path ` (quoted when the path has spaces).
        let mention_list: Option<gpui::AnyElement> = self
            .mention
            .as_ref()
            .filter(|mention| mention.loading || !mention.results.is_empty())
            .map(|mention| {
                let mut list = v_flex()
                    .w_full()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().secondary);
                if mention.loading {
                    list = list.child(
                        h_flex()
                            .px_3()
                            .py_2()
                            .gap_2()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(Icon::new(IconName::LoaderCircle).size_3p5())
                            .child("Searching workspace…"),
                    );
                }
                for (index, entry) in mention.results.iter().enumerate() {
                    let icon = if entry.kind == ProjectEntryKind::Directory {
                        crate::icons::folder_icon(false, cx)
                    } else {
                        crate::icons::file_icon(&entry.path.0, cx)
                    };
                    list = list.child(
                        h_flex()
                            .id(("mention", index))
                            .px_3()
                            .py_1()
                            .gap_2()
                            .items_center()
                            .cursor_pointer()
                            .text_sm()
                            .when(index == mention.selected, |row| row.bg(cx.theme().accent))
                            .hover(|style| style.bg(cx.theme().accent))
                            .child(icon)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(SharedString::from(entry.path.0.clone())),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.apply_mention(index, window, cx);
                            })),
                    );
                }
                list.into_any_element()
            });
        let command_list: Option<gpui::AnyElement> = self.composer_menu.as_ref().map(|menu| {
            let mut list = v_flex()
                .w_full()
                .max_h(px(300.))
                .overflow_y_scrollbar()
                .py_1()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().secondary);
            if menu.items.is_empty() {
                list = list.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No matching suggestions"),
                );
            }
            for (index, item) in menu.items.iter().enumerate() {
                let (label, description, icon) = match item {
                    ComposerMenuItem::Model => (
                        "/model".to_string(),
                        "Switch response model for this thread".to_string(),
                        IconName::Bot,
                    ),
                    ComposerMenuItem::InteractionMode(ProviderInteractionMode::Plan) => (
                        "/plan".to_string(),
                        "Switch this thread into plan mode".to_string(),
                        IconName::SquarePen,
                    ),
                    ComposerMenuItem::InteractionMode(_) => (
                        "/default".to_string(),
                        "Switch this thread back to normal build mode".to_string(),
                        IconName::Bot,
                    ),
                    ComposerMenuItem::ProviderCommand { name, description } => (
                        format!("/{name}"),
                        description.clone(),
                        IconName::ChevronRight,
                    ),
                    ComposerMenuItem::Thread { title, detail, .. } => {
                        (format!("#{title}"), detail.clone(), IconName::MessageSquare)
                    }
                    ComposerMenuItem::Skill { name, description } => {
                        (format!("${name}"), description.clone(), IconName::SquarePen)
                    }
                };
                list = list.child(
                    h_flex()
                        .id(("composer-command", index))
                        .px_3()
                        .py_1p5()
                        .gap_2()
                        .items_center()
                        .cursor_pointer()
                        .when(index == menu.selected, |row| row.bg(cx.theme().accent))
                        .hover(|style| style.bg(cx.theme().accent))
                        .child(
                            Icon::new(icon)
                                .size_3p5()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().text_sm().truncate().child(SharedString::from(label)))
                                .child(
                                    div()
                                        .text_xs()
                                        .truncate()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(SharedString::from(description)),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(menu) = &mut this.composer_menu {
                                menu.selected = index;
                            }
                            this.apply_active_composer_suggestion(window, cx);
                        })),
                );
            }
            list.into_any_element()
        });
        (mention_list, command_list)
    }
    pub(super) fn ensure_draft_project(&mut self, cx: &mut Context<Self>) {
        if self.thread.is_none()
            && self.draft_project.is_none()
            && let Some(project) = self
                .shell_threads()
                .into_iter()
                .filter(|t| self.sidebar.thread_last_visited(&t.id.0).is_some())
                .max_by(|a, b| {
                    self.sidebar
                        .thread_last_visited(&a.id.0)
                        .cmp(&self.sidebar.thread_last_visited(&b.id.0))
                })
                .map(|t| t.project_id)
                .or_else(|| {
                    self.shell
                        .snapshot
                        .as_ref()
                        .and_then(|s| s.projects.first())
                        .map(|p| p.id.clone())
                })
        {
            self.draft_restore = Some(self.drafts.load(&format!("project:{}", project.0)));
            self.draft_project = Some(project);
            cx.notify();
        }
    }
    pub(super) fn new_thread(
        &mut self,
        project: Option<ProjectId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.draft_creating {
            return;
        }
        let project = project
            .or_else(|| {
                self.thread
                    .as_ref()
                    .and_then(|t| self.shell_thread(&t.id))
                    .map(|t| t.project_id.clone())
            })
            .or_else(|| self.draft_project.clone());
        self.save_composer_draft(cx);
        self.close_open_thread(cx);
        self.draft_project = project;
        self.ensure_draft_project(cx);
        self.draft_restore = self
            .draft_project
            .as_ref()
            .map(|id| self.drafts.load(&format!("project:{}", id.0)));
        self.restore_composer_draft(window, cx);
        self.composer.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }
    pub(super) fn render_draft_landing(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (mentions, commands) = self.render_composer_suggestions(cx);
        let projects = self
            .shell
            .snapshot
            .as_ref()
            .map(|s| s.projects.clone())
            .unwrap_or_default();
        let selected = self
            .draft_project
            .as_ref()
            .and_then(|id| projects.iter().find(|p| &p.id == id));
        let name = selected
            .map(|p| p.title.0.clone())
            .unwrap_or_else(|| "Choose a project".into());
        let owner = cx.entity().downgrade();
        let model = self
            .draft_model
            .clone()
            .or_else(|| selected.and_then(|p| p.default_model_selection.clone()))
            .or_else(|| default_model_selection(&self.provider_snapshots, cx));
        let slug = model.as_ref().and_then(|m| m.model.as_str());
        let instance = model
            .as_ref()
            .and_then(|m| m.instance_id.as_ref())
            .and_then(|v| v.as_ref())
            .and_then(|v| v.as_str());
        let model_info = self
            .provider_snapshots
            .iter()
            .flat_map(|p| p.models.iter().map(move |m| (p, m)))
            .find(|(p, m)| {
                Some(m.slug.0.as_str()) == slug && instance.is_none_or(|id| id == p.instance_id.0)
            });
        let model_label = model_info
            .map(|(_, m)| m.name.0.as_str())
            .or(slug)
            .unwrap_or("Choose model")
            .to_string();
        let picker = self.model_picker.clone();
        let focus = picker.as_ref().map(|p| p.read(cx).query.focus_handle(cx));
        let picker_owner = cx.weak_entity();
        let model_menu =
            gpui_component::popover::Popover::new("draft-model-menu")
                .anchor(gpui::Anchor::BottomLeft)
                .open(picker.is_some())
                .w((window.rem_size() * 32.)
                    .min(window.viewport_size().width - window.rem_size() * 2.))
                .p_1()
                .trigger(
                    Button::new("draft-model")
                        .ghost()
                        .small()
                        .label(model_label)
                        .when_some(model_info, |b, (p, _)| {
                            b.icon(crate::icons::provider_icon(&p.driver.0, cx))
                        })
                        .tooltip("Choose model")
                        .disabled(self.draft_creating),
                )
                .when_some(focus, |p, f| p.track_focus(&f))
                .on_open_change(move |open, w, cx| {
                    let _ = picker_owner.update(cx, |app, cx| {
                        if *open != app.model_picker.is_some() {
                            app.open_model_picker(w, cx);
                        }
                    });
                })
                .content(move |_, _, _| div().children(picker.clone()));
        v_flex().flex_1().min_w_0().h_full().bg(crate::glass::background(cx)).items_center().justify_center().p_6()
            .child(v_flex().w_full().max_w(gpui::rems(48.)).gap_5()
                .child(v_flex().gap_2().child(div().text_2xl().font_semibold().child("What would you like to build?"))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Draft your next conversation. It stays here until you send it.")))
                .when(selected.is_none(),|hero|hero.child(h_flex().child(Button::new("draft-open-folder").primary().label("Open a project folder…").on_click(cx.listener(|app,_,w,cx|app.open_add_project(w,cx))))))
                .child(h_flex().child(Button::new("draft-project").ghost().small().icon(IconName::Folder).label(name).disabled(self.draft_creating).dropdown_menu(move |menu, _, _| {
                    let mut menu = menu;
                    for project in &projects { let owner = owner.clone(); let id = project.id.clone(); menu = menu.item(PopupMenuItem::new(project.title.0.clone()).on_click(move |_, w, cx| {let _ = owner.update(cx, |app, cx| app.new_thread(Some(id.clone()), w, cx));})); } menu
                })))
                .child(v_flex().p_3().gap_3().rounded_xl().border_1().border_color(cx.theme().border).bg(crate::glass::control(cx))
                    .overflow_hidden()
                    .on_drop(cx.listener(|app, paths: &ExternalPaths, w, cx| app.stage_attachment_paths(paths.paths().iter(), w, cx)))
                    .on_drop(cx.listener(|app, context: &crate::files::FileContext, w, cx| app.insert_file_context(context,w,cx)))
                    .capture_key_down(cx.listener(|app, event: &gpui::KeyDownEvent, w, cx| {
                        let modifiers = event.keystroke.modifiers;
                        if event.keystroke.key == "v" && if cfg!(target_os="macos") {modifiers.platform} else {modifiers.control}
                            && let Some(clipboard) = cx.read_from_clipboard() && app.stage_clipboard_item(&clipboard,w,cx) {cx.stop_propagation();return;}
                        let handled = match event.keystroke.key.as_str() {
                            "up" => app.move_composer_suggestion(-1,cx), "down" => app.move_composer_suggestion(1,cx),
                            "escape" if app.mention.is_some() || app.composer_menu.is_some() => {app.mention=None;app.composer_menu=None;cx.notify();true}, _=>false
                        }; if handled {cx.stop_propagation();}
                    }))
                    .children(mentions).children(commands)
                    .child(Textarea::new(&self.composer).appearance(false).disabled(self.draft_creating))
                    .children(self.pending_attachments.iter().enumerate().map(|(i,a)| Button::new(("draft-attachment",i)).ghost().small().label(format!("{} ×",a.name))
                        .on_click(cx.listener(move |app,_,_,cx| {app.pending_attachments.remove(i); app.schedule_draft_save(cx); cx.notify();}))))
                    .child(h_flex().gap_2().flex_wrap().child(model_menu)
                        .child(self.render_mode_switch(self.draft_mode == Some(ProviderInteractionMode::Plan),window,cx))
                        .child(Button::new("draft-attach").ghost().small().icon(IconName::Plus).tooltip("Attach files…").on_click(cx.listener(|app,_,w,cx| app.attach_files(w,cx)))))
                    .child(h_flex().justify_between().gap_3()
                        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(if self.draft_creating {"Starting conversation…"} else {"Enter to send · Shift+Enter for a new line"}))
                        .child(Button::new("draft-send").primary().small().label("Send").disabled(self.draft_creating || self.draft_project.is_none() || (self.composer.read(cx).value().trim().is_empty() && self.pending_attachments.is_empty()))
                            .on_click(cx.listener(|app, _, w, cx| app.send(w, cx))))))
                .children(self.last_error.clone().map(|e| div().text_sm().text_color(cx.theme().danger).child(e))))
            .into_any_element()
    }
    pub(super) fn capture_composer_draft(&self, cx: &Context<Self>) -> ComposerDraft {
        ComposerDraft {
            text: self.composer.read(cx).value().to_string(),
            model: self.draft_model.clone(),
            mode: self.draft_mode.clone(),
            attachments: self.pending_attachments.clone(),
            review_comments: self.pending_review_comments.clone(),
            terminal_contexts: self.pending_terminal_contexts.clone(),
        }
    }
    pub(super) fn insert_file_context(
        &mut self,
        context: &crate::files::FileContext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.thread.is_none() && self.draft_project.is_none() {
            window.push_notification(
                Notification::info("Open a conversation before adding a file."),
                cx,
            );
            return;
        }
        let path = if self.search_root().as_deref() == Some(&context.cwd) {
            context.path.clone()
        } else {
            Path::new(&context.cwd)
                .join(&context.path)
                .to_string_lossy()
                .into_owned()
        };
        let token = if path
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\\')
        {
            format!("@{}", serde_json::to_string(&path).unwrap())
        } else {
            format!("@{path}")
        };
        self.composer.update(cx, |s, cx| {
            let value = s.value();
            s.set_value(
                format!(
                    "{}{}{} ",
                    value,
                    if value.is_empty() || value.ends_with(char::is_whitespace) {
                        ""
                    } else {
                        " "
                    },
                    token
                ),
                window,
                cx,
            );
            s.focus(window, cx);
        });
        self.schedule_draft_save(cx);
        cx.notify();
    }
    pub(super) fn schedule_draft_save(&mut self, cx: &mut Context<Self>) {
        if self.draft_restore.is_some() {
            return;
        }
        let executor = cx.background_executor().clone();
        self.draft_save_task = Some(cx.spawn(async move |this, cx| {
            executor.timer(Duration::from_millis(350)).await;
            let _ = this.update(cx, |this, cx| this.save_composer_draft(cx));
        }));
    }
    pub(super) fn save_composer_draft(&mut self, cx: &mut Context<Self>) {
        if self.draft_restore.is_some() {
            return;
        }
        let Some(id) = self.thread.as_ref().map(|t| t.id.0.clone()).or_else(|| {
            self.draft_project
                .as_ref()
                .map(|p| format!("project:{}", p.0))
        }) else {
            return;
        };
        let draft = self.capture_composer_draft(cx);
        if let Err(e) = self.drafts.save(&id, &draft) {
            self.runtime_notice =
                Some(format!("Could not save the conversation draft: {e}").into());
        }
    }
    pub(super) fn restore_composer_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.draft_restore.take() else {
            return;
        };
        self.composer
            .update(cx, |s, cx| s.set_value(draft.text, window, cx));
        self.pending_attachments = draft.attachments;
        self.pending_review_comments = draft.review_comments;
        self.pending_terminal_contexts = draft.terminal_contexts;
        self.draft_model = draft.model;
        self.draft_mode = draft.mode;
        self.sync_composer_menu(cx);
        self.sync_mention(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_threads_restore_text_and_context_after_restart() {
        let dir = std::env::temp_dir().join(fresh_id("vitre-draft-test"));
        let store = DraftStore::new(&dir);
        store
            .save(
                "env/thread/one",
                &ComposerDraft {
                    text: "keep café\n".into(),
                    attachments: vec![PendingAttachment {
                        name: "one.txt".into(),
                        mime_type: "text/plain".into(),
                        size_bytes: 1,
                        data_url: "data:text/plain;base64,eA==".into(),
                        is_image: false,
                    }],
                    ..Default::default()
                },
            )
            .unwrap();
        store
            .save(
                "two",
                &ComposerDraft {
                    text: "separate".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        store.flush();
        let restored = DraftStore::new(&dir);
        assert_eq!(restored.load("env/thread/one").text, "keep café\n");
        assert_eq!(restored.load("env/thread/one").attachments.len(), 1);
        assert_eq!(restored.load("two").text, "separate");
        assert!(restored.load("missing").text.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
