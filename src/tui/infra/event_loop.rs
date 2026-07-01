use crate::engine::config::{Config, StoreBackend};
use crate::engine::document::split_frontmatter;
use crate::engine::gh::GhCli;
use crate::engine::git_ref::GitCli;
use crate::engine::git_ref_store::GitRefStore;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store::Store;
use crate::engine::store_dispatch::{DocumentStore, GithubIssuesStore};
use crate::tui::content;
use crate::tui::infra::{perf_log, terminal_caps};
use crate::tui::state::App;
use crate::tui::state::AppEvent;
use crate::tui::views;
use anyhow::Result;
use crossterm::{
    event::{Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use notify::{EventKind, RecursiveMode, Watcher};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

// Discard any pending crossterm events buffered in stdin. Called after a child
// process (editor, agent) exits to drop bytes that may have arrived during the
// subprocess but were not consumed by it. Caller must hold the stdin lock so the
// input thread does not race the reads.
fn drain_stdin() {
    while let Ok(true) = crossterm::event::poll(Duration::from_millis(0)) {
        let _ = crossterm::event::read();
    }
}

fn run_editor(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, path: &Path) -> Result<()> {
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

    let (_yaml, body) =
        split_frontmatter(&content).map_err(|e| format!("failed to parse edited file: {e}"))?;

    let store = Store::load(root, config).map_err(|e| e.to_string())?;
    let doc = store
        .get(relative)
        .ok_or_else(|| "document not found in store".to_string())?;
    let doc_id = doc.id.clone();
    let type_name = doc.doc_type.as_str().to_string();

    let type_def = config
        .type_by_name(&type_name)
        .ok_or_else(|| format!("type '{}' not found in config", type_name))?;

    if type_def.store != StoreBackend::GithubIssues {
        return Ok(());
    }

    let body_trimmed = body.trim();
    let mut gh_store = shared_store
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;
    gh_store
        .update(type_def, &doc_id, &[("body", body_trimmed)])
        .map_err(|e| e.to_string())
}

fn try_push_git_ref_edit(root: &Path, relative: &Path, config: &Config) -> Result<(), String> {
    let store = Store::load(root, config).map_err(|e| e.to_string())?;
    let doc = store
        .get(relative)
        .ok_or_else(|| "document not found in store".to_string())?;
    let doc_id = doc.id.clone();
    let type_name = doc.doc_type.as_str().to_string();

    let type_def = config
        .type_by_name(&type_name)
        .ok_or_else(|| format!("type '{}' not found in config", type_name))?;

    if type_def.store != StoreBackend::GitRef {
        return Ok(());
    }

    let mut git_store = GitRefStore {
        git: GitCli,
        root: root.to_path_buf(),
        config: config.clone(),
        reserved_number: None,
    };
    git_store
        .update(type_def, &doc_id, &[])
        .map_err(|e| e.to_string())
}

// Whether the background poll should run for this project: true when any type is
// backed by a GitHub store the poll refreshes (issues or milestones). Milestone-
// only projects still need the poll so a milestone created after launch appears
// live in the list.
fn has_pollable_gh_types(config: &Config) -> bool {
    config
        .documents
        .types
        .iter()
        .any(|t| t.store == StoreBackend::GithubIssues || t.store == StoreBackend::GithubMilestones)
}

// Rebuild the watcher over the current config's watch set. `notify` has no
// reliable cross-reload "unwatch all" when the watched dirs change, so we
// replace the watcher wholesale: a fresh watcher is constructed and the old one
// dropped, which stops all of its prior watches. Used by both startup and reload.
fn rewatch(
    watcher: &mut notify::RecommendedWatcher,
    root: &Path,
    config: &Config,
    tx: &crossbeam_channel::Sender<AppEvent>,
) -> Result<()> {
    let fs_tx = tx.clone();
    let mut new_watcher =
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                let _ = fs_tx.send(AppEvent::FileChange(event));
            }
        })?;
    for path in crate::engine::watch::watch_paths(root, config) {
        new_watcher.watch(&path, RecursiveMode::NonRecursive)?;
    }
    *watcher = new_watcher;
    Ok(())
}

