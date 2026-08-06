use crate::engine::clickup::ClickupClient;
use crate::engine::config::{Config, StoreBackend};
use crate::engine::credentials::{CredentialStore, LayeredCredentialStore};
use crate::engine::document::split_frontmatter;
use crate::engine::gh::{GhCli, GhGraphql, GhIssueDependencyApi, GhIssueReader};
use crate::engine::git_ref::{GitCli, GitRefOps};
use crate::engine::git_ref_store::GitRefStore;
use crate::engine::issue_body::TypeMatchRule;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::status_colors::StatusColors;
use crate::engine::store::Store;
use crate::engine::store_dispatch::{DocumentStore, GithubIssuesStore};
use crate::engine::sync::{
    sync_all, ClickupMaps, ClickupSync, GhIssueSync, GhMaps, GhMilestoneSync, GhRound, GitRefSync,
    SyncContext, Syncers,
};
use crate::engine::task_map::TaskMap;
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
use std::sync::{Arc, Mutex, PoisonError, TryLockError};
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

    let command = crate::tui::state::resolve_editor_command();
    let (program, args) = command.split_first().expect("editor command is non-empty");
    let status = Command::new(program).args(args).arg(path).status();

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;

    if let Err(e) = status {
        eprintln!("Failed to launch editor '{}': {}", program, e);
    }

    Ok(())
}

fn run_viewer(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    command: &[String],
    path: &Path,
) -> Result<()> {
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    disable_raw_mode()?;

    let (program, args) = command.split_first().expect("viewer command is non-empty");
    let status = Command::new(program).args(args).arg(path).status();

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;

    if let Err(e) = status {
        eprintln!("Failed to launch viewer '{}': {}", program, e);
    }
    Ok(())
}

// Reload the shared store's issue map from disk without ever waiting on the
// lock. The background poll holds this same mutex across the whole network sync
// (`poll_sync`), so a blocking `lock()` on the UI thread would stall the loop
// until the sync finished -- the root of BUG-001. `try_lock` returns at once; on
// contention we return `false` and the caller keeps `gh_issue_map_stale` set to
// retry next tick. A poisoned lock is recovered: `issue_map` is plain data with
// no invariant to protect.
fn try_refresh_issue_map(shared_store: &Arc<Mutex<GithubIssuesStore>>, root: &Path) -> bool {
    let mut guard = match shared_store.try_lock() {
        Ok(g) => g,
        Err(TryLockError::WouldBlock) => return false,
        Err(TryLockError::Poisoned(e)) => e.into_inner(),
    };
    if let Ok(map) = IssueMap::load(root) {
        guard.issue_map = map;
    }
    true
}

fn try_push_gh_edit(
    root: &Path,
    relative: &Path,
    config: &Config,
    shared_store: &Arc<Mutex<GithubIssuesStore>>,
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
        .map(|_| ())
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
        git: Box::new(GitCli),
        root: root.to_path_buf(),
        config: config.clone(),
        remote: config.git_ref.remote.clone(),
        reserved_number: None,
    };
    git_store
        .recommit_cache(type_def, &doc_id)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// Push a clickup-tasks doc's edited body back to ClickUp after an external-editor
// save -- the third backend arm alongside `try_push_gh_edit`/`try_push_git_ref_edit`
// (RFC-056 write-through). Early-returns `Ok(())` for any non-clickup type, so the
// caller spawns it unconditionally and the gating stays internal.
//
// Production wiring; delegates to `try_push_clickup_edit_with` with the real HTTP
// client and the global credential store. Token/network I/O lives here in the TUI
// layer, never in engine `Store::load` (DICTUM-003).
fn try_push_clickup_edit(root: &Path, relative: &Path, config: &Config) -> Result<(), String> {
    try_push_clickup_edit_with(
        root,
        relative,
        config,
        crate::engine::clickup::ClickupHttpClient::new,
        || LayeredCredentialStore::global().load_clickup_token(),
    )
}

