//! M1 live check: `EnvironmentClient` end to end against a supervised
//! sidecar — shell sync goes live, a project + thread are created via typed
//! `orchestration.dispatchCommand`, the thread projection follows a live
//! subscription, and a meta update flows through to the rendered view.
//!
//! Run from the repo root (after `pnpm --filter t3 build:bundle`):
//!   cargo run -p vitre-client --example spike_client
//! Env: T3_SERVER_ENTRY, VITRE_NODE, VITRE_SPIKE_HOME override the defaults.

use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::watch;
use vitre_client::{EnvironmentClient, SyncPhase};
use vitre_contracts::{
    ClientOrchestrationCommand, CommandId, ModelSelection, ProjectCreateCommand, ProjectId,
    RuntimeMode, ThreadId, TrimmedNonEmptyString,
};
use vitre_sidecar::{SidecarConfig, Supervisor};

const PROJECT_ID: &str = "vitre-spike-project-01";
const THREAD_ID: &str = "vitre-spike-thread-01";
const CREATED_AT: &str = "2026-08-27T00:00:00.000Z";

fn tnes(text: &str) -> TrimmedNonEmptyString {
    TrimmedNonEmptyString(text.to_string())
}

fn model_selection() -> ModelSelection {
    ModelSelection {
        instance_id: Some(Some(serde_json::json!("claude"))),
        model: serde_json::json!("claude-sonnet-4-5"),
        options: None,
        provider: None,
    }
}

