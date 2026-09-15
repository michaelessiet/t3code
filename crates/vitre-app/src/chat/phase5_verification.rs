//! Integrated native acceptance in the disposable icon fixture only.
use super::*;
use vitre_state::right_panel::SurfaceKind;

impl ChatApp {
    pub(super) fn verify_phase5(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |this,cx| {
            let run = async {
                ui_verification::float_fixture(cx).await?;
                cx.update(|w,_|w.resize(gpui::size(px(1440.),px(960.)))).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_secs(5)).await;
                let (thread, project, cwd, other, root2, client) = this.update(cx, |a,_| {
                    let thread = a.thread.as_ref().unwrap().id.clone(); let shell = a.shell_thread(&thread).unwrap();
                    let other = a.shell_threads().into_iter().find(|t| t.id != thread).unwrap();
                    (thread, shell.project_id.clone(), a.search_root().unwrap(), other.id, a.project_root(&other.project_id).unwrap(), a.client.clone().unwrap())
                }).map_err(|e| e.to_string())?;
                this.update_in(cx,|a,w,cx| {a.composer.update(cx,|s,cx|s.set_value("Draft with café and `code`",w,cx));a.save_composer_draft(cx);a.select_thread(other.clone(),cx);a.restore_composer_draft(w,cx);}).map_err(|e|e.to_string())?;
                if this.update(cx,|a,cx|!a.composer.read(cx).value().is_empty()).map_err(|e|e.to_string())? {return Err("Composer leaked across threads".into());}
                this.update_in(cx,|a,w,cx| {a.select_thread(thread.clone(),cx);a.restore_composer_draft(w,cx);}).map_err(|e|e.to_string())?;
                if this.update(cx,|a,cx|a.composer.read(cx).value() != "Draft with café and `code`").map_err(|e|e.to_string())? {return Err("Thread draft was not restored".into());}
                this.update_in(cx,|a,w,cx| a.open_workspace_surface(SurfaceKind::Search,w,cx)).map_err(|e|e.to_string())?;
                let search = this.update(cx,|a,_|a.search_panels[&cwd].clone()).map_err(|e|e.to_string())?;
                cx.update(|w,cx|search.update(cx,|s,cx|s.verify_query("Workspace",w,cx))).map_err(|e|e.to_string())?;
                loop {executor.timer(Duration::from_millis(100)).await;if let Some(result)=search.read_with(cx,|s,_|s.verification_result()){result?;break;}}
                executor.timer(Duration::from_millis(400)).await; ui_verification::capture("phase5-search-dark",cx).await?;
                search.update(cx,|s,cx|s.verify_replace(cx));
                loop {executor.timer(Duration::from_millis(100)).await;if let Some(result)=search.read_with(cx,|s,_|s.verification_replace_done()){if !result.starts_with("Replaced") || result.contains("Skipped"){return Err(result);}break;}}
                let result = client.call::<vitre_contracts::methods::ProjectsReadFile>(&vitre_contracts::ProjectReadFileInput {cwd:tnes(&cwd),relative_path:tnes("src/components/Workspace.tsx")}).await.map_err(|e|e.user_message())?;
                if !result.contents.0.contains("function Studio") {return Err("Search replace did not update the fixture".into());}
                this.update_in(cx,|a,w,cx|a.dock_open_file("README.md".into(),None,w,cx)).map_err(|e|e.to_string())?;
                loop {executor.timer(Duration::from_millis(80)).await;if this.update(cx,|a,cx|a.files.as_ref().and_then(|f|f.read(cx).active_file_status()).is_some_and(|(p,_)|p=="README.md")).map_err(|e|e.to_string())?{break;}}
                this.update_in(cx,|a,w,cx| {a.files.as_ref().unwrap().update(cx,|f,cx| {let n=f.verify_filter("Workspace",w,cx); assert!(n>0 && n<12);f.verify_markdown_preview(cx);});}).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(500)).await;ui_verification::capture("phase5-markdown-tree",cx).await?;
                this.update_in(cx,|a,w,cx|a.files.as_ref().unwrap().update(cx,|f,cx|f.verify_compare(w,cx))).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(700)).await;ui_verification::capture("phase5-compare",cx).await?;
                cx.update(|w,cx|w.close_dialog(cx)).map_err(|e|e.to_string())?;
                this.update_in(cx,|a,w,cx|a.dock_open_file("preview.png".into(),None,w,cx)).map_err(|e|e.to_string())?;
                loop {executor.timer(Duration::from_millis(100)).await;if this.update(cx,|a,cx|a.files.as_ref().and_then(|f|f.read(cx).verify_image_loaded())==Some(true)).map_err(|e|e.to_string())?{break;}}
                executor.timer(Duration::from_millis(400)).await;ui_verification::capture("phase5-image",cx).await?;
                this.update(cx,|a,cx|a.update_workspace_roots(vec![vitre_contracts::WorkspaceRootRef::Path {path:tnes(&root2)}],cx)).map_err(|e|e.to_string())?;
                loop {executor.timer(Duration::from_millis(80)).await;if this.update(cx,|a,_|a.workspace_roots().contains(&root2)).map_err(|e|e.to_string())?{break;}}
                this.update_in(cx,|a,w,cx| {a.active_roots.insert(thread.0.clone(),root2.clone());a.dock_open_file("src/main.rs".into(),None,w,cx);}).map_err(|e|e.to_string())?;
                loop {executor.timer(Duration::from_millis(80)).await;if this.update(cx,|a,cx|a.files.as_ref().is_some_and(|f|f.read(cx).cwd()==root2 && f.read(cx).active_file_status().is_some_and(|(p,_)|p=="src/main.rs"))).map_err(|e|e.to_string())?{break;}}
                this.update(cx,|a,cx|a.snooze_thread(thread.clone(),Some(1),cx)).map_err(|e|e.to_string())?;
                loop {executor.timer(Duration::from_millis(80)).await;if this.update(cx,|a,_|a.shell_thread(&thread).is_some_and(|t|matches!(t.snoozed_until,Some(Some(Some(_)))))).map_err(|e|e.to_string())?{break;}}
                this.update_in(cx,|a,w,cx|{a.snooze_thread(thread.clone(),None,cx);a.open_workspace_surface(SurfaceKind::Graph,w,cx);}).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(900)).await;ui_verification::capture("phase5-graph",cx).await?;
                // Read a real graph through the backend, without installing
                // tools or invoking an external model to manufacture fixture data.
                let graph_project = this.update(cx,|a,_| a.shell_threads().into_iter().find(|t| t.id == other).unwrap().project_id).map_err(|e|e.to_string())?;
                let graph_dir = crate::vitre_home().join("caches/graph").join(&graph_project.0).join("detached-bd97d819/graphify-out");
                std::fs::create_dir_all(&graph_dir).map_err(|e|e.to_string())?;
                let nodes: Vec<_> = (0..12).map(|i| serde_json::json!({"id":format!("node-{i}"),"label":if i==0 {"Workspace".to_string()}else{format!("Workspace module {i}")},"file_type":"code","source_file":"src/main.rs","source_location":"L1","community":i%3,"community_name":(["Interface","State","Services"][i%3])})).collect();
                let links: Vec<_> = (1..12).map(|i| serde_json::json!({"source":"node-0","target":format!("node-{i}"),"relation":"imports","confidence":"EXTRACTED"})).collect();
                std::fs::write(graph_dir.join("graph.json"),serde_json::to_vec(&serde_json::json!({"nodes":nodes,"links":links})).unwrap()).map_err(|e|e.to_string())?;
                client.call::<vitre_contracts::methods::ServerUpdateSettings>(&vitre_contracts::ServerUpdateSettingsPayload {patch:serde_json::from_value(serde_json::json!({"knowledgeGraph":{"enabled":true,"autoRebuild":false}})).unwrap()}).await.map_err(|e|e.user_message())?;
                let graph = this.update(cx,|a,_|a.graph_panels[&root2].clone()).map_err(|e|e.to_string())?;
                cx.update(|w,cx|graph.update(cx,|g,cx|g.verify_query(w,cx))).map_err(|e|e.to_string())?;
                loop {executor.timer(Duration::from_millis(100)).await;if let Some(result)=graph.read_with(cx,|g,_|g.verify_results()){if result?!=12 {return Err("Graph search fixture count did not match".into());}break;}}
                graph.update(cx,|g,cx|g.verify_inspect(cx));
                loop {executor.timer(Duration::from_millis(100)).await;if let Some(result)=graph.read_with(cx,|g,_|g.verify_loaded()){if result?!=12 {return Err("Graph neighbourhood did not load".into());}break;}}
                executor.timer(Duration::from_millis(500)).await;ui_verification::capture("phase5-graph-populated",cx).await?;
                graph.update(cx,|g,cx|g.verify_path(cx));
                loop {executor.timer(Duration::from_millis(100)).await;if let Some(result)=graph.read_with(cx,|g,_|g.verify_loaded()){if result?!=2{return Err("Graph path did not connect the two nodes".into());}break;}}
                std::fs::write(Path::new(&cwd).join("copy-source.txt"),"Native copy fixture").map_err(|e|e.to_string())?;
                this.update(cx,|a,cx|a.copy_workspace_entry(crate::files::FileContext{cwd:cwd.clone(),path:"copy-source.txt".into()},root2.clone(),cx)).map_err(|e|e.to_string())?;
                loop {executor.timer(Duration::from_millis(100)).await;if let Ok(file)=client.call::<vitre_contracts::methods::ProjectsReadFile>(&vitre_contracts::ProjectReadFileInput{cwd:tnes(&root2),relative_path:tnes("copy-source.txt")}).await {if file.contents.0!="Native copy fixture" {return Err("Copy contents differed".into());}break;}}
                let conflict=client.call::<vitre_contracts::methods::ProjectsCopyEntry>(&vitre_contracts::ProjectCopyEntryInput{from_cwd:tnes(&cwd),from_relative_path:tnes("README.md"),to_cwd:tnes(&root2),to_relative_path:tnes("copy-source.txt"),overwrite:Some(Some(false))}).await;
                if conflict.is_ok(){return Err("Copy overwrote an existing destination".into());}
                this.update_in(cx,|a,w,cx|{if a.dock_open(){a.toggle_right_panel(w,cx);}a.new_thread(Some(project),w,cx);a.composer.update(cx,|s,cx|s.set_value("Plan a calm, focused workspace",w,cx));}).map_err(|e|e.to_string())?;
                if this.update(cx,|a,_|a.thread.is_some()).map_err(|e|e.to_string())? {return Err("New conversation created a server thread before sending".into());}
                executor.timer(Duration::from_millis(600)).await;ui_verification::capture("phase5-draft-dark",cx).await?;
                this.update_in(cx,|a,w,cx|{gpui_component::Theme::change(gpui_component::ThemeMode::Light,Some(w),cx);a.open_model_picker(w,cx);}).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(650)).await;ui_verification::capture("phase5-draft-light-model",cx).await?;
                this.update_in(cx,|a,w,_|{a.model_picker=None;w.resize(gpui::size(px(900.),px(680.)));}).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(600)).await;ui_verification::capture("phase5-draft-narrow",cx).await?;
                let viewport=cx.update(|w,_|w.viewport_size()).map_err(|e|e.to_string())?;
                if viewport.width > px(1000.) {return Err(format!("Window manager prevented narrow-window verification: {viewport:?}"));}
                this.update_in(cx,|a,w,cx| {w.resize(gpui::size(px(1440.),px(960.)));gpui_component::Theme::change(gpui_component::ThemeMode::Dark,Some(w),cx);a.select_thread(thread.clone(),cx);a.active_roots.remove(&thread.0);a.dock_open_file("src/components/Workspace.tsx".into(),Some(1),w,cx);}).map_err(|e|e.to_string())?;
                loop {executor.timer(Duration::from_millis(80)).await;if this.update(cx,|a,cx|a.files.as_ref().and_then(|f|f.read(cx).active_file_status()).is_some_and(|(p,_)|p=="src/components/Workspace.tsx")).map_err(|e|e.to_string())?{break;}}
                this.update_in(cx,|a,w,cx|a.verify_surface_ui(w,cx)).map_err(|e|e.to_string())?;
                Ok::<_,String>(())
            };
            let result=tokio::select! {r=run=>r,_=executor.timer(Duration::from_secs(100))=>Err("Phase 5 verification timed out".into())};
            match result {Ok(())=>eprintln!("[vitre-phase5-test] PASS: draft isolation/restore, local new-chat landing, real search/replace RPC, virtual file filter/Markdown, root attach/switch, snooze, graph state and screenshots"),Err(e)=>eprintln!("[vitre-phase5-test] FAIL: {e}")}
        }).detach();
    }
}
