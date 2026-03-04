use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::engine::document::Status;
use crate::tui::app::{App, Panel};

fn status_color(status: &Status) -> Color {
    match status {
        Status::Draft => Color::Yellow,
        Status::Review => Color::Blue,
        Status::Accepted => Color::Green,
        Status::Rejected => Color::Red,
        Status::Superseded => Color::DarkGray,
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(f.area());

    let title = Line::from(vec![Span::styled(
        "  lazyspec",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]);
    f.render_widget(Paragraph::new(title), outer[0]);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(outer[1]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main[1]);

    draw_type_panel(f, app, main[0]);
    draw_doc_list(f, app, right[0]);
    draw_preview(f, app, right[1]);
}

fn draw_type_panel(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .doc_types
        .iter()
        .enumerate()
        .map(|(i, dt)| {
            let count = app.doc_count(dt);
            let content = format!("  {}s  ({})", dt, count);
            let style = if i == app.selected_type {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(content).style(style)
        })
        .collect();

    let border_style = if app.active_panel == Panel::Types {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" Types "),
    );
    f.render_widget(list, area);
}

fn draw_doc_list(f: &mut Frame, app: &App, area: Rect) {
    let docs = app.docs_for_current_type();
    let items: Vec<ListItem> = docs
        .iter()
        .enumerate()
        .map(|(i, doc)| {
            let filename = doc
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?");
            let status_style = Style::default().fg(status_color(&doc.status));
            let line = Line::from(vec![
                Span::raw(format!("  {:<30} ", filename)),
                Span::styled(format!("{}", doc.status), status_style),
            ]);
            let style = if i == app.selected_doc {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let border_style = if app.active_panel == Panel::DocList {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" Documents "),
    );
    f.render_widget(list, area);
}

fn draw_preview(f: &mut Frame, app: &App, area: Rect) {
    let content = if let Some(doc) = app.selected_doc_meta() {
        match app.store.get_body(&doc.path) {
            Ok(body) => {
                let header = format!(
                    "# {}\nType: {} | Status: {} | Author: {}\nDate: {} | Tags: {}\n\n",
                    doc.title,
                    doc.doc_type,
                    doc.status,
                    doc.author,
                    doc.date,
                    doc.tags.join(", ")
                );
                format!("{}{}", header, body)
            }
            Err(_) => "Error loading document body.".to_string(),
        }
    } else {
        "No document selected.".to_string()
    };

    let paragraph = Paragraph::new(Text::raw(content))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Preview "),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}