/// Await a watch channel until `predicate` accepts its value.
async fn wait_until<T: Clone, F: FnMut(&T) -> bool>(
    rx: &mut watch::Receiver<T>,
    what: &str,
    mut predicate: F,
) -> anyhow::Result<T> {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            {
                let value = rx.borrow_and_update();
                if predicate(&value) {
                    return Ok::<T, anyhow::Error>(value.clone());
                }
            }
            rx.changed()
                .await
                .map_err(|_| anyhow::anyhow!("channel closed while waiting for {what}"))?;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("timed out waiting for {what}"))?
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server_entry = PathBuf::from(
        std::env::var("T3_SERVER_ENTRY").unwrap_or_else(|_| "apps/server/dist/bin.mjs".into()),
    );
    anyhow::ensure!(
        server_entry.exists(),
        "server entry {server_entry:?} not found — run `pnpm --filter t3 build:bundle` first"
    );
    let home = std::env::var("VITRE_SPIKE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::temp_dir().join(format!("vitre-spike-client-{}", std::process::id()))
        });
    let workspace_root = home.join("workspace");
    std::fs::create_dir_all(&workspace_root)?;
    println!("[spike] home: {}", home.display());

    let supervisor = Supervisor::start(SidecarConfig {
        node_binary: std::env::var("VITRE_NODE").unwrap_or_else(|_| "node".into()),
        server_entry,
        t3_home: home,
        fixed_port: None,
    });
    let client = EnvironmentClient::start(supervisor.status());

    // 1. Shell goes live (HTTP snapshot + subscribe + completion marker).
    let mut shell = client.shell();
    let state = wait_until(&mut shell, "shell to go live", |state| {
        state.phase == SyncPhase::Live && state.snapshot.is_some()
    })
    .await?;
    println!(
        "[spike] 1/6 shell live (sequence {})",
        state.snapshot.as_ref().unwrap().snapshot_sequence.0
    );

    // 2. Create a project via typed dispatch; the shell projection sees it.
    let result = client
        .dispatch(&ClientOrchestrationCommand::ProjectCreateCommand(
            ProjectCreateCommand {
                additional_roots: None,
                command_id: CommandId("vitre-spike-cmd-project".into()),
                create_workspace_root_if_missing: Some(Some(true)),
                created_at: tnes(CREATED_AT),
                default_model_selection: None,
                project_id: ProjectId(PROJECT_ID.into()),
                title: tnes("Vitre spike project"),
                r#type: Default::default(),
                workspace_root: tnes(workspace_root.to_str().expect("utf8 path")),
            },
        ))
        .await
        .map_err(|error| anyhow::anyhow!("project create failed: {error:?}"))?;
    println!(
        "[spike] 2/6 project created (sequence {})",
        result.sequence.0
    );
    wait_until(&mut shell, "project in the shell", |state| {
        state.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot
                .projects
                .iter()
                .any(|project| project.id.0 == PROJECT_ID)
        })
    })
    .await?;

    // 3. Create a thread; the shell projection lists it.
    let result = client
        .dispatch(&ClientOrchestrationCommand::ThreadCreate {
            additional_roots: None,
            branch: None,
            command_id: CommandId("vitre-spike-cmd-thread".into()),
            created_at: tnes(CREATED_AT),
            interaction_mode: None,
            model_selection: model_selection(),
            project_id: ProjectId(PROJECT_ID.into()),
            runtime_mode: RuntimeMode::FullAccess,
            thread_id: ThreadId(THREAD_ID.into()),
            title: tnes("Vitre spike thread"),
            r#type: Default::default(),
            worktree_path: None,
        })
        .await
        .map_err(|error| anyhow::anyhow!("thread create failed: {error:?}"))?;
    println!(
        "[spike] 3/6 thread created (sequence {})",
        result.sequence.0
    );
    wait_until(&mut shell, "thread in the shell", |state| {
        state.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot
                .threads
                .iter()
                .any(|thread| thread.id.0 == THREAD_ID)
        })
    })
    .await?;

    // 4. Open the thread: HTTP snapshot seed + live subscription.
    let thread = client.open_thread(ThreadId(THREAD_ID.into()));
    let mut thread_state = thread.state();
    let state = wait_until(&mut thread_state, "thread view to go live", |state| {
        state.phase == SyncPhase::Live && state.view.is_some()
    })
    .await?;
    let view = state.view.as_ref().unwrap();
    anyhow::ensure!(view.title.0 == "Vitre spike thread", "unexpected title");
    println!(
        "[spike] 4/6 thread view live — title \"{}\", {} message(s)",
        view.title.0,
        view.messages.len()
    );

    // 5. Meta changes are SHELL-scoped: subscribeThread only carries the six
    //    detail event kinds (`isThreadDetailEvent` in apps/server/src/ws.ts);
    //    title/archive/model flow to detail views by merging the thread shell.
    //    Assert the rename lands in the shell projection.
    client
        .dispatch(&ClientOrchestrationCommand::ThreadMetaUpdate {
            additional_roots: None,
            branch: None,
            command_id: CommandId("vitre-spike-cmd-retitle".into()),
            expected_branch: None,
            model_selection: None,
            thread_id: ThreadId(THREAD_ID.into()),
            title: Some(Some(tnes("Vitre spike thread (renamed)"))),
            r#type: Default::default(),
            worktree_path: None,
        })
        .await
        .map_err(|error| anyhow::anyhow!("meta update failed: {error:?}"))?;
    wait_until(&mut shell, "renamed title in the thread shell", |state| {
        state.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.threads.iter().any(|thread| {
                thread.id.0 == THREAD_ID && thread.title.0 == "Vitre spike thread (renamed)"
            })
        })
    })
    .await?;
    println!("[spike] 5/6 live meta update reached the shell projection");

    // 6. Live DETAIL events flow through the thread projection: a turn start
    //    against the unconfigured provider instance deterministically emits
    //    thread.session-set (error) + thread.activity-appended.
    client
        .dispatch(&ClientOrchestrationCommand::ThreadTurnStart {
            bootstrap: None,
            command_id: CommandId("vitre-spike-cmd-turn".into()),
            created_at: tnes(CREATED_AT),
            interaction_mode: vitre_contracts::ProviderInteractionMode::Default,
            message: vitre_contracts::ClientOrchestrationCommandThreadTurnStartMessage {
                attachments: vec![],
                message_id: vitre_contracts::MessageId("vitre-spike-message-01".into()),
                role: Default::default(),
                text: tnes("hello from the vitre spike"),
            },
            model_selection: None,
            runtime_mode: RuntimeMode::FullAccess,
            source_proposed_plan: None,
            thread_id: ThreadId(THREAD_ID.into()),
            title_seed: None,
            r#type: Default::default(),
        })
        .await
        .map_err(|error| anyhow::anyhow!("turn start failed: {error:?}"))?;
    let state = wait_until(&mut thread_state, "session error in the view", |state| {
        state.view.as_ref().is_some_and(|view| {
            view.session.as_ref().is_some_and(|session| {
                session.status == vitre_contracts::OrchestrationSessionStatus::Error
            }) && !view.activities.is_empty()
        })
    })
    .await?;
    let view = state.view.as_ref().unwrap();
    println!(
        "[spike] 6/6 live detail events reached the thread view — session error + {} activity(ies)",
        view.activities.len()
    );

    drop(thread);
    supervisor.shutdown();
    println!("[spike] PASS — EnvironmentClient shell + thread sync validated live");
    Ok(())
}
