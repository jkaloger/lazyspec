#[cfg(feature = "agent")]
pub mod agent;
pub mod app;
pub mod ui;

use app::App;
use crate::engine::config::Config;
use crate::engine::store::Store;
use app::AppEvent;
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
use std::sync::Arc;
use std::time::Duration;

fn run_editor(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    path: &Path,
) -> Result<()> {
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    disable_raw_mode()?;

    let editor = app::resolve_editor();
    let status = Command::new(&editor).arg(path).status();

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;

    if let Err(e) = status {
        eprintln!("Failed to launch editor '{}': {}", editor, e);
    }

    Ok(())
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
                                let _ = app.store.reload_file(root, relative);
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
        #[cfg(feature = "agent")]
        AppEvent::AgentFinished => {}
    }
}

pub fn run(store: Store, config: &Config) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(store, config);
    app.git_branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    app.refresh_validation(config);

    let (tx, rx) = crossbeam_channel::unbounded();
    app.event_tx = tx.clone();
    let root = app.store.root().to_path_buf();
    let fs_tx = tx.clone();
    let mut _watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            let _ = fs_tx.send(AppEvent::FileChange(event));
        }
    })?;

    let dirs: Vec<&str> = config.types.iter().map(|t| t.dir.as_str()).collect();
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
            if crossterm::event::poll(Duration::from_millis(50)).unwrap_or(false) {
                if let Ok(Event::Key(key)) = crossterm::event::read() {
                    let _ = term_tx.send(AppEvent::Terminal(key));
                }
            }
        }
    });

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;
        app.request_expansion(&tx);

        #[cfg(feature = "agent")]
        app.agent_spawner.poll_finished();

        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                handle_app_event(&mut app, event, &root, config);
                while let Ok(event) = rx.try_recv() {
                    handle_app_event(&mut app, event, &root, config);
                }
            }
            Err(_) => {}
        }

        if let Some(path) = app.editor_request.take() {
            input_paused.store(true, Ordering::Relaxed);
            while rx.try_recv().is_ok() {}
            run_editor(&mut terminal, &path)?;
            while rx.try_recv().is_ok() {}
            input_paused.store(false, Ordering::Relaxed);
            let root = app.store.root().to_path_buf();
            if let Ok(relative) = path.strip_prefix(&root) {
                let _ = app.store.reload_file(&root, relative);
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
            let output = crate::cli::fix::run_human(&root, &app.store, config, &paths, false);
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
