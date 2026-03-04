pub mod app;
pub mod ui;

use app::App;
use crate::engine::config::Config;
use crate::engine::store::Store;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use notify::{RecursiveMode, Watcher, EventKind};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::sync::mpsc;
use std::time::Duration;

pub fn run(store: Store, config: &Config) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(store);

    let (tx, rx) = mpsc::channel();
    let root = app.store.root().to_path_buf();
    let mut _watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    })?;

    let dirs = [
        &config.directories.rfcs,
        &config.directories.adrs,
        &config.directories.stories,
        &config.directories.iterations,
    ];
    for dir in dirs {
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
            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = event::read()?
            {
                if app.show_help {
                    app.show_help = false;
                } else if app.create_form.active {
                    match code {
                        KeyCode::Esc => app.close_create_form(),
                        KeyCode::Enter => {
                            let root = app.store.root().to_path_buf();
                            let _ = app.submit_create_form(&root, config);
                        }
                        KeyCode::Tab => app.form_next_field(),
                        KeyCode::BackTab => app.form_prev_field(),
                        KeyCode::Backspace => app.form_backspace(),
                        KeyCode::Char(c) => app.form_type_char(c),
                        _ => {}
                    }
                } else if app.delete_confirm.active {
                    match code {
                        KeyCode::Enter => { let _ = app.confirm_delete(&root); }
                        KeyCode::Esc => app.close_delete_confirm(),
                        _ => {}
                    }
                } else if app.search_mode {
                    match code {
                        KeyCode::Esc => app.exit_search(),
                        KeyCode::Enter => app.select_search_result(),
                        KeyCode::Backspace => {
                            app.search_query.pop();
                            app.update_search();
                        }
                        KeyCode::Up => {
                            if app.search_selected > 0 {
                                app.search_selected -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if !app.search_results.is_empty()
                                && app.search_selected < app.search_results.len() - 1
                            {
                                app.search_selected += 1;
                            }
                        }
                        KeyCode::Char(c) => {
                            if modifiers.contains(KeyModifiers::CONTROL) && c == 'k' {
                                if app.search_selected > 0 {
                                    app.search_selected -= 1;
                                }
                            } else if modifiers.contains(KeyModifiers::CONTROL) && c == 'j' {
                                if !app.search_results.is_empty()
                                    && app.search_selected < app.search_results.len() - 1
                                {
                                    app.search_selected += 1;
                                }
                            } else {
                                app.search_query.push(c);
                                app.update_search();
                            }
                        }
                        _ => {}
                    }
                } else if app.fullscreen_doc {
                    match code {
                        KeyCode::Esc | KeyCode::Char('q') => app.exit_fullscreen(),
                        KeyCode::Char('j') | KeyCode::Down => app.scroll_down(),
                        KeyCode::Char('k') | KeyCode::Up => app.scroll_up(),
                        KeyCode::Char('g') => app.scroll_offset = 0,
                        KeyCode::Char('G') => app.scroll_offset = u16::MAX / 2,
                        _ => {}
                    }
                } else {
                    match (code, modifiers) {
                        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                            app.should_quit = true;
                        }
                        (KeyCode::Char('?'), _) => {
                            app.show_help = true;
                        }
                        (KeyCode::Char('/'), _) => app.enter_search(),
                        (KeyCode::Char('n'), _) => app.open_create_form(),
                        (KeyCode::Char('d'), _) if app.selected_doc_meta().is_some() => {
                            app.open_delete_confirm();
                        }
                        (KeyCode::Enter, _) => {
                            if app.preview_tab == app::PreviewTab::Relations {
                                app.navigate_to_relation();
                            } else {
                                app.enter_fullscreen();
                            }
                        }
                        (KeyCode::Char('j') | KeyCode::Down, _) => {
                            if app.preview_tab == app::PreviewTab::Relations {
                                app.move_relation_down();
                            } else {
                                app.move_down();
                            }
                        }
                        (KeyCode::Char('k') | KeyCode::Up, _) => {
                            if app.preview_tab == app::PreviewTab::Relations {
                                app.move_relation_up();
                            } else {
                                app.move_up();
                            }
                        }
                        (KeyCode::Char('h') | KeyCode::Left, _) => app.move_type_prev(),
                        (KeyCode::Char('l') | KeyCode::Right, _) => app.move_type_next(),
                        (KeyCode::Tab, _) => app.toggle_preview_tab(),
                        (KeyCode::Char('g'), _) => app.move_to_top(),
                        (KeyCode::Char('G'), _) => app.move_to_bottom(),
                        _ => {}
                    }
                }
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
