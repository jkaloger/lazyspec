use crate::tui::state::App;
use crate::tui::state::AppEvent;
use crate::tui::content;
use crate::tui::infra::{perf_log, terminal_caps};
use crate::tui::views;
use crate::engine::config::{Config, StoreBackend};
use crate::engine::document::split_frontmatter;
use crate::engine::gh::GhCli;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store::Store;
use crate::engine::store_dispatch::{DocumentStore, GithubIssuesStore};
use anyhow::Result;
use crossterm::{
    event::Event,
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use notify::{RecursiveMode, Watcher, EventKind};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn run_editor(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    path: &Path,
) -> Result<()> {
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    disable_raw_mode()?;

    let editor = crate::tui::state::resolve_editor();
    let status = Command::new(&editor).arg(path).status();

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;

    if let Err(e) = status {
        eprintln!("Failed to launch editor '{}': {}", editor, e);
    }

    Ok(())
}

fn try_push_gh_edit(
    root: &Path,
    relative: &Path,
    config: &Config,
    shared_store: &Arc<Mutex<GithubIssuesStore<GhCli>>>,
) -> Result<(), String> {
    let content = std::fs::read_to_string(root.join(relative))
        .map_err(|e| format!("failed to read edited file: {e}"))?;

    let (_yaml, body) = split_frontmatter(&content)
        .map_err(|e| format!("failed to parse edited file: {e}"))?;

    let store = Store::load(root, config).map_err(|e| e.to_string())?;
    let doc = store.get(relative).ok_or_else(|| "document not found in store".to_string())?;
    let doc_id = doc.id.clone();
    let type_name = doc.doc_type.as_str().to_string();

    let type_def = config.type_by_name(&type_name)
        .ok_or_else(|| format!("type '{}' not found in config", type_name))?;

    if type_def.store != StoreBackend::GithubIssues {
        return Ok(());
    }

    let body_trimmed = body.trim();
    let mut gh_store = shared_store.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    gh_store.update(type_def, &doc_id, &[("body", body_trimmed)])
        .map_err(|e| e.to_string())
}

fn handle_app_event(app: &mut App, event: AppEvent, root: &Path, config: &Config) {
    match event {
        AppEvent::Terminal(key) => {
            app.handle_key(key.code, key.modifiers, root, config);
        }
        AppEvent::FileChange(event) => {
            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                    let mut has_non_md = false;
                    for path in &event.paths {
                        if path.extension().and_then(|e| e.to_str()) == Some("md") {
                            if let Ok(relative) = path.strip_prefix(root) {
                                let _ = app.store.reload_file(root, relative, &*app.fs);
                                app.expanded_body_cache.remove(relative);
                                app.disk_cache.invalidate(relative);
                            }
                        } else {
                            has_non_md = true;
                        }
                    }
                    if has_non_md {
                        app.expanded_body_cache.clear();
                        app.disk_cache.clear();
                    }
                    app.refresh_validation(config);
                    app.git_status_cache.invalidate();
                }
                _ => {}
            }
        }
        AppEvent::ExpansionResult { path, body, body_hash } => {
            if app.expansion_in_flight.as_ref() == Some(&path) {
                app.expansion_in_flight = None;
            }
            app.disk_cache.write(&path, body_hash, &body);
            app.expanded_body_cache.insert(path, body);
        }
        AppEvent::DiagramRendered { source_hash, entry } => {
            app.diagram_cache.insert(source_hash, entry);
        }
        AppEvent::ProbeResult { picker, protocol, tool_availability } => {
            app.picker = picker;
            app.terminal_image_protocol = protocol;
            app.tool_availability = tool_availability;
            app.diagram_cache = content::diagram::DiagramCache::new();
            app.image_states.clear();
        }
        AppEvent::CacheRefresh => {
            let root = app.store.root().to_path_buf();
            if let Ok(refreshed) = Store::load(&root, config) {
                app.store = refreshed;
            }
            app.last_sync = Some(Instant::now());
            app.filtered_docs_cache = None;
            app.rebuild_search_index();
            app.refresh_validation(config);
        }
        AppEvent::GhPushResult(result) => {
            app.gh_push_in_flight.store(false, Ordering::Relaxed);
            match result {
                Ok(()) => {
                    let root = app.store.root().to_path_buf();
                    if let Ok(refreshed) = Store::load(&root, config) {
                        app.store = refreshed;
                    }
                    app.filtered_docs_cache = None;
                    app.rebuild_search_index();
                    app.refresh_validation(config);
                    app.expanded_body_cache.clear();
                    app.disk_cache.clear();
                }
                Err(msg) => {
                    app.gh_conflict_message = Some(msg);
                }
            }
        }
        AppEvent::CreateStarted => {}
        AppEvent::CreateProgress { message } => {
            if app.create_form.active && app.create_form.loading {
                app.create_form.status_message = Some(message);
            }
        }
        AppEvent::CreateComplete { result } => {
            if !app.create_form.active {
                return;
            }
            match result {
                Ok(create_result) => {
                    let _ = app.store.reload_file(root, &create_result.path, &*app.fs);
                    app.filtered_docs_cache = None;
                    app.rebuild_search_index();
                    if let Some(type_idx) = app.doc_types.iter().position(|t| *t == create_result.doc_type) {
                        app.selected_type = type_idx;
                        app.build_doc_tree();
                        if let Some(doc_idx) = app.doc_tree.iter().position(|n| n.path == create_result.path) {
                            app.selected_doc = doc_idx;
                        }
                    }
                    app.close_create_form();
                    app.refresh_validation(config);
                    app.git_status_cache.invalidate();
                }
                Err(msg) => {
                    app.create_form.loading = false;
                    app.create_form.error = Some(msg);
                    app.create_form.status_message = None;
                }
            }
        }
        #[cfg(feature = "agent")]
        AppEvent::AgentFinished => {}
    }
}

