mod colors;
mod layout;
mod overlays;
mod panels;

pub use colors::{status_color, tag_color};
pub use layout::{calculate_image_height, wrapped_line_count, wrapped_lines_total};

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::{App, ViewMode};

use overlays::{
    draw_create_form, draw_help_overlay, draw_delete_confirm, draw_link_editor,
    draw_search_overlay, draw_status_picker, draw_warnings_panel,
};
#[cfg(feature = "agent")]
use overlays::draw_agent_dialog;
use panels::{
    draw_doc_list, draw_graph, draw_metrics_skeleton, draw_preview, draw_type_panel,
    render_filter_panel, render_fullscreen_document,
};
#[cfg(feature = "agent")]
use panels::draw_agents_screen;

pub fn draw(f: &mut Frame, app: &mut App) {
    app.git_status_cache.refresh();
    if app.fullscreen_doc {
        render_fullscreen_document(f, app);
        if app.show_warnings {
            draw_warnings_panel(f, app);
        }
        if app.show_help {
            draw_help_overlay(f);
        }
        return;
    }
    if app.create_form.active {
        draw_create_form(f, app);
        if app.show_warnings {
            draw_warnings_panel(f, app);
        }
        if app.show_help {
            draw_help_overlay(f);
        }
        return;
    }
    if app.search_mode {
        draw_search_overlay(f, app);
        if app.show_warnings {
            draw_warnings_panel(f, app);
        }
        if app.show_help {
            draw_help_overlay(f);
        }
        return;
    }

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

    let mode_indicator = Line::from(vec![Span::styled(
        format!("[{}] ` to cycle ", app.view_mode.name()),
        Style::default().fg(Color::DarkGray),
    )]);
    f.render_widget(
        Paragraph::new(mode_indicator).alignment(Alignment::Right),
        outer[0],
    );

    match app.view_mode {
        ViewMode::Types => {
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
        ViewMode::Filters => render_filter_panel(f, app, outer[1]),
        ViewMode::Metrics => draw_metrics_skeleton(f, outer[1]),
        ViewMode::Graph => draw_graph(f, app, outer[1]),
        #[cfg(feature = "agent")]
        ViewMode::Agents => draw_agents_screen(f, app, outer[1]),
    }

    if app.delete_confirm.active {
        draw_delete_confirm(f, app);
    }

    if app.status_picker.active {
        draw_status_picker(f, app);
    }

    if app.link_editor.active {
        draw_link_editor(f, app);
    }

    #[cfg(feature = "agent")]
    if app.agent_dialog.active {
        draw_agent_dialog(f, app);
    }

    if app.show_warnings {
        draw_warnings_panel(f, app);
    }

    if app.show_help {
        draw_help_overlay(f);
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::document::Status;
    use std::path::Path;

    use super::panels;

    fn display_name(path: &Path) -> &str {
        let stem = path.file_stem().and_then(|s| s.to_str());
        match stem {
            Some("index") => path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("?"),
            Some(name) => name,
            None => "?",
        }
    }

    #[test]
    fn display_name_flat_file() {
        assert_eq!(display_name(Path::new("docs/rfcs/RFC-001-foo.md")), "RFC-001-foo");
    }

    #[test]
    fn display_name_subfolder_index() {
        assert_eq!(display_name(Path::new("docs/rfcs/RFC-002-bar/index.md")), "RFC-002-bar");
    }

    fn cell_debug(cell: &ratatui::widgets::Cell) -> String {
        format!("{:?}", cell)
    }

    #[test]
    fn doc_row_cells_standard_document() {
        let tags = vec!["cli".to_string(), "tui".to_string()];
        let cells = panels::doc_row_cells_for_test("RFC-001", "Test Title", &Status::Draft, &tags, false, false);

        assert_eq!(cells.len(), 4);

        let id_dbg = cell_debug(&cells[0]);
        assert!(id_dbg.contains("RFC-001"), "ID cell should contain RFC-001, got: {}", id_dbg);
        assert!(!id_dbg.contains("(virtual)"), "Non-virtual doc should not contain (virtual)");

        let title_dbg = cell_debug(&cells[1]);
        assert!(title_dbg.contains("Test Title"), "Title cell should contain Test Title, got: {}", title_dbg);
        assert!(!title_dbg.contains("(virtual)"), "Non-virtual doc should not contain (virtual)");

        let status_dbg = cell_debug(&cells[2]);
        assert!(status_dbg.contains("draft"), "Status cell should contain draft, got: {}", status_dbg);

        let tags_dbg = cell_debug(&cells[3]);
        assert!(tags_dbg.contains("[cli]"), "Tags cell should contain [cli], got: {}", tags_dbg);
        assert!(tags_dbg.contains("[tui]"), "Tags cell should contain [tui], got: {}", tags_dbg);
    }

    #[test]
    fn doc_row_cells_virtual_document() {
        let cells = panels::doc_row_cells_for_test("RFC-002", "Virtual Doc", &Status::Draft, &[], true, false);

        assert_eq!(cells.len(), 4);

        let title_dbg = cell_debug(&cells[1]);
        assert!(title_dbg.contains("(virtual)"), "Virtual doc title should contain (virtual), got: {}", title_dbg);
    }

    #[test]
    fn doc_row_cells_tag_overflow() {
        let tags = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string(),
        ];
        let cells = panels::doc_row_cells_for_test("RFC-003", "Tags", &Status::Draft, &tags, false, false);

        let tags_dbg = cell_debug(&cells[3]);
        assert!(tags_dbg.contains("[a]"), "Tags cell should contain [a], got: {}", tags_dbg);
        assert!(tags_dbg.contains("[b]"), "Tags cell should contain [b], got: {}", tags_dbg);
        assert!(tags_dbg.contains("[c]"), "Tags cell should contain [c], got: {}", tags_dbg);
        assert!(tags_dbg.contains("+2"), "Tags cell should contain +2 overflow, got: {}", tags_dbg);
        assert!(!tags_dbg.contains("[d]"), "Tags cell should not contain [d], got: {}", tags_dbg);
        assert!(!tags_dbg.contains("[e]"), "Tags cell should not contain [e], got: {}", tags_dbg);
    }

    #[test]
    fn doc_row_cells_dim_when_relations_focused() {
        let tags = vec!["x".to_string()];
        let cells = panels::doc_row_cells_for_test("RFC-004", "Dim", &Status::Accepted, &tags, false, true);

        for (i, cell) in cells.iter().enumerate() {
            let dbg = cell_debug(cell);
            let has_dark_gray = dbg.contains("DarkGray") || dbg.contains("dark_gray");
            assert!(
                has_dark_gray,
                "Cell {} should have DarkGray style when dim=true, got: {}",
                i,
                dbg,
            );
        }
    }
}