// The `try_push_clickup_edit` body with the client factory and token loader
// injected, so a test drives the `ClickupClient` seam with a `FakeClickupClient`
// and a scripted token (DICTUM-002) without a keychain or the network.
fn try_push_clickup_edit_with<C: ClickupClient + 'static>(
    root: &Path,
    relative: &Path,
    config: &Config,
    client_factory: impl FnOnce() -> C,
    token_loader: impl FnOnce() -> anyhow::Result<Option<crate::engine::credentials::Token>>,
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

    if type_def.store != StoreBackend::ClickupTasks {
        return Ok(());
    }

    let body_trimmed = body.trim();
    let mut clickup_store = crate::engine::store_dispatch::clickup_write_store(
        root,
        config,
        "editing",
        client_factory,
        token_loader,
    )
    .map_err(|e| e.to_string())?;
    clickup_store
        .update(type_def, &doc_id, &[("body", body_trimmed)])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// Human-readable summary of a fix run for the warnings panel. The engine op core
// returns the structured `FixOutput`; each frontend renders its own
// presentation, so this mirrors the CLI's `fix::output::format_human`
// (non-dry-run) output without the TUI reaching into the cli module (STORY-212 AC1).
fn format_fix_output(output: &crate::engine::ops::fix::FixOutput) -> String {
    let mut result = String::new();

    for r in &output.field_fixes {
        if r.fields_added.is_empty() {
            continue;
        }
        result.push_str(&format!(
            "Fixed {} (added: {})\n",
            r.path,
            r.fields_added.join(", ")
        ));
    }

    for c in &output.conflict_fixes {
        result.push_str(&format!("Renamed {} -> {}\n", c.old_path, c.new_path));
    }

    for r in &output.status_fixes {
        result.push_str(&format!(
            "Fixed status in {}: {} -> {}\n",
            r.path, r.old_status, r.new_status
        ));
    }

    for r in &output.relation_fixes {
        for (old_target, new_target) in &r.replacements {
            result.push_str(&format!(
                "Migrated relation in {}: {} -> {}\n",
                r.path, old_target, new_target
            ));
        }
        for (rel_type, target) in &r.deduped {
            result.push_str(&format!(
                "Dropped duplicate relation in {}: {} {}\n",
                r.path, rel_type, target
            ));
        }
    }

    result
}

