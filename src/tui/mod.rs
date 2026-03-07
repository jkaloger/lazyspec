pub mod app;
pub mod ui;

use app::App;
use crate::engine::config::Config;
use crate::engine::store::Store;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use notify::{RecursiveMode, Watcher, EventKind};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
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

pub fn run(store: Store, config: &Config) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(store, config);

    let (tx, rx) = mpsc::channel();
    let root = app.store.root().to_path_buf();
    let mut _watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    })?;

    let dirs: Vec<&str> = config.types.iter().map(|t| t.dir.as_str()).collect();
    for dir in &dirs {
        let full = root.join(dir);
        if full.exists() {
            _watcher.watch(&full, RecursiveMode::NonRecursive)?;
        }
    }

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        while let Ok(event) = rx.try_recv() {
            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                    for path in &event.paths {
                        if path.extension().and_then(|e| e.to_str()) == Some("md") {
                            if let Ok(relative) = path.strip_prefix(&root) {
                                let _ = app.store.reload_file(&root, relative);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {
                let root = app.store.root().to_path_buf();
                app.handle_key(code, modifiers, &root, config);
            }
        }

        if let Some(path) = app.editor_request.take() {
            run_editor(&mut terminal, &path)?;
            let root = app.store.root().to_path_buf();
            if let Ok(relative) = path.strip_prefix(&root) {
                let _ = app.store.reload_file(&root, relative);
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