// Re-load `Config` from `.lazyspec.toml`, rebuild the `Store`, refresh all
// config-derived App caches, and re-establish the watcher. The whole iteration
// is built on this single primitive.
//
// AC8: a failed reload leaves the running session completely intact. Both
// fallible loads (Config, Store) are computed into locals BEFORE any state is
// mutated; an early `Err` therefore leaves `*config`, `app`, and `*watcher`
// untouched, so the previous Config/Store/watch set stay in effect.
//
// AC7: no redraw flag is needed -- `run`'s loop calls `terminal.draw` every
// iteration, so a reload performed inside the loop body is rendered next pass.
//
// Driven by `App::config_reload_request`: the FileChange arm (external
// `.lazyspec.toml` edit) and the manual reload keybinding both set the flag,
// which the `run` loop drains and dispatches here.
fn reload_session(
    app: &mut App,
    config: &mut Config,
    watcher: &mut notify::RecommendedWatcher,
    root: &Path,
    tx: &crossbeam_channel::Sender<AppEvent>,
) -> Result<()> {
    // 1. Re-parse Config (strict). On Err, nothing is mutated (AC8).
    let new_config = Config::load(root, &crate::engine::fs::RealFileSystem)?;
    // 2. Rebuild Store against the new Config. On Err, still nothing mutated (AC8).
    let new_store = Store::load(root, &new_config)?;

    // 3. Commit: both loads succeeded, so it is now safe to mutate.
    *config = new_config;
    app.store = new_store;
    app.apply_config(config);
    app.refresh_validation(config);
    // Mirror the cache resets the CacheRefresh and GhPushResult(Ok) arms perform.
    app.filtered_docs_cache = None;
    app.rebuild_search_index();
    app.build_doc_tree();
    app.git_status_cache.invalidate();
    app.expanded_body_cache.clear();
    app.disk_cache.clear();

    // 4. Re-establish the watcher over the new config's watch set (AC4, AC5).
    rewatch(watcher, root, config, tx)?;
    Ok(())
}