pub fn run(store: Store, config: &Config) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let picker = ratatui_image::picker::Picker::halfblocks();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(store, config, picker, Box::new(crate::engine::fs::RealFileSystem));
    app.refresh_validation(config);

    let (tx, rx) = crossbeam_channel::unbounded();
    app.event_tx = tx.clone();

    // Spawn background probe for terminal image protocol and diagram tool availability
    let probe_tx = tx.clone();
    std::thread::spawn(move || {
        let picker = terminal_caps::create_picker();
        let protocol = terminal_caps::TerminalImageProtocol::from(picker.protocol_type());
        let tool_availability = content::diagram::ToolAvailability::detect();
        let _ = probe_tx.send(AppEvent::ProbeResult { picker, protocol, tool_availability });
    });

    let has_gh_types = config.documents.types.iter()
        .any(|t| t.store == StoreBackend::GithubIssues);

    let shared_gh_store: Option<Arc<Mutex<GithubIssuesStore<GhCli>>>> = if has_gh_types {
        let gh_config = config.documents.github.as_ref();
        let repo = gh_config.and_then(|g| g.repo.clone());
        repo.map(|repo| {
            let root = app.store.root();
            Arc::new(Mutex::new(GithubIssuesStore {
                client: GhCli::new(),
                root: root.to_path_buf(),
                repo,
                config: config.clone(),
                issue_map: IssueMap::load(root).unwrap_or_else(|_| {
                    serde_json::from_str("{}").unwrap()
                }),
                issue_cache: IssueCache::new(root),
            }))
        })
    } else {
        None
    };

    let cache_ttl = config.documents.github.as_ref()
        .map(|g| g.cache_ttl)
        .unwrap_or(60);
    let mut next_poll = if shared_gh_store.is_some() {
        Some(Instant::now())
    } else {
        None
    };
    let refresh_in_flight = Arc::new(AtomicBool::new(false));

    let root = app.store.root().to_path_buf();
    let fs_tx = tx.clone();
    let mut _watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            let _ = fs_tx.send(AppEvent::FileChange(event));
        }
    })?;

    let dirs: Vec<&str> = config.documents.types.iter().map(|t| t.dir.as_str()).collect();
    for dir in &dirs {
        let full = root.join(dir);
        if full.exists() {
            _watcher.watch(&full, RecursiveMode::NonRecursive)?;
        }
    }

    // Dedicated terminal input thread: sends key events through the unified channel
    let input_paused = Arc::new(AtomicBool::new(false));
    let term_tx = tx.clone();
    let paused = input_paused.clone();
    std::thread::spawn(move || {
        loop {
            if paused.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            // Poll with short timeout so we re-check paused frequently
            match crossterm::event::poll(Duration::from_millis(50)) {
                Ok(true) => {
                    if let Ok(Event::Key(key)) = crossterm::event::read() {
                        perf_log::log(&format!("input_thread: read key {:?}", key.code));
                        let _ = term_tx.send(AppEvent::Terminal(key));
                        perf_log::log("input_thread: sent to channel");
                    }
                }
                _ => {}
            }
        }
    });

    let mut loop_count: u64 = 0;
    loop {
        let loop_start = Instant::now();

        let t = Instant::now();
        terminal.draw(|f| views::draw(f, &mut app, config))?;
        perf_log::log_duration("draw", t);

        let t = Instant::now();
        app.request_expansion(&tx);

        if let Some(meta) = app.selected_doc_meta() {
            if let Some(body) = app.expanded_body_cache.get(&meta.path) {
                let body_hash = crate::engine::cache::DiskCache::body_hash(body);
                let blocks = match &app.diagram_blocks_cache {
                    Some((p, h, b)) if p == &meta.path && *h == body_hash => b.clone(),
                    _ => {
                        let b = content::diagram::extract_diagram_blocks(body);
                        app.diagram_blocks_cache = Some((meta.path.clone(), body_hash, b.clone()));
                        b
                    }
                };
                for block in &blocks {
                    app.request_diagram_render(block, &tx);
                }
            }
        }
        perf_log::log_duration("between_frames", t);

        #[cfg(feature = "agent")]
        app.agent_spawner.poll_finished();

        let t = Instant::now();
        match rx.recv_timeout(Duration::from_millis(16)) {
            Ok(event) => {
                perf_log::log_duration("recv_wait", t);
                let t2 = Instant::now();
                let mut event_count = 1u32;
                handle_app_event(&mut app, event, &root, config);
                while let Ok(event) = rx.try_recv() {
                    event_count += 1;
                    handle_app_event(&mut app, event, &root, config);
                }
                perf_log::log_duration(&format!("handle_events({})", event_count), t2);
            }
            Err(_) => {
                perf_log::log_duration("recv_timeout", t);
            }
        }

        if let (Some(deadline), Some(ref shared_store)) = (next_poll, &shared_gh_store) {
            if Instant::now() >= deadline && !refresh_in_flight.load(Ordering::Relaxed) {
                refresh_in_flight.store(true, Ordering::Relaxed);
                next_poll = Some(Instant::now() + Duration::from_secs(cache_ttl));
                let poll_tx = tx.clone();
                let poll_root = root.clone();
                let poll_config = config.clone();
                let poll_flag = refresh_in_flight.clone();
                let poll_store = Arc::clone(shared_store);
                std::thread::spawn(move || {
                    let gh_types: Vec<_> = poll_config.documents.types.iter()
                        .filter(|t| t.store == StoreBackend::GithubIssues)
                        .collect();
                    let all_type_names: Vec<String> = poll_config.documents.types.iter()
                        .map(|t| t.name.clone())
                        .collect();
                    let client = GhCli::new();
                    let mut guard = poll_store.lock().unwrap();
                    let store = &mut *guard;
                    for type_def in &gh_types {
                        if let Err(e) = store.issue_cache.fetch_all(
                            &poll_root,
                            type_def,
                            &client,
                            &store.repo,
                            &mut store.issue_map,
                            &all_type_names,
                        ) {
                            eprintln!("cache refresh failed for {}: {}", type_def.name, e);
                        }
                    }
                    let _ = store.issue_map.save(&poll_root);
                    drop(guard);
                    poll_flag.store(false, Ordering::Relaxed);
                    let _ = poll_tx.send(AppEvent::CacheRefresh);
                });
            }
        }

        loop_count += 1;
        if perf_log::enabled() && loop_count % 60 == 0 {
            perf_log::log(&format!("--- loop #{} ---", loop_count));
        }
        perf_log::log_duration("loop_total", loop_start);

        if let Some(path) = app.editor_request.take() {
            input_paused.store(true, Ordering::Relaxed);
            while rx.try_recv().is_ok() {}
            run_editor(&mut terminal, &path)?;
            while rx.try_recv().is_ok() {}
            input_paused.store(false, Ordering::Relaxed);
            let root = app.store.root().to_path_buf();
            if let Ok(relative) = path.strip_prefix(&root) {
                let _ = app.store.reload_file(&root, relative, &*app.fs);
                app.expanded_body_cache.remove(relative);
                app.disk_cache.invalidate(relative);
                if let Some(ref shared_store) = shared_gh_store {
                    let push_root = root.clone();
                    let push_relative = relative.to_path_buf();
                    let push_config = config.clone();
                    let push_tx = tx.clone();
                    let push_flag = app.gh_push_in_flight.clone();
                    let push_store = Arc::clone(shared_store);
                    push_flag.store(true, Ordering::Relaxed);
                    std::thread::spawn(move || {
                        let result = try_push_gh_edit(&push_root, &push_relative, &push_config, &push_store);
                        push_flag.store(false, Ordering::Relaxed);
                        let _ = push_tx.send(AppEvent::GhPushResult(result));
                    });
                }
            }
            app.refresh_validation(config);
        }

        #[cfg(feature = "agent")]
        if let Some(session_id) = app.resume_request.take() {
            input_paused.store(true, Ordering::Relaxed);
            while rx.try_recv().is_ok() {}

            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            disable_raw_mode()?;
            let _ = Command::new("claude")
                .args(["--resume", &session_id])
                .status();
            enable_raw_mode()?;
            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
            terminal.clear()?;

            while rx.try_recv().is_ok() {}
            input_paused.store(false, Ordering::Relaxed);
            let root = app.store.root().to_path_buf();
            app.store = Store::load(&root, config)?;
            app.refresh_validation(config);
        }

        if app.fix_request {
            app.fix_request = false;
            let root = app.store.root().to_path_buf();
            let paths: Vec<String> = app.store.parse_errors()
                .iter()
                .map(|e| e.path.to_string_lossy().to_string())
                .collect();
            let fs = crate::engine::fs::RealFileSystem;
            let output = crate::cli::fix::run_human(&root, &app.store, config, &paths, false, &fs);
            app.store = Store::load(&root, config)?;
            app.refresh_validation(config);
            app.fix_result = if output.is_empty() { None } else { Some(output) };
            app.warnings_selected = 0;
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}