// One background poll: refresh every configured type through the engine's
// `sync_all`, exactly as `lazyspec fetch` does, and return the warnings to
// surface on `CacheRefresh` -- each `SyncOutcome`'s `error` and `warnings`,
// folded together. Never aborts: a per-type failure is a warning, not a crash.
//
// The GitHub `issue_map` is borrowed `&mut` straight out of the shared
// `GithubIssuesStore` (locked for the whole sync), so the poll, `try_push_gh_edit`,
// and the `gh_issue_map_stale` reload all read one authoritative map -- no
// drifting duplicate. ClickUp's `task_map`/`status_colors` are per-poll, loaded
// and saved here. Derived lifecycles are deliberately NOT persisted: a
// background poll must never rewrite `.lazyspec.toml`.
//
// Clients/tokens are injected (DICTUM-003): production passes the real `GhCli` /
// `GitCli` / `ClickupHttpClient`; tests drive the same seams with fakes.
#[allow(clippy::too_many_arguments)]
fn poll_sync(
    root: &Path,
    config: &Config,
    gh_store: Option<&Arc<Mutex<GithubIssuesStore>>>,
    gh_reader: &dyn GhIssueReader,
    gh_graphql: &dyn GhGraphql,
    gh_dependency: &dyn GhIssueDependencyApi,
    git_ops: &dyn GitRefOps,
    clickup: &dyn ClickupClient,
    clickup_token: Option<&str>,
) -> Vec<String> {
    let types = &config.documents.types;
    let has_milestones = types
        .iter()
        .any(|t| t.store == StoreBackend::GithubMilestones);
    let has_gh_issues = types.iter().any(|t| t.store == StoreBackend::GithubIssues);
    let has_git_ref = types.iter().any(|t| t.store == StoreBackend::GitRef);
    let has_clickup = types.iter().any(|t| t.store == StoreBackend::ClickupTasks);

    let mut warnings: Vec<String> = Vec::new();
    let type_rules: Vec<TypeMatchRule> = types.iter().map(TypeMatchRule::from).collect();

    // Per-poll ClickUp sidecar maps, loaded only when a clickup-tasks type is
    // configured and a token is present; saved after `sync_all`.
    let mut task_map = None;
    let mut status_colors = None;
    if has_clickup && clickup_token.is_some() {
        match TaskMap::load(root) {
            Ok(m) => task_map = Some(m),
            Err(e) => warnings.push(format!(
                "clickup poll skipped: failed to load task map: {e}"
            )),
        }
        match StatusColors::load(root) {
            Ok(c) => status_colors = Some(c),
            Err(e) => warnings.push(format!(
                "clickup poll skipped: failed to load status colours: {e}"
            )),
        }
    }

    // Lock the shared store for the whole sync so the borrowed `issue_map` is the
    // store's own field, not a copy. `repo` is read out first so the syncers can
    // own it without contending with the `&mut` borrow below.
    let mut guard = gh_store.map(|s| s.lock().unwrap_or_else(PoisonError::into_inner));
    let repo = guard.as_ref().map(|g| g.repo.clone());

    let outcomes = {
        let mut ctx = SyncContext {
            gh: guard.as_mut().map(|g| GhMaps {
                issue_map: &mut g.issue_map,
            }),
            clickup: match (task_map.as_mut(), status_colors.as_mut()) {
                (Some(t), Some(s)) => Some(ClickupMaps {
                    task_map: t,
                    status_colors: s,
                }),
                _ => None,
            },
            fetch: None,
        };

        let mut syncers = Syncers::default();
        if let Some(repo) = repo.clone() {
            syncers.round = Some(GhRound {
                gh: gh_graphql,
                repo,
            });
        }
        if has_milestones {
            syncers.milestone = Some(GhMilestoneSync);
        }
        if has_gh_issues {
            if let Some(repo) = repo.clone() {
                syncers.issue = Some(GhIssueSync {
                    reader: gh_reader,
                    graphql: gh_graphql,
                    dependency: gh_dependency,
                    repo,
                    type_rules,
                });
            }
        }
        if has_git_ref {
            syncers.git_ref = Some(GitRefSync {
                ops: git_ops,
                remote: config.git_ref.remote.clone(),
            });
        }
        if has_clickup {
            if let Some(token) = clickup_token {
                syncers.clickup = Some(ClickupSync {
                    client: clickup,
                    token: token.to_string(),
                });
            }
        }

        sync_all(root, config, &mut ctx, &mut syncers, None)
    };

    for o in &outcomes {
        if let Some(e) = &o.error {
            warnings.push(format!("{}: {}", o.type_name, e));
        }
        warnings.extend(o.warnings.iter().cloned());
    }

    // Save through the borrow: `issue_map` is the store's own field, mutated in
    // place; the ClickUp maps are per-poll. No lifecycle persist.
    if let Some(g) = guard.as_ref() {
        if let Err(e) = g.issue_map.save(root) {
            warnings.push(format!("issue map save failed: {e}"));
        }
    }
    drop(guard);
    if let Some(m) = &task_map {
        if let Err(e) = m.save(root) {
            warnings.push(format!("clickup task map save failed: {e}"));
        }
    }
    if let Some(c) = &status_colors {
        if let Err(e) = c.save(root) {
            warnings.push(format!("status colours save failed: {e}"));
        }
    }

    warnings
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
                    app.refresh_validation(config);
                    app.expanded_body_cache.clear();
                    app.disk_cache.clear();
                }
                Err(msg) => {
                    app.gh_conflict_message = Some(msg);
                }
            }
        }
        AppEvent::SearchResults {
            generation,
            results,
        } => {
            app.apply_search_results(generation, results);
        }
        AppEvent::CreateStarted => {}
        AppEvent::CreateProgress { message, state } => {
            if app.create_form.active && app.create_form.loading {
                app.create_form.status_message = Some(message);
                app.create_form.state = state;
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
                    app.refresh_validation(config);
                    app.git_status_cache.invalidate();
                    app.gh_issue_map_stale = true;
                    // Hold the success face over the updated list for a beat so a
                    // create that finishes instantly still renders it before the
                    // overlay is torn down; the run loop dismisses on this deadline.
                    app.create_form.loading = false;
                    app.create_form.state = crate::spinners::SpinnerState::Success;
                    app.create_form.dismiss_at =
                        Some(std::time::Instant::now() + std::time::Duration::from_millis(600));
                }
                Err(msg) => {
                    app.create_form.loading = false;
                    app.create_form.state = crate::spinners::SpinnerState::Error;
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

    // Background search worker (BUG-011): fuzzy-scoring every doc body takes
    // tens of milliseconds, which blocked the event loop when run per keystroke.
    // The worker owns each request's corpus snapshot (DICTUM-007: threads never
    // touch App state; messages only) and drains the channel to the newest
    // request before searching, so a burst of keystrokes costs one search.
    // Results carry their generation; the handler drops stale ones.
    let (search_tx, search_rx) = crossbeam_channel::unbounded::<crate::tui::state::SearchRequest>();
    app.search_tx = search_tx;
    let search_result_tx = tx.clone();
    std::thread::spawn(move || {
        while let Ok(mut req) = search_rx.recv() {
            while let Ok(newer) = search_rx.try_recv() {
                req = newer;
            }
            let results: Vec<std::path::PathBuf> = req
                .corpus
                .search(&req.query)
                .into_iter()
                .map(|r| r.path)
                .collect();
            if search_result_tx
                .send(AppEvent::SearchResults {
                    generation: req.generation,
                    results,
                })
                .is_err()
            {
                break;
            }
        }
    });

    let shared_gh_store: Option<Arc<Mutex<GithubIssuesStore>>> =
        if crate::tui::has_pollable_types(&config) {
            let gh_config = config.documents.github.as_ref();
            let repo = gh_config.and_then(|g| g.repo.clone());
            repo.map(|repo| {
                let root = app.store.root();
                Arc::new(Mutex::new(GithubIssuesStore {
                    client: Box::new(GhCli::new()),
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
    // Schedule polling whenever the project has any pollable type, independent of
    // whether a github store was built: a clickup-only project has no github repo
    // (shared_gh_store == None) but must still poll to refresh its task cache.
    let mut next_poll = if crate::tui::has_pollable_types(&config) {
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
    let anim_start = Instant::now();
    loop {
        let loop_start = Instant::now();

        let t = Instant::now();
        // Spinner cadence: wall-clock seconds, not loop count. The render loop
        // wakes tens of times/sec for input; binding frame_idx to that spun the
        // spinner far too fast. One frame per second.
        app.frame_idx = anim_start.elapsed().as_secs();
        app.refresh_in_flight = refresh_in_flight.load(Ordering::Relaxed);
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

        if let Some(deadline) = app.create_form.dismiss_at {
            if Instant::now() >= deadline {
                app.close_create_form();
            }
        }

        if app.gh_issue_map_stale {
            match shared_gh_store {
                Some(ref shared_store) => {
                    if try_refresh_issue_map(shared_store, &root) {
                        app.gh_issue_map_stale = false;
                    }
                }
                None => app.gh_issue_map_stale = false,
            }
        }

        if let Some(deadline) = next_poll {
            if Instant::now() >= deadline && !refresh_in_flight.load(Ordering::Relaxed) {
                // Always advance the deadline, even when there is no work this
                // poll, so the trigger keeps firing for later refreshes.
                next_poll = Some(Instant::now() + Duration::from_secs(cache_ttl));
                // Spawn ONE poll thread whenever there is ANY work: a github store
                // to refresh, or at least one clickup-tasks type. A project with
                // neither (no gh store AND no clickup types) skips the spawn and
                // just rides the advanced deadline above.
                let has_clickup_types = config
                    .documents
                    .types
                    .iter()
                    .any(|t| t.store == StoreBackend::ClickupTasks);
                if shared_gh_store.is_some() || has_clickup_types {
                    refresh_in_flight.store(true, Ordering::Relaxed);
                    let poll_tx = tx.clone();
                    let poll_root = root.clone();
                    let poll_config = config.clone();
                    let poll_flag = refresh_in_flight.clone();
                    let poll_store = shared_gh_store.clone();
                    std::thread::spawn(move || {
                        // Real clients/tokens, injected into the engine seam
                        // (DICTUM-003) and reused across every type this poll
                        // refreshes. The ClickUp token is loaded only when a
                        // clickup-tasks type exists, so a github-only project
                        // never touches the credential store.
                        let gh = GhCli::new();
                        let git_ops = GitCli;
                        let clickup = crate::engine::clickup::ClickupHttpClient::new();
                        let clickup_token = if has_clickup_types {
                            LayeredCredentialStore::global()
                                .load_clickup_token()
                                .ok()
                                .flatten()
                        } else {
                            None
                        };

                        let warnings = poll_sync(
                            &poll_root,
                            &poll_config,
                            poll_store.as_ref(),
                            &gh,
                            &gh,
                            &gh,
                            &git_ops,
                            &clickup,
                            clickup_token.as_ref().map(|t| t.expose()),
                        );

                        poll_flag.store(false, Ordering::Relaxed);
                        let _ = poll_tx.send(AppEvent::CacheRefresh { warnings });
                    });
                }
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
                {
                    let push_root = root.clone();
                    let push_relative = relative.to_path_buf();
                    let push_config = config.clone();
                    let push_tx = tx.clone();
                    std::thread::spawn(move || {
                        let result =
                            try_push_clickup_edit(&push_root, &push_relative, &push_config);
                        if let Err(msg) = result {
                            let _ = push_tx.send(AppEvent::GhPushResult(Err(msg)));
                        }
                    });
                }
            }
            app.refresh_validation(&config);
        }

        if let Some(req) = app.open_request.take() {
            match req {
                crate::tui::state::OpenRequest::Browser(url) => {
                    // Detached: no terminal suspend/resume for a browser hand-off.
                    let opener = if cfg!(target_os = "macos") {
                        "open"
                    } else {
                        "xdg-open"
                    };
                    if let Err(e) = Command::new(opener).arg(&url).spawn() {
                        app.open_message =
                            Some(format!("failed to launch browser via '{opener}': {e}"));
                    }
                }
                crate::tui::state::OpenRequest::Viewer { command, path } => {
                    let _stdin_guard = stdin_lock.lock().unwrap_or_else(PoisonError::into_inner);
                    while rx.try_recv().is_ok() {}
                    run_viewer(&mut terminal, &command, &path)?;
                    drain_stdin();
                    while rx.try_recv().is_ok() {}
                    drop(_stdin_guard);
                }
            }
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
            let fixes = crate::engine::ops::fix::plan_field_and_conflict_fixes(
                &root, &app.store, &config, &paths, false, &fs,
            );
            let output = format_fix_output(&fixes);
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

    // --- ITERATION-287: background poll wired onto engine::sync::sync_all ---

    use crate::engine::clickup::{ClickupError, ClickupStatus, ClickupTask};
    use crate::engine::clickup_cache;
    use crate::engine::gh::test_support::MockGhClient;
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use std::cell::Cell;

    fn clickup_config() -> Config {
        let mut td = TypeDef::test_fixture("task", StoreBackend::ClickupTasks);
        td.prefix = "TASK".to_string();
        td.clickup_list_id = Some("list123".to_string());
        let mut config = Config::default();
        config.documents.types = vec![td];
        config
    }

    // An inert GitHub client for clickup-only poll tests: no github types are
    // configured and no github store is passed, so no method is ever reached.
    fn inert_gh() -> MockGhClient {
        MockGhClient::new()
    }

    // AC (STORY-203): a poll over a clickup-tasks type bound to a List with
    // per-status colours writes the derived colours to `status-colors.json` --
    // the deliverable this slice fixes (the poll previously never wrote it).
    #[test]
    fn poll_writes_status_colors_for_clickup_type() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let config = clickup_config();

        let task: ClickupTask = serde_json::from_str(
            r#"{"id":"86abc","name":"Live task","status":{"status":"in progress"}}"#,
        )
        .unwrap();
        let statuses = vec![ClickupStatus {
            status: "in progress".to_string(),
            orderindex: 0,
            status_type: "custom".to_string(),
            color: "#4194f6".to_string(),
        }];
        let clickup = FakeClickupClient::with_tasks(vec![task]).with_statuses(statuses);
        let git = MockGitRefClient::new();
        let reader = inert_gh();

        let warnings = poll_sync(
            root,
            &config,
            None,
            &reader,
            &reader,
            &reader,
            &git,
            &clickup,
            Some("pk_x"),
        );

        assert!(warnings.is_empty(), "got: {warnings:?}");
        assert!(
            root.join(".lazyspec/status-colors.json").exists(),
            "the poll must write the derived status colours sidecar"
        );
        let colors = StatusColors::load(root).unwrap();
        assert_eq!(colors.get("task", "in progress"), Some("#4194f6"));
    }

    // AC (STORY-203): a per-type fetch failure surfaces as a warning on the
    // `CacheRefresh { warnings }` channel and never aborts the poll.
    #[test]
    fn poll_per_type_failure_warns_without_aborting() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let config = clickup_config();

        // The clickup client errors on every call, so fetch_tasks fails -> the
        // outcome carries an error, folded into warnings, and poll_sync returns.
        let clickup = FakeClickupClient::failing(ClickupError::Timeout);
        let git = MockGitRefClient::new();
        let reader = inert_gh();

        let warnings = poll_sync(
            root,
            &config,
            None,
            &reader,
            &reader,
            &reader,
            &git,
            &clickup,
            Some("pk_x"),
        );

        assert!(
            warnings.iter().any(|w| w.starts_with("task:")),
            "the failing type must surface as a warning, got: {warnings:?}"
        );
    }

    // AC (STORY-203): the poll borrows `&mut store.issue_map`, so the fetched
    // mapping lands in the store's own field -- the one authoritative map that
    // `try_push_gh_edit` and the `gh_issue_map_stale` reload also read. No
    // duplicated/drifting copy.
    #[test]
    fn poll_mutates_the_shared_store_issue_map_in_place() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut config = Config::default();
        config.documents.types = vec![TypeDef::test_fixture("story", StoreBackend::GithubIssues)];

        let gh_issue = crate::engine::gh::GhIssue {
            number: 42,
            id: "I_node42".to_string(),
            url: String::new(),
            title: "An issue".to_string(),
            body: String::new(),
            labels: vec![crate::engine::gh::GhLabel {
                name: "lazyspec:story".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-07-01T00:00:00Z".to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };
        let gh = MockGhClient::new().with_list_result(vec![gh_issue]);
        let git = MockGitRefClient::new();
        let clickup = FakeClickupClient::with_tasks(vec![]);

        let store = Arc::new(Mutex::new(GithubIssuesStore {
            client: Box::new(GhCli::new()),
            root: root.to_path_buf(),
            repo: "owner/repo".to_string(),
            config: config.clone(),
            issue_map: IssueMap::load(root).unwrap(),
            issue_cache: IssueCache::new(root),
        }));

        let _warnings = poll_sync(
            root,
            &config,
            Some(&store),
            &gh,
            &gh,
            &gh,
            &git,
            &clickup,
            None,
        );

        // The fetched issue is mapped in the SHARED store's own field, proving
        // the poll borrowed &mut store.issue_map rather than a throwaway copy.
        let guard = store.lock().unwrap();
        assert_eq!(
            guard.issue_map.get("STORY-42").map(|e| e.node_id.as_str()),
            Some("I_node42"),
            "poll must write the fetched mapping into the shared store's issue_map"
        );
    }

    // --- ITERATION-311: non-blocking store lock on the UI thread (BUG-001) ---

    // Regression: the UI-thread issue-map refresh must never wait on the store
    // mutex, so pressing `e` (open editor) stays instant even while the poll
    // holds the lock across a slow network sync. Another thread grabs the lock
    // and holds it; the refresh must return `false` at once, then succeed once
    // the lock is free.
    #[test]
    fn issue_map_refresh_is_nonblocking_while_store_locked() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut config = Config::default();
        config.documents.types = vec![TypeDef::test_fixture("story", StoreBackend::GithubIssues)];

        let store = Arc::new(Mutex::new(GithubIssuesStore {
            client: Box::new(GhCli::new()),
            root: root.to_path_buf(),
            repo: "owner/repo".to_string(),
            config,
            issue_map: IssueMap::load(root).unwrap(),
            issue_cache: IssueCache::new(root),
        }));

        // A stand-in for the poll thread: hold the lock until told to release,
        // signalling once it is actually held so the assertion below is not racy.
        let held = Arc::clone(&store);
        let (locked_tx, locked_rx) = crossbeam_channel::unbounded::<()>();
        let (release_tx, release_rx) = crossbeam_channel::unbounded::<()>();
        let handle = std::thread::spawn(move || {
            let _guard = held.lock().unwrap();
            locked_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        locked_rx.recv().unwrap();

        assert!(
            !try_refresh_issue_map(&store, root),
            "refresh must not block while another thread holds the store lock"
        );

        release_tx.send(()).unwrap();
        handle.join().unwrap();

        assert!(
            try_refresh_issue_map(&store, root),
            "refresh must acquire the lock and succeed once it is free"
        );
    }

    // --- ITERATION-282: clickup arm of the editor-save push-back ---

    use crate::engine::clickup::FakeClickupClient;
    use crate::engine::credentials::Token;

    // Materialize a clickup cache doc (+ task-map baseline) as an earlier fetch
    // would, so `Store::load` resolves the doc and the write path has a mapped
    // task to PUT against. Returns the cache doc's root-relative path. The body is
    // seeded as "old body" so a test can rewrite it to simulate an editor save.
    fn materialize_clickup_doc(root: &Path, config: &Config) -> std::path::PathBuf {
        let type_def = &config.documents.types[0];
        let task: ClickupTask = serde_json::from_str(
            r#"{"id":"86abc","name":"Task","status":{"status":"open"},"date_updated":"1700000000000","markdown_description":"old body"}"#,
        )
        .unwrap();
        let mut task_map = TaskMap::load(root).unwrap();
        clickup_cache::fetch_tasks(
            root,
            type_def,
            &FakeClickupClient::with_tasks(vec![task]),
            "pk_x",
            &mut task_map,
        )
        .unwrap();
        task_map.save(root).unwrap();
        std::path::PathBuf::from(".lazyspec/cache/task/TASK-86abc.md")
    }

    // A non-clickup type is a no-op: the helper returns Ok(()) and never builds a
    // client, so an ordinary filesystem-doc edit never touches ClickUp.
    #[test]
    fn clickup_edit_noop_for_non_clickup_type() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let config = Config::default();

        std::fs::create_dir_all(root.join("docs/rfcs")).unwrap();
        let relative = Path::new("docs/rfcs/RFC-001-first.md");
        std::fs::write(
            root.join(relative),
            concat!(
                "---\n",
                "title: \"First RFC\"\n",
                "type: rfc\n",
                "status: draft\n",
                "author: \"test\"\n",
                "date: 2026-01-01\n",
                "tags: []\n",
                "---\n",
                "Body of first RFC.\n",
            ),
        )
        .unwrap();

        let built = Cell::new(false);
        let result = try_push_clickup_edit_with(
            root,
            relative,
            &config,
            || {
                built.set(true);
                FakeClickupClient::with_tasks(vec![])
            },
            || Ok(Some(Token::new("pk_x"))),
        );

        assert!(result.is_ok(), "got: {result:?}");
        assert!(
            !built.get(),
            "a non-clickup type must return before building a client"
        );
    }

    // Happy path: editing a clickup-tasks doc PUTs the edited body to the mapped
    // task via the injected client seam.
    #[test]
    fn clickup_edit_pushes_body_via_client() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let config = clickup_config();

        let relative = materialize_clickup_doc(root, &config);

        // Simulate the external-editor save: change the body on disk.
        let content = std::fs::read_to_string(root.join(&relative)).unwrap();
        std::fs::write(
            root.join(&relative),
            content.replace("old body", "edited from tui"),
        )
        .unwrap();

        // ClickUp echoes the edited task; the pre-write lock fetch matches the
        // recorded baseline, so the write proceeds.
        let echo: ClickupTask = serde_json::from_str(
            r#"{"id":"86abc","name":"Task","status":{"status":"open"},"date_updated":"1774587145901","markdown_description":"edited from tui"}"#,
        )
        .unwrap();
        let remote_unchanged: ClickupTask = serde_json::from_str(
            r#"{"id":"86abc","name":"Task","status":{"status":"open"},"date_updated":"1700000000000"}"#,
        )
        .unwrap();
        let fake = FakeClickupClient::with_tasks(vec![])
            .with_viewed_task(remote_unchanged)
            .with_updated_task(echo);
        let update_calls = fake.update_calls();

        let result = try_push_clickup_edit_with(
            root,
            &relative,
            &config,
            move || fake,
            || Ok(Some(Token::new("pk_x"))),
        );

        assert!(result.is_ok(), "got: {result:?}");
        let recorded = update_calls.borrow();
        assert_eq!(recorded.len(), 1, "exactly one PUT");
        assert_eq!(recorded[0].0, "86abc", "to the mapped task id");
        assert_eq!(
            recorded[0].1.markdown_content,
            Some("edited from tui".to_string()),
            "the edited body is pushed"
        );
    }

    // Token absent: the helper errors with a one-line warning and never builds a
    // client, so a clickup edit with no credential fails loud without a network
    // call.
    #[test]
    fn clickup_edit_token_absent_errs_without_client() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let config = clickup_config();

        let relative = materialize_clickup_doc(root, &config);

        let built = Cell::new(false);
        let result = try_push_clickup_edit_with(
            root,
            &relative,
            &config,
            || {
                built.set(true);
                FakeClickupClient::with_tasks(vec![])
            },
            || Ok(None),
        );

        let err = result.unwrap_err();
        assert!(err.contains("no ClickUp token"), "got: {err}");
        assert!(
            !built.get(),
            "no client must be built when the token is absent"
        );
    }
}