fn handle_app_event(app: &mut App, event: AppEvent, root: &Path, config: &Config) {
    match event {
        AppEvent::Terminal(key) => {
            app.handle_key(key.code, key.modifiers, root, config);
        }
        AppEvent::FileChange(event) => match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                let mut has_non_md = false;
                let config_path = root.join(".lazyspec.toml");
                for path in &event.paths {
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Ok(relative) = path.strip_prefix(root) {
                            let _ = app.store.reload_file(root, relative, &*app.fs);
                            app.expanded_body_cache.remove(relative);
                            app.disk_cache.invalidate(relative);
                        }
                    } else {
                        has_non_md = true;
                        // The root `.lazyspec.toml` changed externally (e.g. a
                        // `git pull`). Request a full session reload; the `run`
                        // loop drains this flag and calls `reload_session`.
                        // `handle_app_event` only holds `&Config` and no
                        // `&mut watcher`, so it cannot reload directly.
                        if path == &config_path {
                            app.config_reload_request = true;
                        }
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
        },
        AppEvent::ExpansionResult {
            path,
            body,
            body_hash,
        } => {
            if app.expansion_in_flight.as_ref() == Some(&path) {
                app.expansion_in_flight = None;
            }
            app.disk_cache.write(&path, body_hash, &body);
            app.expanded_body_cache.insert(path, body);
        }
        AppEvent::DiagramRendered { source_hash, entry } => {
            app.diagram_cache.insert(source_hash, entry);
        }
        AppEvent::CacheRefresh { warnings } => {
            let root = app.store.root().to_path_buf();
            if let Ok(refreshed) = Store::load(&root, config) {
                app.store = refreshed;
            }
            app.last_sync = Some(Instant::now());
            app.gh_fetch_warnings = warnings;
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
                    if let Some(type_idx) = app
                        .doc_types
                        .iter()
                        .position(|t| *t == create_result.doc_type)
                    {
                        app.selected_type = type_idx;
                        app.build_doc_tree();
                        if let Some(doc_idx) = app
                            .doc_tree
                            .iter()
                            .position(|n| n.path == create_result.path)
                        {
                            app.selected_doc = doc_idx;
                        }
                    }
                    app.close_create_form();
                    app.refresh_validation(config);
                    app.git_status_cache.invalidate();
                    app.gh_issue_map_stale = true;
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
    // Owned, reassignable session config: `reload_session` re-parses
    // `.lazyspec.toml` and rebinds this so subsequent reads see it.
    let mut config: Config = config.clone();

    // Probe terminal capabilities BEFORE entering raw mode and spawning the input thread.
    // `Picker::from_query_stdio` reads stdin directly to capture terminal capability responses;
    // running it concurrently with the crossterm input thread races for stdin and silently
    // consumes user keystrokes (the parser eats bytes looking for the DSR Status terminator).
    let picker = terminal_caps::create_picker();
    let protocol = terminal_caps::TerminalImageProtocol::from(picker.protocol_type());
    let tool_availability = content::diagram::ToolAvailability::detect();

    // Restore the terminal on panic. Without this, an unwinding panic skips the
    // teardown at the end of `run`, leaving raw mode and the alternate screen
    // active: the panic message overwrites the UI and the cursor is left
    // mispositioned. Chain the original hook so the message still prints.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(
        store,
        &config,
        picker,
        Box::new(crate::engine::fs::RealFileSystem),
    );
    app.terminal_image_protocol = protocol;
    app.tool_availability = tool_availability;
    app.refresh_validation(&config);

    let (tx, rx) = crossbeam_channel::unbounded();
    app.event_tx = tx.clone();

    let shared_gh_store: Option<Arc<Mutex<GithubIssuesStore<GhCli>>>> =
        if has_pollable_gh_types(&config) {
            let gh_config = config.documents.github.as_ref();
            let repo = gh_config.and_then(|g| g.repo.clone());
            repo.map(|repo| {
                let root = app.store.root();
                Arc::new(Mutex::new(GithubIssuesStore {
                    client: GhCli::new(),
                    root: root.to_path_buf(),
                    repo,
                    config: config.clone(),
                    issue_map: IssueMap::load(root)
                        .unwrap_or_else(|_| serde_json::from_str("{}").unwrap()),
                    issue_cache: IssueCache::new(root),
                }))
            })
        } else {
            None
        };

    let cache_ttl = config
        .documents
        .github
        .as_ref()
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
    // Route startup through the same helper reload uses, so `.lazyspec.toml` is
    // watched from startup (AC5) and both paths share one watch-set source.
    rewatch(&mut _watcher, &root, &config, &tx)?;

    // Dedicated terminal input thread: sends key events through the unified channel.
    // The mutex enforces single-reader ownership of stdin: the main thread acquires it
    // before disabling raw mode for an external editor / subprocess, guaranteeing that
    // no concurrent crossterm poll consumes bytes meant for the child process.
    let stdin_lock = Arc::new(Mutex::new(()));
    let term_tx = tx.clone();
    let thread_stdin_lock = stdin_lock.clone();
    std::thread::spawn(move || loop {
        // The lock guards `()`: a poisoned mutex carries no corrupt state, so
        // recover the guard rather than panicking and silently killing input.
        let _guard = thread_stdin_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Ok(true) = crossterm::event::poll(Duration::from_millis(50)) {
            if let Ok(Event::Key(key)) = crossterm::event::read() {
                if key.kind == KeyEventKind::Press {
                    perf_log::log(&format!("input_thread: read key {:?}", key.code));
                    let _ = term_tx.send(AppEvent::Terminal(key));
                    perf_log::log("input_thread: sent to channel");
                }
            }
        }
        drop(_guard);
        std::thread::yield_now();
    });

    let mut loop_count: u64 = 0;
    loop {
        let loop_start = Instant::now();

        let t = Instant::now();
        terminal.draw(|f| views::draw(f, &mut app, &config))?;
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
                handle_app_event(&mut app, event, &root, &config);
                while let Ok(event) = rx.try_recv() {
                    event_count += 1;
                    handle_app_event(&mut app, event, &root, &config);
                }
                perf_log::log_duration(&format!("handle_events({})", event_count), t2);
            }
            Err(_) => {
                perf_log::log_duration("recv_timeout", t);
            }
        }

        if app.gh_issue_map_stale {
            if let Some(ref shared_store) = shared_gh_store {
                if let Ok(mut guard) = shared_store.lock() {
                    if let Ok(map) = IssueMap::load(&root) {
                        guard.issue_map = map;
                    }
                }
            }
            app.gh_issue_map_stale = false;
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
                    let gh_types: Vec<_> = poll_config
                        .documents
                        .types
                        .iter()
                        .filter(|t| t.store == StoreBackend::GithubIssues)
                        .collect();
                    let milestone_types: Vec<_> = poll_config
                        .documents
                        .types
                        .iter()
                        .filter(|t| t.store == StoreBackend::GithubMilestones)
                        .collect();
                    let all_type_names: Vec<String> = poll_config
                        .documents
                        .types
                        .iter()
                        .map(|t| t.name.clone())
                        .collect();
                    let client = GhCli::new();
                    let mut guard = poll_store.lock().unwrap();
                    let store = &mut *guard;
                    let mut warnings: Vec<String> = Vec::new();
                    // Milestones MUST be fetched before issues: an issue's native
                    // milestone is surfaced as a forward `targets: MILESTONE-n`
                    // relation by resolving the milestone number through the
                    // issue-map, so the milestone has to be mapped first or the
                    // lookup silently drops the relation on a fresh poll.
                    for type_def in &milestone_types {
                        match crate::engine::milestone_cache::fetch_milestones(
                            &poll_root,
                            type_def,
                            &client,
                            &store.repo,
                            &mut store.issue_map,
                        ) {
                            Ok(result) => {
                                warnings.extend(result.warnings.into_iter().map(|w| w.message));
                            }
                            Err(e) => {
                                warnings.push(format!(
                                    "milestone cache refresh failed for {}: {}",
                                    type_def.name, e
                                ));
                            }
                        }
                    }
                    for type_def in &gh_types {
                        match store.issue_cache.fetch_all(
                            &poll_root,
                            type_def,
                            &client,
                            &client,
                            &store.repo,
                            &mut store.issue_map,
                            &all_type_names,
                            &poll_config,
                        ) {
                            Ok(result) => {
                                warnings.extend(result.warnings.into_iter().map(|w| w.message));
                            }
                            Err(e) => {
                                warnings.push(format!(
                                    "cache refresh failed for {}: {}",
                                    type_def.name, e
                                ));
                            }
                        }
                    }
                    let _ = store.issue_map.save(&poll_root);
                    drop(guard);
                    poll_flag.store(false, Ordering::Relaxed);
                    let _ = poll_tx.send(AppEvent::CacheRefresh { warnings });
                });
            }
        }

        loop_count += 1;
        if perf_log::enabled() && loop_count.is_multiple_of(60) {
            perf_log::log(&format!("--- loop #{} ---", loop_count));
        }
        perf_log::log_duration("loop_total", loop_start);

        if let Some(path) = app.editor_request.take() {
            let _stdin_guard = stdin_lock.lock().unwrap_or_else(PoisonError::into_inner);
            while rx.try_recv().is_ok() {}
            run_editor(&mut terminal, &path)?;
            drain_stdin();
            while rx.try_recv().is_ok() {}
            drop(_stdin_guard);
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
                        let result =
                            try_push_gh_edit(&push_root, &push_relative, &push_config, &push_store);
                        push_flag.store(false, Ordering::Relaxed);
                        let _ = push_tx.send(AppEvent::GhPushResult(result));
                    });
                }
                {
                    let push_root = root.clone();
                    let push_relative = relative.to_path_buf();
                    let push_config = config.clone();
                    let push_tx = tx.clone();
                    std::thread::spawn(move || {
                        let result =
                            try_push_git_ref_edit(&push_root, &push_relative, &push_config);
                        if let Err(msg) = result {
                            let _ = push_tx.send(AppEvent::GhPushResult(Err(msg)));
                        }
                    });
                }
            }
            app.refresh_validation(&config);
        }

        #[cfg(feature = "agent")]
        if let Some(session_id) = app.resume_request.take() {
            let _stdin_guard = stdin_lock.lock().unwrap_or_else(PoisonError::into_inner);
            while rx.try_recv().is_ok() {}

            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            disable_raw_mode()?;
            let _ = Command::new("claude")
                .args(["--resume", &session_id])
                .status();
            enable_raw_mode()?;
            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
            terminal.clear()?;

            drain_stdin();
            while rx.try_recv().is_ok() {}
            drop(_stdin_guard);
            let root = app.store.root().to_path_buf();
            app.store = Store::load(&root, &config)?;
            app.refresh_validation(&config);
        }

        #[cfg(feature = "agent")]
        if let Some(req) = app.interactive_request.take() {
            let _stdin_guard = stdin_lock.lock().unwrap_or_else(PoisonError::into_inner);
            while rx.try_recv().is_ok() {}

            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            disable_raw_mode()?;
            let mut command = crate::engine::agent_interactive::build_interactive_command(
                &req.cmd,
                &req.prompt,
                &req.doc_path,
            );
            let _ = command.status();
            enable_raw_mode()?;
            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
            terminal.clear()?;

            drain_stdin();
            while rx.try_recv().is_ok() {}
            drop(_stdin_guard);
            let root = app.store.root().to_path_buf();
            app.store = Store::load(&root, &config)?;
            app.refresh_validation(&config);
        }

        if app.fix_request {
            app.fix_request = false;
            let root = app.store.root().to_path_buf();
            let paths: Vec<String> = app
                .store
                .parse_errors()
                .iter()
                .map(|e| e.path.to_string_lossy().to_string())
                .collect();
            let fs = crate::engine::fs::RealFileSystem;
            let output = crate::cli::fix::run_human(&root, &app.store, &config, &paths, false, &fs);
            app.store = Store::load(&root, &config)?;
            app.refresh_validation(&config);
            app.fix_result = if output.is_empty() {
                None
            } else {
                Some(output)
            };
            app.warnings_selected = 0;
        }

        if app.config_reload_request {
            app.config_reload_request = false;
            // TODO(slice-3): gate on dirty buffer; prompt keep/discard when edits unsaved.
            // There is no dirty edit buffer this iteration, so the buffer is always
            // clean -> honor the reload unconditionally (AC6).
            //
            // AC8: a failed reload must not kill the session. `reload_session`
            // mutates nothing on Err, so swallow it here and keep looping; the
            // previous Config/Store/watch set remain in effect.
            if let Err(e) = reload_session(&mut app, &mut config, &mut _watcher, &root, &tx) {
                perf_log::log(&format!("config reload failed: {e}"));
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::TypeDef;
    use tempfile::TempDir;

    // Gate: a project whose only GitHub-backed type is github-milestones must
    // still poll, so a milestone created after launch appears live in the list.
    #[test]
    fn milestone_only_project_is_pollable() {
        let mut config = Config::default();
        config.documents.types = vec![TypeDef::test_fixture(
            "milestone",
            StoreBackend::GithubMilestones,
        )];

        assert!(has_pollable_gh_types(&config));
    }

    // Gate: a project with no GitHub-backed types must not poll.
    #[test]
    fn project_without_gh_types_is_not_pollable() {
        let mut config = Config::default();
        config.documents.types = vec![
            TypeDef::test_fixture("doc", StoreBackend::Filesystem),
            TypeDef::test_fixture("note", StoreBackend::Filesystem),
        ];

        assert!(!has_pollable_gh_types(&config));
    }

    // Build an App over `root` with the given config, using a deterministic
    // halfblocks picker so no terminal probing happens in tests.
    fn make_app(root: &Path, config: &Config) -> App {
        let store = Store::load(root, config).unwrap();
        let mut app = App::new(
            store,
            config,
            ratatui_image::picker::Picker::halfblocks(),
            Box::new(crate::engine::fs::RealFileSystem),
        );
        app.refresh_validation(config);
        app
    }

    // A real notify watcher; never awaited, so it cannot make tests flaky.
    fn make_watcher(tx: &crossbeam_channel::Sender<AppEvent>) -> notify::RecommendedWatcher {
        let fs_tx = tx.clone();
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                let _ = fs_tx.send(AppEvent::FileChange(event));
            }
        })
        .unwrap()
    }

    // A valid config carrying `count` distinct filesystem doc types, written via
    // `to_toml` so it round-trips through strict `Config::parse`.
    fn valid_config_toml(count: usize) -> String {
        let mut config = Config::default();
        config.documents.types = (0..count)
            .map(|i| {
                let name = format!("type{i}");
                let mut t = TypeDef::test_fixture(&name, StoreBackend::Filesystem);
                t.dir = format!("docs/{name}");
                t
            })
            .collect();
        config.to_toml().unwrap()
    }

    // AC2/AC3: the success path makes the new Config and Store active and
    // refreshes config-derived App caches (here, `doc_types`).
    #[test]
    fn reload_session_activates_new_config_and_store() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".lazyspec.toml"), valid_config_toml(1)).unwrap();

        let mut config = Config::load(root, &crate::engine::fs::RealFileSystem).unwrap();
        let mut app = make_app(root, &config);
        let (tx, _rx) = crossbeam_channel::unbounded();
        let mut watcher = make_watcher(&tx);

        assert_eq!(config.documents.types.len(), 1);
        assert_eq!(app.doc_types.len(), 1);

        // Config B: two types.
        std::fs::write(root.join(".lazyspec.toml"), valid_config_toml(2)).unwrap();

        reload_session(&mut app, &mut config, &mut watcher, root, &tx).unwrap();

        assert_eq!(
            config.documents.types.len(),
            2,
            "config B should be active after reload"
        );
        assert_eq!(
            app.doc_types.len(),
            2,
            "App caches should reflect config B after reload"
        );
        assert_eq!(app.doc_types[0].as_str(), "type0");
        assert_eq!(app.doc_types[1].as_str(), "type1");
    }

    // AC8: a Config parse error leaves `*config`, `app.store`, and the
    // config-derived caches untouched (still config A).
    #[test]
    fn reload_session_invalid_config_leaves_state_intact() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".lazyspec.toml"), valid_config_toml(1)).unwrap();

        let mut config = Config::load(root, &crate::engine::fs::RealFileSystem).unwrap();
        let mut app = make_app(root, &config);
        let (tx, _rx) = crossbeam_channel::unbounded();
        let mut watcher = make_watcher(&tx);

        let docs_before = app.store.list(&Default::default()).len();

        // Config B: malformed TOML -- strict parse fails.
        std::fs::write(
            root.join(".lazyspec.toml"),
            "this is not = valid = toml [[[",
        )
        .unwrap();

        let result = reload_session(&mut app, &mut config, &mut watcher, root, &tx);

        assert!(result.is_err(), "reload should fail on malformed TOML");
        assert_eq!(
            config.documents.types.len(),
            1,
            "config A must remain active after a failed reload"
        );
        assert_eq!(
            app.doc_types.len(),
            1,
            "App caches must remain config A after a failed reload"
        );
        assert_eq!(
            app.store.list(&Default::default()).len(),
            docs_before,
            "store must be unchanged after a failed reload"
        );
    }

    // AC8: a strict-parse violation (missing `[[relationships]]`) is rejected by
    // `Config::load`, so state stays intact -- the prev Config/Store remain.
    #[test]
    fn reload_session_missing_relationships_leaves_state_intact() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".lazyspec.toml"), valid_config_toml(2)).unwrap();

        let mut config = Config::load(root, &crate::engine::fs::RealFileSystem).unwrap();
        let mut app = make_app(root, &config);
        let (tx, _rx) = crossbeam_channel::unbounded();
        let mut watcher = make_watcher(&tx);

        // Config B: a single type but NO `[[relationships]]` block. Strict parse rejects it.
        let invalid =
            "[[documents.types]]\nname = \"solo\"\nplural = \"solos\"\ndir = \"docs/solo\"\n";
        std::fs::write(root.join(".lazyspec.toml"), invalid).unwrap();

        let result = reload_session(&mut app, &mut config, &mut watcher, root, &tx);

        assert!(
            result.is_err(),
            "reload should fail when [[relationships]] is missing"
        );
        assert_eq!(
            config.documents.types.len(),
            2,
            "config A must remain active after the rejected reload"
        );
        assert_eq!(app.doc_types.len(), 2, "App caches must remain config A");
    }

    // AC6: a FileChange whose paths include `<root>/.lazyspec.toml` requests a
    // session reload. The `run` loop drains the flag and calls `reload_session`
    // (covered by the reload_session unit tests above).
    #[test]
    fn file_change_on_lazyspec_toml_requests_reload() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".lazyspec.toml"), valid_config_toml(1)).unwrap();

        let config = Config::load(root, &crate::engine::fs::RealFileSystem).unwrap();
        let mut app = make_app(root, &config);
        assert!(!app.config_reload_request);

        let event = notify::Event::new(EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(root.join(".lazyspec.toml"));
        handle_app_event(&mut app, AppEvent::FileChange(event), root, &config);

        assert!(
            app.config_reload_request,
            "a .lazyspec.toml FileChange must request a reload"
        );
    }

    // AC6 negative: an md-only FileChange must NOT request a reload, otherwise
    // every doc edit would re-parse the config and rebuild the store.
    #[test]
    fn file_change_on_md_only_does_not_request_reload() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".lazyspec.toml"), valid_config_toml(1)).unwrap();

        let config = Config::load(root, &crate::engine::fs::RealFileSystem).unwrap();
        let mut app = make_app(root, &config);

        let event = notify::Event::new(EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(root.join("docs/type0/STORY-001-example.md"));
        handle_app_event(&mut app, AppEvent::FileChange(event), root, &config);

        assert!(
            !app.config_reload_request,
            "an md-only FileChange must not request a reload"
        );
    }
}
