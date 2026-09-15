//! Native integration pass against an explicitly disposable sidecar home.
use super::*;
use serde_json::json;
use std::time::Duration;

impl ChatApp {
    pub(super) fn verify_polish_if_requested(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if std::env::var("VITRE_VERIFY_POLISH").as_deref() != Ok("1") {
            return;
        }
        let home = crate::vitre_home();
        if !home
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("vitre-polish-"))
        {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        if self.shell.snapshot.is_none() || STARTED.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let test = async {
                let project = fresh_id("polish-project");
                let cwd = home.join("workspace").join(&project).to_string_lossy().into_owned();
                let thread = ThreadId(fresh_id("polish-thread"));
                for payload in [
                    json!({"type":"project.create","commandId":fresh_id("cmd"),"projectId":project,"title":"Glass workshop","workspaceRoot":cwd,"createWorkspaceRootIfMissing":true,"createdAt":now_iso()}),
                    json!({"type":"thread.create","commandId":fresh_id("cmd"),"projectId":project,"threadId":thread,"title":"Native polish verification","modelSelection":{"provider":"codex","model":"gpt-5"},"runtimeMode":"full-access","interactionMode":"default","branch":null,"worktreePath":null,"createdAt":now_iso()})
                ] {
                    let command = serde_json::from_value(payload).map_err(|e| format!("fixture command: {e}"))?;
                    client.dispatch(&command).await.map_err(|e| e.user_message())?;
                }
                loop {
                    executor.timer(Duration::from_millis(80)).await;
                    if this.update(cx, |a, _| a.shell_thread(&thread).is_some()).map_err(|e| e.to_string())? { break; }
                }
                this.update(cx, |a, cx| a.select_thread(thread.clone(), cx)).map_err(|e| e.to_string())?;
                loop {
                    executor.timer(Duration::from_millis(80)).await;
                    if this.update(cx, |a, _| a.thread.as_ref().is_some_and(|t| t.state.view.is_some())).map_err(|e| e.to_string())? { break; }
                }
                for mode in [ProviderInteractionMode::Plan, ProviderInteractionMode::Default] {
                    this.update(cx, |a, cx| a.set_interaction_mode(mode.clone(), cx)).map_err(|e| e.to_string())?;
                    loop {
                        executor.timer(Duration::from_millis(80)).await;
                        if this.update(cx, |a, _| a.current_interaction_mode(&thread) == mode && a.pending_interaction_mode.is_none()).map_err(|e| e.to_string())? { break; }
                    }
                }
                this.update_in(cx, |a, w, cx| a.open_model_picker(w, cx)).map_err(|e| e.to_string())?;
                executor.timer(Duration::from_millis(500)).await;
                this.update_in(cx, |_, w, cx| w.close_dialog(cx)).map_err(|e| e.to_string())?;
                let markdown = "# Native polish verification\n\nA workspace plan saved through the native dialog.";
                let plan = this.update_in(cx, |a, w, cx| a.save_plan_in_workspace(markdown.into(), w, cx).expect("workspace")).map_err(|e| e.to_string())?;
                executor.timer(Duration::from_millis(400)).await;
                this.update_in(cx, |_, w, cx| plan.update(cx, |p, cx| p.save(w, cx))).map_err(|e| e.to_string())?;
                loop {
                    executor.timer(Duration::from_millis(80)).await;
                    if let Some(result) = plan.update(cx, |p, _| p.verification_result()) { result?; break; }
                }
                let filename = plan_filename(markdown);
                let read = client.call::<vitre_contracts::methods::ProjectsReadFile>(&vitre_contracts::ProjectReadFileInput {cwd:tnes(&cwd),relative_path:tnes(&filename)}).await.map_err(|e|e.user_message())?;
                if read.contents.0.trim() != markdown { return Err("Saved plan content differs".into()); }
                let collision = this.update_in(cx, |a,w,cx|a.save_plan_in_workspace("# Native polish verification\n\nMust not replace the original.".into(),w,cx).unwrap()).map_err(|e|e.to_string())?;
                this.update_in(cx, |_,w,cx|collision.update(cx,|p,cx|p.save(w,cx))).map_err(|e|e.to_string())?;
                loop { executor.timer(Duration::from_millis(80)).await; if let Some(result)=collision.update(cx,|p,_|p.verification_result()) { if result.is_ok(){return Err("Existing plan was overwritten".into());}break; } }
                this.update_in(cx,|a,w,cx| {w.close_dialog(cx);a.dock_open_file(filename.clone(),Some(1),w,cx);}).map_err(|e|e.to_string())?;
                loop { executor.timer(Duration::from_millis(80)).await; if this.update(cx,|a,cx|a.files.as_ref().and_then(|f|f.read(cx).active_file_status()).is_some_and(|(p,_)|p==filename)).map_err(|e|e.to_string())?{break;} }
                this.update_in(cx,|a,w,cx| {if let Some(files)=a.files.clone(){files.update(cx,|f,cx|f.verify_annotation(w,cx));}}).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(600)).await;
                this.update_in(cx,|a,w,cx| {w.close_dialog(cx);a.terminal_toggle(cx);}).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(600)).await;
                this.update(cx,|a,cx|a.terminal_toggle(cx)).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(600)).await;
                this.update_in(cx,|a,w,cx| {a.toggle_right_panel(w,cx);}).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(350)).await;
                this.update(cx,|a,cx| {
                    assert!(!a.activity_orb.read(cx).is_visible(), "Idle orb must not tick");
                    assert!(a.welcome_orb.read(cx).reduced_motion_value(), "Welcome artwork must be static");
                }).map_err(|e|e.to_string())?;
                let previous_reduce_motion = this.update(cx, |_, cx| {let previous=cx.reduce_motion(); cx.set_reduce_motion(false); previous}).map_err(|e|e.to_string())?;
                let orb = this.update_in(cx, |_, window, cx| {
                    let orb = cx.new(|_| vitre_bezel_orbs::orbs::Orb::new().size(vitre_bezel_orbs::orbs::OrbSize::Large).pause_when_inactive(false));
                    let content = orb.clone();
                    window.open_dialog(cx, move |dialog,_,_| { let orb=content.clone();dialog.title("Bezel animation verification").content(move |c,_,_|c.child(orb.clone())) });
                    orb
                }).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(350)).await;
                if !orb.update(cx, |o,_|o.animation_scheduled()) { return Err("Visible orb did not animate".into()); }
                this.update(cx, |_,cx|cx.set_reduce_motion(true)).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(200)).await;
                if orb.update(cx, |o,_|o.animation_scheduled()) { return Err("Reduce Motion did not stop the orb".into()); }
                this.update(cx, |_,cx|cx.set_reduce_motion(false)).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(200)).await;
                orb.update(cx, |o,cx|o.set_visible(false,cx));
                if orb.update(cx, |o,_|o.animation_scheduled()) { return Err("Hidden orb retained its timer".into()); }
                this.update_in(cx,|_,w,cx|{w.close_dialog(cx);cx.set_reduce_motion(previous_reduce_motion);}).map_err(|e|e.to_string())?;
                Ok::<_,String>(())
            };
            let result = tokio::select! { result=test => result, _=executor.timer(Duration::from_secs(45)) => Err("Native polish verification timed out".into()) };
            match result {Ok(())=>eprintln!("[vitre-polish-test] PASS: Build/Plan acknowledgement, model picker, workspace plan save, overwrite rejection, editor annotation dialog, terminal spring, dock reveal, live orb animation, Reduce Motion and hidden timer cancellation"), Err(e)=>eprintln!("[vitre-polish-test] FAIL: {e}")}
        }).detach();
    }
}
