//! Optional creation-ordered sidebar with explicit settle/resume controls.
use super::*;
use gpui_component::menu::{ContextMenuExt as _, DropdownMenu as _};
use vitre_state::sidebar::{ThreadStatus, is_archived, resolve_thread_status};

pub(super) fn settled(thread: &vitre_contracts::OrchestrationThreadShell) -> bool {
    thread
        .settled_at
        .as_ref()
        .and_then(|v| v.as_ref())
        .and_then(|v| v.as_ref())
        .is_some()
}

fn auto_settle_eligible(
    thread: &vitre_contracts::OrchestrationThreadShell,
    cutoff: chrono::DateTime<chrono::Utc>,
) -> bool {
    !thread.has_actionable_proposed_plan
        && !is_archived(thread)
        && !settled(thread)
        && thread
            .settled_override
            .as_ref()
            .and_then(|v| v.as_ref())
            .and_then(|v| v.as_ref())
            .is_none()
        && matches!(
            resolve_thread_status(thread, None),
            None | Some(ThreadStatus::Completed)
        )
        && chrono::DateTime::parse_from_rfc3339(&thread.updated_at.0).is_ok_and(|at| at < cutoff)
}

impl ChatApp {
    pub(super) fn render_beta_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = v_flex().gap_2();
        if let Some(snapshot) = &self.shell.snapshot {
            let mut threads = snapshot
                .threads
                .iter()
                .filter(|t| !is_archived(t))
                .collect::<Vec<_>>();
            threads.sort_by(|a, b| b.created_at.0.cmp(&a.created_at.0));
            for thread in threads {
                let status =
                    resolve_thread_status(thread, self.sidebar.thread_last_visited(&thread.id.0));
                let settled = settled(thread);
                let id = thread.id.clone();
                let project = snapshot
                    .projects
                    .iter()
                    .find(|p| p.id == thread.project_id)
                    .map(|p| p.title.0.clone())
                    .unwrap_or_default();
                let selected = self.thread.as_ref().is_some_and(|t| t.id == thread.id);
                let menu_owner = cx.entity().downgrade();
                let menu_target = self.thread_menu_target(&thread.id);
                let mut card = v_flex()
                    .id(SharedString::from(format!("beta-thread-{}", thread.id.0)))
                    .p_2()
                    .gap_1()
                    .rounded_md()
                    .cursor_pointer()
                    .when(selected, |d| d.bg(cx.theme().list_active))
                    .when(!settled, |d| d.border_1().border_color(cx.theme().border))
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .child(self.render_thread_title(
                                &thread.id,
                                thread.title.0.clone().into(),
                                cx,
                            ))
                            .child(
                                Button::new(SharedString::from(format!(
                                    "beta-menu-{}",
                                    thread.id.0
                                )))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Ellipsis)
                                .tooltip("Conversation actions")
                                .dropdown_menu(
                                    move |menu, window, cx| {
                                        if let Some(target) = &menu_target {
                                            super::thread_actions::thread_context_menu(
                                                menu,
                                                &menu_owner,
                                                target,
                                                true,
                                                window,
                                                cx,
                                            )
                                        } else {
                                            menu
                                        }
                                    },
                                ),
                            ),
                    )
                    .on_click(cx.listener(move |p, _, _, cx| p.select_thread(id.clone(), cx)));
                if !settled {
                    card = card.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{project} · {}",
                                status.map(|s| s.label()).unwrap_or("Ready")
                            )),
                    );
                }
                let id = thread.id.clone();
                card = card.child(
                    Button::new("settle-thread")
                        .label(if settled { "Resume" } else { "Settle" })
                        .ghost()
                        .xsmall()
                        .disabled(matches!(
                            status,
                            Some(
                                ThreadStatus::Working
                                    | ThreadStatus::Connecting
                                    | ThreadStatus::PendingApproval
                                    | ThreadStatus::AwaitingInput
                            )
                        ))
                        .on_click(cx.listener(move |p, _, _, cx| {
                            cx.stop_propagation();
                            p.set_thread_settled(id.clone(), !settled, cx);
                        })),
                );
                let owner = cx.entity().downgrade();
                let target = self.thread_menu_target(&thread.id);
                if self.is_thread_renaming(&thread.id) {
                    list = list.child(card);
                    continue;
                }
                list = list.child(card.context_menu(move |menu, window, cx| {
                    if let Some(target) = &target {
                        super::thread_actions::thread_context_menu(
                            menu, &owner, target, true, window, cx,
                        )
                    } else {
                        menu
                    }
                }));
            }
        }
        list
    }

    pub(super) fn set_thread_settled(
        &mut self,
        id: ThreadId,
        settle: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let mut payload = serde_json::json!({"type":if settle{"thread.settle"}else{"thread.unsettle"},"commandId":fresh_id("settle"),"threadId":id});
        if !settle {
            payload["reason"] = serde_json::json!("user");
        }
        let command = serde_json::from_value(payload).expect("settle command");
        cx.spawn(async move |this, cx| {
            if let Err(e) = client.dispatch(&command).await {
                let _ = this.update(cx, |p, cx| {
                    p.last_error = Some(e.user_message().into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(super) fn sync_auto_settle(&mut self, cx: &mut Context<Self>) {
        if self.auto_settle_task.is_some() {
            return;
        }
        self.auto_settle_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(60))
                    .await;
                if this
                    .update(cx, |p, cx| {
                        let settings = ClientSettings::get(cx);
                        if !settings.sidebar_v2_enabled {
                            return;
                        }
                        let Some(days) = settings.sidebar_auto_settle_after_days else {
                            return;
                        };
                        let cutoff = chrono::Utc::now() - chrono::Duration::days(days.into());
                        let ids = p
                            .shell
                            .snapshot
                            .as_ref()
                            .map(|s| {
                                s.threads
                                    .iter()
                                    .filter(|t| auto_settle_eligible(t, cutoff))
                                    .map(|t| t.id.clone())
                                    .take(20)
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        for id in ids {
                            p.set_thread_settled(id, true, cx);
                        }
                    })
                    .is_err()
                {
                    return;
                }
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_settle_never_hides_actionable_recent_or_archived_threads() {
        let base = serde_json::json!({"id":"test","projectId":"project","title":"Test","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z","runtimeMode":"full-access","modelSelection":{"model":"test"},"hasActionableProposedPlan":false,"hasPendingApprovals":false,"hasPendingUserInput":false});
        let cutoff = chrono::DateTime::parse_from_rfc3339("2026-09-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let thread = serde_json::from_value(base.clone()).unwrap();
        assert!(auto_settle_eligible(&thread, cutoff));
        for (field, value) in [
            ("hasPendingApprovals", serde_json::json!(true)),
            ("hasPendingUserInput", serde_json::json!(true)),
            ("hasActionableProposedPlan", serde_json::json!(true)),
            ("archivedAt", serde_json::json!("2026-01-02T00:00:00Z")),
            ("settledAt", serde_json::json!("2026-01-02T00:00:00Z")),
            ("updatedAt", serde_json::json!("2026-09-02T00:00:00Z")),
            ("updatedAt", serde_json::json!("invalid")),
        ] {
            let mut value_base = base.clone();
            value_base[field] = value;
            let thread = serde_json::from_value(value_base).unwrap();
            assert!(!auto_settle_eligible(&thread, cutoff), "eligible: {field}");
        }
    }
}
