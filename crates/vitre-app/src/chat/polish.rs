//! Native motion and agent-status presentation. Animation never owns task state.
use super::*;
use gpui::{Animation, AnimationExt as _};
use std::time::Duration;
use vitre_bezel_orbs::orbs::{Orb, OrbSize, OrbState};

/// Short, finite entrance; GPUI suppresses travel for Reduce Motion.
pub(super) fn reveal(id: impl Into<gpui::ElementId>, child: impl IntoElement) -> AnyElement {
    div()
        .size_full()
        .child(child)
        .with_animation(
            id,
            Animation::new(Duration::from_millis(220))
                .with_easing(gpui_component::animation::ease_out_cubic),
            |element, progress| {
                element
                    .opacity(progress)
                    .relative()
                    .top(px(6. * (1. - progress)))
            },
        )
        .into_any_element()
}

/// Paused artwork has no animation timer; it is not a false busy indicator.
pub(super) fn welcome_orb(cx: &mut Context<ChatApp>) -> Entity<Orb> {
    cx.new(|_| {
        Orb::new()
            .state(OrbState::Breathing)
            .size(OrbSize::Hero)
            .reduced_motion(true)
    })
}

pub(super) fn activity_orb(cx: &mut Context<ChatApp>) -> Entity<Orb> {
    cx.new(|_| Orb::new().size(OrbSize::Inline).visible(false))
}

