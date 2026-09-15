//! Workspace-side plan persistence, including stale-revision protection.
use super::*;
use gpui_component::input::{Input, InputState};
use vitre_contracts::methods::{ProjectsMutateEntry, ProjectsReadFile, ProjectsWriteFile};

pub(super) fn relative_plan_path(value: &str) -> Option<String> {
    let value = value.trim();
    let path = Path::new(value);
    (!value.is_empty()
        && !value.contains(['\\', '\0'])
        && path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_))))
    .then(|| value.to_string())
}

pub(super) struct PlanSaveDialog {
    client: Arc<EnvironmentClient>,
    cwd: String,
    markdown: String,
    path: Entity<InputState>,
    busy: bool,
    error: Option<SharedString>,
    // A failed write can retry the file we created, but never overwrite a
    // pre-existing file or a file subsequently changed by another process.
    created: HashMap<String, Option<Option<TrimmedNonEmptyString>>>,
}

impl PlanSaveDialog {
    pub(super) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(path) = relative_plan_path(&self.path.read(cx).value()) else {
            self.error =
                Some("Use a workspace-relative filename without .. or an absolute path.".into());
            cx.notify();
            return;
        };
        self.busy = true;
        self.error = None;
        cx.notify();
        let client = self.client.clone();
        let cwd = self.cwd.clone();
        let markdown = self.markdown.clone();
        let created = self.created.get(&path).cloned();
        cx.spawn_in(window, async move |this, cx| {
            let result: Result<(), String> = async {
                let revision = if let Some(revision) = created { revision } else {
                    client.call::<ProjectsMutateEntry>(&vitre_contracts::ProjectMutateEntryInput::Create {
                        cwd: tnes(&cwd), relative_path: tnes(&path),
                        kind: vitre_contracts::ProjectMutateEntryInputCreateKind::File,
                    }).await.map_err(|e| e.user_message())?;
                    let read = client.call::<ProjectsReadFile>(&vitre_contracts::ProjectReadFileInput {
                        cwd: tnes(&cwd), relative_path: tnes(&path),
                    }).await.map_err(|e| e.user_message())?;
                    if !read.contents.0.is_empty() { return Err("The new file changed before the plan could be saved. Choose another filename.".into()); }
                    let revision = read.revision;
                    let _ = this.update(cx, |dialog, _| { dialog.created.insert(path.clone(), revision.clone()); });
                    revision
                };
                if revision.as_ref().and_then(|r| r.as_ref()).is_none() {
                    return Err("The server did not return a file revision; refusing an unguarded overwrite.".into());
                }
                client.call::<ProjectsWriteFile>(&vitre_contracts::ProjectWriteFileInput {
                    cwd: tnes(cwd), relative_path: tnes(&path),
                    contents: tnes(format!("{}\n", markdown.trim_end())), base_revision: revision,
                }).await.map_err(|e| e.user_message())?;
                Ok(())
            }.await;
            let _ = this.update_in(cx, |dialog, window, cx| {
                dialog.busy = false;
                match result {
                    Ok(()) => { window.close_dialog(cx); window.push_notification(Notification::success(format!("Plan saved to {path}")), cx); }
                    Err(error) => { dialog.error = Some(format!("Could not save {path}: {error}").into()); cx.notify(); }
                }
            });
        }).detach();
    }
}

impl Render for PlanSaveDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap_3()
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Create a Markdown file in this thread’s workspace. Existing files are never replaced."))
            .child(Input::new(&self.path).disabled(self.busy))
            .children(self.error.clone().map(|error| div().text_sm().text_color(cx.theme().danger).child(error)))
            .child(h_flex().justify_end().child(Button::new("save-workspace-plan").primary()
                .label(if self.busy { "Saving…" } else { "Save plan" }).disabled(self.busy)
                .on_click(cx.listener(|dialog, _, window, cx| dialog.save(window, cx)))))
    }
}

impl ChatApp {
    pub(super) fn save_plan_in_workspace(
        &self,
        markdown: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<PlanSaveDialog>> {
        let (Some(client), Some(cwd)) = (self.client.clone(), self.search_root()) else {
            return None;
        };
        let dialog = cx.new(|cx| PlanSaveDialog {
            client,
            cwd,
            path: cx.new(|cx| InputState::new(window, cx).default_value(plan_filename(&markdown))),
            markdown,
            busy: false,
            error: None,
            created: HashMap::new(),
        });
        let content = dialog.clone();
        window.open_dialog(cx, move |d, _, _| {
            let dialog = content.clone();
            d.title("Save plan to workspace")
                .w(px(480.))
                .content(move |c, _, _| c.child(dialog.clone()))
        });
        Some(dialog)
    }
}

#[cfg(debug_assertions)]
impl PlanSaveDialog {
    pub(super) fn verification_result(&self) -> Option<Result<(), String>> {
        (!self.busy).then(|| self.error.as_ref().map_or(Ok(()), |e| Err(e.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_paths_are_relative_and_cannot_traverse() {
        for bad in [
            "",
            "/tmp/plan.md",
            "../plan.md",
            "plans/../../x",
            "x\\y",
            "a\0b",
        ] {
            assert_eq!(relative_plan_path(bad), None, "{bad}");
        }
        assert_eq!(
            relative_plan_path(" plans/next step.md "),
            Some("plans/next step.md".into())
        );
    }
}
