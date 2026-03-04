pub mod app;
pub mod ui;

use app::App;
use crate::engine::store::Store;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

pub fn run(store: Store) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(store);

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        if let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event::read()?
        {
            if app.search_mode {
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
                    (KeyCode::Char('/'), _) => app.enter_search(),
                    (KeyCode::Enter, _) => app.enter_fullscreen(),
                    (KeyCode::Char('j') | KeyCode::Down, _) => app.move_down(),
                    (KeyCode::Char('k') | KeyCode::Up, _) => app.move_up(),
                    (KeyCode::Char('h') | KeyCode::Left, _) => {
                        app.active_panel = app::Panel::Types;
                    }
                    (KeyCode::Char('l') | KeyCode::Right, _) => {
                        app.active_panel = app::Panel::DocList;
                    }
                    (KeyCode::Char('g'), _) => app.move_to_top(),
                    (KeyCode::Char('G'), _) => app.move_to_bottom(),
                    _ => {}
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