impl ChatApp {
    pub(super) fn render_mode_switch(
        &self,
        plan: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let offset = gpui_base::motion::spring(
            "composer-mode-thumb",
            if plan { 3.5f32 } else { 0. },
            gpui_base::motion::Spring::new(Duration::from_millis(260)),
            window,
            cx,
        );
        h_flex()
            .relative()
            .p_0p5()
            .h(gpui::rems(1.875))
            .rounded(gpui::rems(0.5625))
            .border_1()
            .border_color(cx.theme().border.opacity(0.6))
            .child(
                div()
                    .absolute()
                    .left(gpui::rems(0.125 + offset))
                    .top_0p5()
                    .bottom_0p5()
                    .w(gpui::rems(3.5))
                    .rounded(gpui::rems(0.4375))
                    .bg(cx.theme().primary.opacity(0.16)),
            )
            .children(
                [(false, "Build"), (true, "Plan")]
                    .into_iter()
                    .map(|(is_plan, label)| {
                        Button::new(if is_plan {
                            "composer-plan"
                        } else {
                            "composer-build"
                        })
                        .ghost()
                        .small()
                        .w(gpui::rems(3.5))
                        .label(label)
                        .text_color(if is_plan == plan {
                            cx.theme().foreground
                        } else {
                            cx.theme().muted_foreground
                        })
                        .disabled(self.pending_interaction_mode.is_some() || self.draft_creating)
                        .tooltip(if is_plan {
                            "Plan the approach before making changes"
                        } else {
                            "Build and edit with the agent"
                        })
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.set_interaction_mode(
                                if is_plan {
                                    ProviderInteractionMode::Plan
                                } else {
                                    ProviderInteractionMode::Default
                                },
                                cx,
                            )
                        }))
                    }),
            )
            .into_any_element()
    }

    pub(super) fn lifecycle_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let thread = self.shell_thread(&self.thread.as_ref()?.id)?;
        let snoozed = thread
            .snoozed_until
            .as_ref()
            .and_then(|v| v.as_ref())
            .and_then(|v| v.as_ref());
        let settled = super::sidebar_beta::settled(thread);
        if !settled && snoozed.is_none() {
            return None;
        }
        let id = thread.id.clone();
        let snoozed = snoozed.map(|until| until.0.clone());
        let message = snoozed
            .as_ref()
            .map(|until| {
                format!(
                    "Snoozed until {}",
                    chrono::DateTime::parse_from_rfc3339(until)
                        .map(|time| time
                            .with_timezone(&chrono::Local)
                            .format("%a, %b %-d at %-I:%M %p")
                            .to_string())
                        .unwrap_or_else(|_| until.clone())
                )
            })
            .unwrap_or_else(|| {
                "This conversation is settled. Resume whenever you’re ready.".into()
            });
        Some(h_flex().w_full().max_w(px(768.)).mx_auto().p_2().gap_2().rounded_md().bg(cx.theme().muted.opacity(0.65))
            .child(Icon::new(IconName::CircleCheck).size_4().text_color(cx.theme().muted_foreground))
            .child(div().flex_1().min_w_0().text_xs().child(message))
            .child(Button::new("resume-conversation").ghost().small().label("Resume")
                .on_click(cx.listener(move |app, _, window, cx| {
                    if snoozed.is_none() { app.set_thread_settled(id.clone(), false, cx); return; }
                    let Some(client) = app.client.clone() else { return; };
                    let command = ClientOrchestrationCommand::ThreadUnsnooze {
                        command_id: CommandId(fresh_id("unsnooze")), thread_id: id.clone(),
                        reason: vitre_contracts::ClientOrchestrationCommandThreadUnsnoozeReason::User, r#type: Default::default(),
                    };
                    cx.spawn_in(window, async move |_, cx| {
                        if let Err(e) = client.dispatch(&command).await { let _ = cx.update(|window, cx| window.push_notification(Notification::error(e.user_message()), cx)); }
                    }).detach();
                }))).into_any_element())
    }

    pub(super) fn open_editor_comment(
        &self,
        comment: ReviewCommentContext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thread_id) = self.thread.as_ref().map(|t| t.id.clone()) else {
            return;
        };
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("What should the agent change or investigate?")
        });
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let comment = comment.clone();
            let owner = owner.clone();
            let thread_id = thread_id.clone();
            dialog.title(format!("{} · {}", comment.file_path, comment.range_label)).w(px(520.))
                .content(move |content, _, cx| {
                    let input = input.clone(); let comment = comment.clone(); let owner = owner.clone(); let thread_id = thread_id.clone();
                    content.child(v_flex().gap_3()
                        .child(div().max_h(px(180.)).overflow_y_scrollbar().p_3().rounded_md().bg(cx.theme().muted)
                            .font_family(cx.theme().mono_font_family.clone()).text_xs().child(comment.diff.clone()))
                        .child(Textarea::new(&input))
                        .child(Button::new("editor-review-add").primary().label("Add to conversation")
                            .on_click(move |_, window, cx| {
                                let text = input.read(cx).value().trim().to_string();
                                if text.is_empty() { return; }
                                let mut comment = comment.clone(); comment.text = text;
                                let added = owner.update(cx, |app, cx| {
                                    if app.thread.as_ref().is_none_or(|t| t.id != thread_id) { return false; }
                                    app.add_review_comment(comment, cx); true
                                }).unwrap_or(false);
                                if added { window.close_dialog(cx); } else { window.push_notification(Notification::warning("Return to the original thread to add this comment."), cx); }
                            })))
                })
        });
    }

    pub(super) fn sync_activity_orb(&mut self, cx: &mut Context<Self>) {
        let view = self.thread.as_ref().and_then(|t| t.state.view.as_ref());
        let waiting = view.is_some_and(|v| {
            !derive_pending_approvals(&v.activities).is_empty()
                || !derive_pending_user_inputs(&v.activities).is_empty()
        });
        let running = self.local_dispatch.is_some()
            || view.is_some_and(|v| {
                v.session
                    .as_ref()
                    .is_some_and(|s| s.status == OrchestrationSessionStatus::Running)
            });
        let composing = self
            .display_messages
            .last()
            .is_some_and(|m| m.role == OrchestrationMessageRole::Assistant && m.streaming);
        let plan = self
            .thread
            .as_ref()
            .is_some_and(|t| self.current_interaction_mode(&t.id) == ProviderInteractionMode::Plan);
        self.activity_orb.update(cx, |orb, cx| {
            orb.set_state(
                if composing {
                    OrbState::Composing
                } else if plan {
                    OrbState::Reasoning
                } else {
                    OrbState::Working
                },
                cx,
            );
            orb.set_paused(waiting, cx);
            orb.set_visible(
                self.settings.is_none() && self.thread.is_some() && running,
                cx,
            );
        });
    }

    pub(super) fn render_welcome(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex().size_full().items_center().justify_center().gap_4().px_8()
            .child(self.welcome_orb.clone())
            .child(v_flex().items_center().gap_2()
                .child(div().text_size(px(26.)).font_semibold().child("Make room for your next idea."))
                .child(div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("Explore a question. Shape a plan. Build something useful.")))
            .child(h_flex().gap_2().mt_2().flex_wrap().justify_center()
                .child(Button::new("welcome-explore").icon(IconName::Search).label("Explore this project")
                    .outline().on_click(cx.listener(|app, _, window, cx| {
                        app.composer.update(cx, |input, cx| {
                            let draft = input.value().to_string();
                            let prompt = "Explain this project's architecture and suggest where to start.";
                            input.set_value(if draft.trim().is_empty() { prompt.into() } else { format!("{draft}\n\n{prompt}") }, window, cx);
                            input.focus(window, cx);
                        });
                    })))
                .child(Button::new("welcome-plan").icon(IconName::SquarePen).label("Shape a plan")
                    .outline().on_click(cx.listener(|app, _, window, cx| {
                        app.set_interaction_mode(ProviderInteractionMode::Plan, cx);
                        app.composer.update(cx, |input, cx| { if input.value().trim().is_empty() { input.set_value("Help me plan ", window, cx); } input.focus(window, cx); });
                    }))))
            .child(div().text_xs().text_color(cx.theme().muted_foreground.opacity(0.65))
                .child("@ files    ·    / commands    ·    ⇧↵ new line"))
            .into_any_element()
    }
}
