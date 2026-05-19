use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Table, TableState, Wrap,
    },
    Frame,
};
use unicode_width::UnicodeWidthChar;

use std::path::PathBuf;

#[cfg(feature = "agent")]
use crate::engine::agent_metadata::AgentStatus;
use crate::engine::config::{Config, StoreBackend};
use crate::engine::document::{DocMeta, RelationType, Status};
use crate::engine::git_status::GitFileStatus;
use crate::tui::state::{App, DocListNode, FilterField, PreviewTab};

use super::colors::{status_color, tag_color};
use super::layout::{calculate_image_height, wrapped_line_count, wrapped_lines_total};

fn get_image_dimensions_cached(app: &mut App, path: &std::path::Path) -> Option<(u32, u32)> {
    if let Some(&dims) = app.image_dimensions_cache.get(path) {
        return Some(dims);
    }
    if let Ok(dims) = image::image_dimensions(path) {
        let dims = (dims.0, dims.1);
        app.image_dimensions_cache.insert(path.to_path_buf(), dims);
        return Some(dims);
    }
    None
}

fn render_markdown_to_lines(text: &str, max_width: u16) -> Vec<Line<'static>> {
    let segments = crate::tui::content::gfm::extract_gfm_segments(text);
    crate::tui::content::gfm::render_gfm_segments(&segments, max_width)
}

fn render_scrollbar(f: &mut Frame, area: Rect, total: usize, visible: usize, position: usize) {
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 0,
    });
    let mut scrollbar_state = ScrollbarState::new(total)
        .viewport_content_length(visible)
        .position(position);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .track_style(Style::default().fg(Color::DarkGray))
        .thumb_style(Style::default().fg(Color::Cyan));
    f.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
}

fn render_image_overlay(
    f: &mut Frame,
    app: &mut App,
    hash: u64,
    path: &std::path::Path,
    img_area: Rect,
) {
    if img_area.height == 0 {
        return;
    }
    if !app.image_states.contains_key(&hash) {
        if let Ok(dyn_img) = image::open(path) {
            let protocol = app.picker.new_resize_protocol(dyn_img);
            app.image_states.insert(hash, protocol);
        }
    }
    if let Some(state) = app.image_states.get_mut(&hash) {
        let widget =
            ratatui_image::StatefulImage::<ratatui_image::protocol::StatefulProtocol>::new();
        f.render_stateful_widget(widget, img_area, state);
    }
}

struct SegmentLines {
    lines: Vec<Line<'static>>,
    image_segments: Vec<(u64, std::path::PathBuf, u16)>,
    wrapped_height: usize,
}

fn render_markdown_segment(
    app: &mut App,
    segments: &[crate::tui::content::diagram::PreviewSegment],
    panel_width: u16,
    panel_height: u16,
    content_width: usize,
) -> SegmentLines {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut image_segments: Vec<(u64, std::path::PathBuf, u16)> = Vec::new();
    let mut wrapped_height: usize = 0;

    for segment in segments {
        match segment {
            crate::tui::content::diagram::PreviewSegment::Markdown(text) => {
                let gfm_lines = render_markdown_to_lines(text, panel_width);
                wrapped_height += wrapped_lines_total(&gfm_lines, content_width);
                lines.extend(gfm_lines);
            }
            crate::tui::content::diagram::PreviewSegment::DiagramImage(path) => {
                let hash = crate::tui::content::diagram::source_hash_path(path);
                let img_height = get_image_dimensions_cached(app, path)
                    .map(|(w, h)| calculate_image_height(w, h, panel_width, panel_height))
                    .unwrap_or(12);
                image_segments.push((hash, path.clone(), img_height));
                for _ in 0..img_height {
                    lines.push(Line::from(""));
                }
                wrapped_height += img_height as usize;
            }
            crate::tui::content::diagram::PreviewSegment::DiagramText(text) => {
                for line_str in text.lines() {
                    let display_line = Line::from(Span::raw(format!(" {}", line_str)));
                    wrapped_height += wrapped_line_count(&display_line, content_width);
                    lines.push(display_line);
                }
            }
            crate::tui::content::diagram::PreviewSegment::DiagramLoading => {
                lines.push(Line::from(Span::styled(
                    " [rendering diagram...]",
                    Style::default().fg(Color::Yellow),
                )));
                wrapped_height += 1;
            }
            crate::tui::content::diagram::PreviewSegment::DiagramError(msg) => {
                lines.push(Line::from(Span::styled(
                    format!(" [diagram error: {}]", msg),
                    Style::default().fg(Color::Red),
                )));
                wrapped_height += 1;
            }
        }
    }

    SegmentLines {
        lines,
        image_segments,
        wrapped_height,
    }
}

fn render_diagram_overlays(
    f: &mut Frame,
    app: &mut App,
    segments: &[crate::tui::content::diagram::PreviewSegment],
    inner: Rect,
    panel_width: u16,
    header_y_offset: u16,
    scroll_offset: u16,
) {
    let content_width = inner.width as usize;
    let mut y_offset = header_y_offset;

    for segment in segments {
        match segment {
            crate::tui::content::diagram::PreviewSegment::Markdown(text) => {
                let gfm_lines = render_markdown_to_lines(text, panel_width);
                y_offset += wrapped_lines_total(&gfm_lines, content_width) as u16;
            }
            crate::tui::content::diagram::PreviewSegment::DiagramImage(path) => {
                let hash = crate::tui::content::diagram::source_hash_path(path);
                let img_height = get_image_dimensions_cached(app, path)
                    .map(|(w, h)| calculate_image_height(w, h, inner.width, inner.height))
                    .unwrap_or(12);

                if y_offset + img_height > scroll_offset && y_offset >= scroll_offset {
                    let scrolled_y = y_offset - scroll_offset;
                    let img_area = Rect::new(
                        inner.x,
                        inner.y.saturating_add(scrolled_y),
                        inner.width,
                        img_height.min(
                            inner
                                .bottom()
                                .saturating_sub(inner.y.saturating_add(scrolled_y)),
                        ),
                    );
                    if img_area.y < inner.bottom() {
                        render_image_overlay(f, app, hash, path, img_area);
                    }
                }
                y_offset += img_height;
            }
            crate::tui::content::diagram::PreviewSegment::DiagramText(text) => {
                for line_str in text.lines() {
                    let display_line = Line::from(Span::raw(format!(" {}", line_str)));
                    y_offset += wrapped_line_count(&display_line, content_width) as u16;
                }
            }
            crate::tui::content::diagram::PreviewSegment::DiagramLoading => {
                y_offset += 1;
            }
            crate::tui::content::diagram::PreviewSegment::DiagramError(_) => {
                y_offset += 1;
            }
        }
    }
}

fn display_name(path: &std::path::Path) -> &str {
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

const GUTTER_COLS: u16 = 1;
const TREE_COLS: u16 = 4;
const ID_COLS: u16 = 18;
const TITLE_MIN_COLS: u16 = 20;
const STATUS_COLS: u16 = 12;
const TAGS_COLS: u16 = 24;
const PROV_MIN_COLS: u16 = 20;

fn doc_table_widths(area_width: u16) -> [Constraint; 7] {
    let inner = area_width.saturating_sub(2);
    let essentials = GUTTER_COLS + TREE_COLS + ID_COLS + 6;
    let mut remaining = inner
        .saturating_sub(essentials)
        .saturating_sub(TITLE_MIN_COLS);
    let status = if remaining >= STATUS_COLS {
        remaining -= STATUS_COLS;
        STATUS_COLS
    } else {
        0
    };
    let tags = if remaining >= TAGS_COLS {
        remaining -= TAGS_COLS;
        TAGS_COLS
    } else {
        0
    };
    let prov_min = if remaining >= PROV_MIN_COLS {
        PROV_MIN_COLS
    } else {
        0
    };
    [
        Constraint::Length(GUTTER_COLS),
        Constraint::Length(TREE_COLS),
        Constraint::Length(ID_COLS),
        Constraint::Min(TITLE_MIN_COLS),
        Constraint::Length(status),
        Constraint::Length(tags),
        if prov_min > 0 {
            Constraint::Min(prov_min)
        } else {
            Constraint::Length(0)
        },
    ]
}

fn truncate_with_ellipsis(s: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    let total: usize = s
        .chars()
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
        .sum();
    if total <= max_cols {
        return s.to_string();
    }
    if max_cols == 1 {
        return "…".to_string();
    }
    let budget = max_cols - 1;
    let mut acc = 0usize;
    let mut out = String::new();
    for c in s.chars() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if acc + w > budget {
            break;
        }
        acc += w;
        out.push(c);
    }
    out.push('…');
    out
}

fn provenance_cell_text(provenance: &[String], max_cols: usize) -> String {
    if provenance.is_empty() {
        return String::new();
    }
    truncate_with_ellipsis(&provenance.join(", "), max_cols)
}

/// Column widths used by `row_content_lines` and `doc_row_for_node` for
/// soft-wrap measurement. Indices align with cells produced by
/// `doc_row_cells` (title at 1, tags at 3, provenance at 4).
#[derive(Debug, Clone, Copy)]
struct DocCellWidths {
    title: u16,
    tags: u16,
    provenance: u16,
}

impl DocCellWidths {
    /// Resolve cell widths from the available table area width.
    /// Mirrors `doc_table_widths(area_width)` ordering: gutter(1), tree(4),
    /// id(18), title(Min 20), status, tags, provenance. Optional columns
    /// (status, tags, provenance) collapse to width 0 at narrow widths so
    /// soft-wrap measurement stays aligned with the responsive layout.
    fn from_area_width(area_width: u16) -> Self {
        // Mirror the layout ratatui's Table will compute for these
        // constraints so wrap measurements match the real cell rects.
        // `column_spacing` defaults to 1 between adjacent cells.
        let inner_width = area_width.saturating_sub(2);
        let rects = Layout::default()
            .direction(Direction::Horizontal)
            .spacing(1)
            .constraints(doc_table_widths(area_width))
            .split(Rect::new(0, 0, inner_width, 1));
        DocCellWidths {
            title: rects[3].width.max(1),
            tags: rects[5].width.max(1),
            provenance: rects[6].width.max(1),
        }
    }
}

fn wrap_segments(text: &str, width: u16) -> usize {
    if text.is_empty() {
        return 1;
    }
    let w = width.max(1) as usize;
    text.split('\n')
        .map(|seg| {
            if seg.is_empty() {
                1
            } else {
                textwrap::wrap(seg, w).len().max(1)
            }
        })
        .sum()
}

/// Greedy word-wrap the full tag list into styled spans, one Line per
/// row of the cell. Each tag is `[name]` separated by a space; tags
/// never split across lines.
fn tag_wrapped_lines(tags: &[String], width: u16, dim: bool) -> Vec<Line<'static>> {
    if tags.is_empty() {
        return vec![Line::from("")];
    }
    let dim_color = Color::DarkGray;
    let w = width.max(1) as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut cur_width = 0usize;
    for tag in tags {
        let token = format!("[{}]", tag);
        let tlen = token.chars().count();
        let needed = if current.is_empty() { tlen } else { tlen + 1 };
        if cur_width + needed > w && !current.is_empty() {
            lines.push(Line::from(std::mem::take(&mut current)));
            cur_width = 0;
        }
        if !current.is_empty() {
            current.push(Span::raw(" "));
            cur_width += 1;
        }
        let tc = if dim { dim_color } else { tag_color(tag) };
        current.push(Span::styled(token, Style::default().fg(tc)));
        cur_width += tlen;
    }
    if !current.is_empty() {
        lines.push(Line::from(current));
    }
    if lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines
}

/// Compute the natural visual line count for a row's content given the
/// resolved cell widths. Returns the maximum across the title, tags, and
/// provenance cells.
fn row_content_lines(
    title: &str,
    tags: &[String],
    provenance: &[String],
    widths: DocCellWidths,
) -> usize {
    let title_lines = wrap_segments(title, widths.title);
    let tags_lines = tag_wrapped_lines(tags, widths.tags, false).len();
    let prov_text = if provenance.is_empty() {
        String::new()
    } else {
        provenance.join(", ")
    };
    let prov_lines = if prov_text.is_empty() {
        1
    } else {
        wrap_segments(&prov_text, widths.provenance)
    };
    title_lines.max(tags_lines).max(prov_lines)
}

/// Returns `true` when `elapsed_secs` exceeds twice the given `cache_ttl`.
pub(crate) fn is_cache_stale(elapsed_secs: u64, cache_ttl: u64) -> bool {
    elapsed_secs >= 2 * cache_ttl
}

fn check_doc_stale(path: &std::path::Path, doc_type: &str, config: &Config) -> (bool, bool) {
    let is_gh = config
        .type_by_name(doc_type)
        .map(|td| td.store == StoreBackend::GithubIssues)
        .unwrap_or(false);
    let is_stale = if is_gh {
        let cache_ttl = config
            .documents
            .github
            .as_ref()
            .map(|g| g.cache_ttl)
            .unwrap_or(60);
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|elapsed| is_cache_stale(elapsed.as_secs(), cache_ttl))
            .unwrap_or(false)
    } else {
        false
    };
    (is_gh, is_stale)
}

/// Tags column width (must match `Constraint::Length(24)` in `doc_table_widths`).
const TAGS_CELL_WIDTH: usize = 24;

/// Greedy-pack tags into the given width budget. Returns (taken, dropped).
/// When some tags are dropped, reserves space for a ` +N` overflow indicator
/// so the indicator never gets clipped at the cell boundary.
fn pack_tags_to_width(tags: &[String], width: usize) -> (usize, usize) {
    fn token_width(tag: &str) -> usize {
        tag.chars()
            .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
            .sum::<usize>()
            + 2 // brackets
    }
    let pack = |reserve: usize| -> usize {
        let mut consumed = 0usize;
        let mut taken = 0usize;
        for tag in tags {
            let tw = token_width(tag);
            let needed = if taken == 0 { tw } else { tw + 1 };
            if consumed + needed + reserve > width {
                break;
            }
            consumed += needed;
            taken += 1;
        }
        taken
    };
    let no_reserve = pack(0);
    if no_reserve == tags.len() {
        return (no_reserve, 0);
    }
    let indicator_width = format!(" +{}", tags.len()).chars().count();
    let with_reserve = pack(indicator_width);
    (with_reserve, tags.len() - with_reserve)
}

#[allow(clippy::too_many_arguments)]
fn doc_row_cells(
    id: &str,
    title: &str,
    status: &Status,
    tags: &[String],
    provenance: &[String],
    is_virtual: bool,
    dim: bool,
    is_gh: bool,
    is_stale: bool,
) -> Vec<Cell<'static>> {
    let dim_style = Style::default().fg(Color::DarkGray);
    let normal_style = Style::default();

    let id_style = if dim { dim_style } else { normal_style };
    let id_cell = if is_gh {
        let badge_style = if dim {
            dim_style
        } else {
            Style::default().fg(Color::Magenta)
        };
        Cell::new(Line::from(vec![
            Span::styled(format!("{:<18}", id), id_style),
            Span::styled(" [gh]", badge_style),
        ]))
    } else {
        Cell::new(Span::styled(format!("{:<18}", id), id_style))
    };

    let title_text = if is_virtual {
        format!("{} (virtual)", title)
    } else {
        title.to_string()
    };
    let title_style = if dim { dim_style } else { normal_style };
    let title_cell = Cell::new(Span::styled(title_text, title_style));

    let status_style = if dim {
        dim_style
    } else {
        Style::default().fg(status_color(status))
    };
    let status_cell = if is_stale {
        let stale_style = if dim {
            dim_style
        } else {
            Style::default().fg(Color::Red)
        };
        Cell::new(Line::from(vec![
            Span::styled(format!("{:<12}", status), status_style),
            Span::styled(" [!]", stale_style),
        ]))
    } else {
        Cell::new(Span::styled(format!("{:<12}", status), status_style))
    };

    let (take_count, dropped) = pack_tags_to_width(tags, TAGS_CELL_WIDTH);
    let mut tag_spans: Vec<Span<'static>> = Vec::new();
    for (idx, tag) in tags.iter().take(take_count).enumerate() {
        if idx > 0 {
            tag_spans.push(Span::raw(" "));
        }
        let tc = if dim { Color::DarkGray } else { tag_color(tag) };
        tag_spans.push(Span::styled(format!("[{}]", tag), Style::default().fg(tc)));
    }
    if dropped > 0 {
        tag_spans.push(Span::styled(format!(" +{}", dropped), dim_style));
    }
    let tags_cell = Cell::new(Line::from(tag_spans));

    let prov_text = provenance_cell_text(provenance, 20);
    let prov_style = if dim { dim_style } else { normal_style };
    let provenance_cell = Cell::new(Span::styled(prov_text, prov_style));

    vec![id_cell, title_cell, status_cell, tags_cell, provenance_cell]
}

fn wrap_to_lines(text: &str, width: u16, style: Style) -> Vec<Line<'static>> {
    let w = width.max(1) as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    for segment in text.split('\n') {
        if segment.is_empty() {
            lines.push(Line::from(""));
            continue;
        }
        for piece in textwrap::wrap(segment, w) {
            lines.push(Line::from(Span::styled(piece.into_owned(), style)));
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines
}

#[allow(clippy::too_many_arguments)]
fn doc_row_cells_expanded(
    id: &str,
    title: &str,
    status: &Status,
    tags: &[String],
    provenance: &[String],
    is_virtual: bool,
    dim: bool,
    is_gh: bool,
    is_stale: bool,
    widths: DocCellWidths,
) -> Vec<Cell<'static>> {
    // Reuse the single-line cell builder, then replace title and provenance
    // cells with wrapped multi-line versions.
    let mut cells = doc_row_cells(
        id, title, status, tags, provenance, is_virtual, dim, is_gh, is_stale,
    );

    let dim_style = Style::default().fg(Color::DarkGray);
    let normal_style = Style::default();
    let title_style = if dim { dim_style } else { normal_style };
    let prov_style = if dim { dim_style } else { normal_style };

    let title_text = if is_virtual {
        format!("{} (virtual)", title)
    } else {
        title.to_string()
    };
    let title_lines = wrap_to_lines(&title_text, widths.title, title_style);
    cells[1] = Cell::from(title_lines);

    if !tags.is_empty() {
        cells[3] = Cell::from(tag_wrapped_lines(tags, widths.tags, dim));
    }

    if !provenance.is_empty() {
        let prov_text = provenance.join(", ");
        let prov_lines = wrap_to_lines(&prov_text, widths.provenance, prov_style);
        cells[4] = Cell::from(prov_lines);
    }

    cells
}

fn doc_row_for_node(
    app: &App,
    node: &DocListNode,
    index: usize,
    dim: bool,
    config: &Config,
    area_width: u16,
) -> Row<'static> {
    let tree_text = if node.depth > 0 {
        let leading = "   ".repeat(node.depth - 1);
        let is_last = match app.doc_tree.get(index + 1) {
            Some(next) => next.depth < node.depth,
            None => true,
        };
        let connector = if is_last { " └─ " } else { " ├─ " };
        format!("{}{}", leading, connector)
    } else if node.is_parent {
        let indicator = if app.is_expanded(&node.path) {
            "▼ "
        } else {
            "▶ "
        };
        format!("  {}", indicator)
    } else {
        "  ".to_string()
    };
    let tree_cell = Cell::new(Span::styled(
        tree_text,
        Style::default().fg(Color::DarkGray),
    ));

    let gutter_cell = match app.git_status_cache.get(&node.path) {
        Some(GitFileStatus::New) => Cell::from("┃").style(Style::default().fg(Color::Green)),
        Some(GitFileStatus::Modified) => Cell::from("┃").style(Style::default().fg(Color::Yellow)),
        None => Cell::from(" "),
    };

    let tags = app
        .store
        .get(&node.path)
        .map(|doc| doc.tags.clone())
        .unwrap_or_default();
    let provenance = app
        .store
        .get(&node.path)
        .map(|doc| doc.provenance.clone())
        .unwrap_or_default();

    let display_id = if node.has_duplicate_id {
        format!("! {}", node.id)
    } else {
        node.id.clone()
    };

    let (is_gh, is_stale) = check_doc_stale(&node.path, node.doc_type.as_str(), config);

    let widths = DocCellWidths::from_area_width(area_width);
    let content_lines = row_content_lines(&node.title, &tags, &provenance, widths);
    let expanded = app.wrap_mode && index == app.selected_doc;

    let mut cells = vec![gutter_cell, tree_cell];
    if expanded {
        cells.extend(doc_row_cells_expanded(
            &display_id,
            &node.title,
            &node.status,
            &tags,
            &provenance,
            node.is_virtual,
            dim,
            is_gh,
            is_stale,
            widths,
        ));
    } else {
        cells.extend(doc_row_cells(
            &display_id,
            &node.title,
            &node.status,
            &tags,
            &provenance,
            node.is_virtual,
            dim,
            is_gh,
            is_stale,
        ));
    }

    let style = if dim {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };

    let row = Row::new(cells).style(style);
    if expanded {
        let max = config.ui.multiline.max_expanded_height.max(1) as u16;
        let height = (content_lines as u16).min(max).max(1);
        row.height(height)
    } else {
        row
    }
}

pub fn draw_type_panel(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .doc_types
        .iter()
        .map(|dt| {
            let count = app.doc_count(dt);
            let plural = app
                .type_plurals
                .get(&dt.to_string())
                .map(|s| s.as_str())
                .unwrap_or("unknown");
            let content = format!("  {}  ({})", plural, count);
            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Types "),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default().with_selected(Some(app.selected_type));
    f.render_stateful_widget(list, area, &mut state);
}

pub fn draw_doc_list(f: &mut Frame, app: &mut App, area: Rect, config: &Config) {
    app.doc_list_height = area.height.saturating_sub(2) as usize;
    let relations_focused = app.preview_tab == PreviewTab::Relations;
    let dim = relations_focused;

    let area_width = area.width;
    let rows: Vec<Row> = app
        .doc_tree
        .iter()
        .enumerate()
        .map(|(i, node)| doc_row_for_node(app, node, i, dim, config, area_width))
        .collect();

    let widths = doc_table_widths(area.width);

    let border_style = if relations_focused {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let highlight_style = if relations_focused {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::REVERSED)
    };

    let table = Table::new(rows, widths)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(" Documents "),
        )
        .row_highlight_style(highlight_style);

    let mut state = TableState::default()
        .with_selected(Some(app.selected_doc))
        .with_offset(app.doc_list_offset);
    f.render_stateful_widget(table, area, &mut state);

    let total_items = app.doc_tree.len();
    if !dim && total_items > app.doc_list_height {
        render_scrollbar(f, area, total_items, app.doc_list_height, app.selected_doc);
    }
}

pub fn draw_preview(f: &mut Frame, app: &mut App, area: Rect) {
    let preview_title = if app.preview_tab == PreviewTab::Preview {
        Line::from(vec![
            Span::styled(
                " Preview ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("| "),
            Span::styled("Relations ", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Preview ", Style::default().fg(Color::DarkGray)),
            Span::raw("| "),
            Span::styled(
                "Relations ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    };

    let border_style = if app.preview_tab == PreviewTab::Relations {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(preview_title);

    let doc = app.selected_doc_meta().cloned();
    match app.preview_tab {
        PreviewTab::Preview => render_document_preview(f, app, area, block, doc.as_ref()),
        PreviewTab::Relations => render_relationship_sections(f, app, area, block, doc.as_ref()),
    }
}

pub(super) fn build_preview_header_lines(doc: &DocMeta, expanding: bool) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            format!(" {}", doc.title),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::raw(" Type: "),
            Span::styled(
                format!("{}", doc.doc_type),
                Style::default().fg(Color::White),
            ),
            Span::raw("  Status: "),
            Span::styled(
                format!("{}", doc.status),
                Style::default().fg(status_color(&doc.status)),
            ),
            Span::raw("  Author: "),
            Span::raw(doc.author.clone()),
        ]),
        Line::from(vec![Span::raw(format!(" Date: {}", doc.date))]),
    ];

    if !doc.tags.is_empty() {
        let mut tag_spans: Vec<Span<'static>> = vec![Span::raw(" Tags: ")];
        for (idx, tag) in doc.tags.iter().enumerate() {
            if idx > 0 {
                tag_spans.push(Span::raw(" "));
            }
            tag_spans.push(Span::styled(
                format!("[{}]", tag),
                Style::default().fg(tag_color(tag)),
            ));
        }
        lines.push(Line::from(tag_spans));
    }

    if !doc.provenance.is_empty() {
        let mut spans: Vec<Span<'static>> = vec![Span::raw(" Provenance: ")];
        for (idx, entry) in doc.provenance.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::raw(", "));
            }
            spans.push(Span::raw(entry.clone()));
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));

    if expanding {
        lines.push(Line::from(Span::styled(
            " [expanding refs...]",
            Style::default().fg(Color::Yellow),
        )));
    }

    lines
}

pub fn render_document_preview(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    block: Block,
    doc: Option<&DocMeta>,
) {
    let Some(doc) = doc else {
        let paragraph = Paragraph::new(" No document selected.")
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
        return;
    };

    let body = app
        .expanded_body_cache
        .get(&doc.path)
        .cloned()
        .unwrap_or_default();

    let expanding = app.expansion_in_flight.as_ref() == Some(&doc.path);
    let header_lines = build_preview_header_lines(doc, expanding);
    let mut lines = header_lines.clone();

    let diagram_blocks = match &app.diagram_blocks_cache {
        Some((p, _, b)) if p == &doc.path => b.clone(),
        _ => crate::tui::content::diagram::extract_diagram_blocks(&body),
    };
    let panel_width = area.width.saturating_sub(2);
    let panel_height = area.height.saturating_sub(2);
    let segments = crate::tui::content::diagram::build_preview_segments(
        &body,
        &app.diagram_cache,
        app.terminal_image_protocol,
        &app.tool_availability,
        &diagram_blocks,
    );

    let content_width = area.width.saturating_sub(2) as usize;
    let segment_lines =
        render_markdown_segment(app, &segments, panel_width, panel_height, content_width);
    let has_images = !segment_lines.image_segments.is_empty();
    lines.extend(segment_lines.lines);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);

    if has_images {
        let inner = area.inner(ratatui::layout::Margin {
            horizontal: 1,
            vertical: 1,
        });
        let header_y = wrapped_lines_total(&header_lines, inner.width as usize) as u16;
        let segments_ref = crate::tui::content::diagram::build_preview_segments(
            &body,
            &app.diagram_cache,
            app.terminal_image_protocol,
            &app.tool_availability,
            &diagram_blocks,
        );
        render_diagram_overlays(f, app, &segments_ref, inner, panel_width, header_y, 0);
    }
}

pub fn render_relationship_sections(
    f: &mut Frame,
    app: &App,
    area: Rect,
    block: Block,
    doc: Option<&DocMeta>,
) {
    let Some(doc) = doc else {
        let paragraph = Paragraph::new(" No document selected.")
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
        return;
    };

    let all_items = app.relation_items(doc);

    if all_items.is_empty() {
        let paragraph = Paragraph::new(" No relations.")
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
        return;
    }

    let mut chain_paths = Vec::new();
    {
        let mut current_path = doc.path.clone();
        while let Some(current_doc) = app.store.get(&current_path) {
            let implements_target = current_doc.related.iter().find_map(|r| {
                if r.rel_type == RelationType::Implements {
                    if let Some(fwd) = app.store.forward_links.get(&current_doc.path) {
                        for (rel, target) in fwd {
                            if *rel == RelationType::Implements {
                                return Some(target.clone());
                            }
                        }
                    }
                    None
                } else {
                    None
                }
            });
            match implements_target {
                Some(parent) => {
                    chain_paths.push(parent.clone());
                    current_path = parent;
                }
                None => break,
            }
        }
        chain_paths.reverse();
    }

    let mut children_paths = Vec::new();
    if let Some(rev) = app.store.reverse_links.get(&doc.path) {
        for (rel, source) in rev {
            if *rel == RelationType::Implements {
                children_paths.push(source.clone());
            }
        }
    }

    let mut related_paths = Vec::new();
    if let Some(fwd) = app.store.forward_links.get(&doc.path) {
        for (rel, target) in fwd {
            if *rel == RelationType::RelatedTo {
                related_paths.push(target.clone());
            }
        }
    }
    if let Some(rev) = app.store.reverse_links.get(&doc.path) {
        for (rel, source) in rev {
            if *rel == RelationType::RelatedTo {
                related_paths.push(source.clone());
            }
        }
    }

    let mut items: Vec<ListItem> = Vec::new();
    let mut flat_index = 0usize;
    let mut list_index = 0usize;
    let mut selected_flat_index = 0usize;

    let section_header = |label: &str| -> ListItem {
        ListItem::new(Line::from(Span::styled(
            format!("  {}", label),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )))
    };

    let render_item = |path: &std::path::Path| -> ListItem {
        let (title, doc_type_str, status_str, status_clr) =
            if let Some(target_doc) = app.store.get(path) {
                (
                    target_doc.title.clone(),
                    format!("{}", target_doc.doc_type),
                    format!("{}", target_doc.status),
                    status_color(&target_doc.status),
                )
            } else {
                let name = display_name(path);
                (
                    name.to_string(),
                    "?".to_string(),
                    "missing".to_string(),
                    Color::Red,
                )
            };

        ListItem::new(Line::from(vec![
            Span::raw("    "),
            Span::styled(format!("{:<35} ", title), Style::default()),
            Span::styled(
                format!("{} ", doc_type_str),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(format!("[{}]", status_str), Style::default().fg(status_clr)),
        ]))
    };

    let sections: Vec<(&str, &Vec<std::path::PathBuf>)> = vec![
        ("chain", &chain_paths),
        ("children", &children_paths),
        ("related", &related_paths),
    ];

    for (label, paths) in &sections {
        if paths.is_empty() {
            continue;
        }

        items.push(section_header(label));
        list_index += 1;

        for path in *paths {
            if flat_index == app.selected_relation {
                selected_flat_index = list_index;
            }
            items.push(render_item(path));
            flat_index += 1;
            list_index += 1;
        }
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("  > ");
    let total_items = list_index;
    let mut state = ListState::default().with_selected(Some(selected_flat_index));
    f.render_stateful_widget(list, area, &mut state);

    let visible_height = area.height.saturating_sub(2) as usize;
    if total_items > visible_height {
        render_scrollbar(f, area, total_items, visible_height, selected_flat_index);
    }
}

pub fn render_fullscreen_document(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.fullscreen_height = area.height.saturating_sub(2) as usize;

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    let Some(doc) = app.selected_doc_meta() else {
        return;
    };

    let header = Line::from(vec![
        Span::styled(
            format!(" {} ", doc.title),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled(
            format!("{}", doc.status),
            Style::default().fg(status_color(&doc.status)),
        ),
        Span::raw(format!(" | {} | {} ", doc.doc_type, doc.author)),
        Span::styled("[Esc] back", Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(header), layout[0]);

    let body = app
        .expanded_body_cache
        .get(&doc.path)
        .cloned()
        .unwrap_or_default();

    let expanding = app.expansion_in_flight.as_ref() == Some(&doc.path);
    let display_body = if expanding {
        format!("[expanding refs...]\n\n{}", body)
    } else {
        body
    };

    let content_width = layout[1].width.saturating_sub(2) as usize;
    let panel_width = layout[1].width.saturating_sub(2);
    let panel_height = layout[1].height.saturating_sub(2);

    let fullscreen_blocks = match &app.diagram_blocks_cache {
        Some((p, _, b)) if p == &doc.path => b.clone(),
        _ => crate::tui::content::diagram::extract_diagram_blocks(&display_body),
    };
    let segments = crate::tui::content::diagram::build_preview_segments(
        &display_body,
        &app.diagram_cache,
        app.terminal_image_protocol,
        &app.tool_availability,
        &fullscreen_blocks,
    );

    let segment_lines =
        render_markdown_segment(app, &segments, panel_width, panel_height, content_width);
    let total_lines = segment_lines.wrapped_height;

    let paragraph = Paragraph::new(segment_lines.lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset, 0));
    f.render_widget(paragraph, layout[1]);

    let inner = layout[1].inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });
    render_diagram_overlays(f, app, &segments, inner, panel_width, 0, app.scroll_offset);

    if total_lines > app.fullscreen_height {
        render_scrollbar(
            f,
            layout[1],
            total_lines,
            app.fullscreen_height,
            app.scroll_offset as usize,
        );
    }
}

pub fn render_filter_panel(f: &mut Frame, app: &mut App, area: Rect, config: &Config) {
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(area);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main[1]);

    let status_value = match &app.filter_status {
        None => "all".to_string(),
        Some(s) => format!("{}", s),
    };
    let tag_value = match &app.filter_tag {
        None => "all".to_string(),
        Some(t) => t.clone(),
    };

    let status_style = if app.filter_focused == FilterField::Status {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if app.filter_status.is_some() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let tag_style = if app.filter_focused == FilterField::Tag {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if app.filter_tag.is_some() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let clear_style = if app.filter_focused == FilterField::ClearAction {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let filter_lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  Status: [{}]", status_value),
            status_style,
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  Tag:    [{}]", tag_value),
            tag_style,
        )),
        Line::from(""),
        Line::from(Span::styled("  [clear filters]", clear_style)),
    ];

    let filter_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Filters ");
    let filter_paragraph = Paragraph::new(filter_lines).block(filter_block);
    f.render_widget(filter_paragraph, main[0]);

    app.doc_list_height = right[0].height.saturating_sub(2) as usize;
    let filtered_count = app.filtered_docs_count();
    let total_count = app.store.all_docs().len();

    let relations_focused = app.preview_tab == PreviewTab::Relations;
    let dim = relations_focused;

    let filtered_paths: Vec<PathBuf> = app.filtered_docs_cache.clone().unwrap_or_default();

    let rows: Vec<Row> = filtered_paths
        .iter()
        .filter_map(|p| app.store.get(p))
        .map(|doc| {
            let gutter_cell = match app.git_status_cache.get(&doc.path) {
                Some(GitFileStatus::New) => {
                    Cell::from("┃").style(Style::default().fg(Color::Green))
                }
                Some(GitFileStatus::Modified) => {
                    Cell::from("┃").style(Style::default().fg(Color::Yellow))
                }
                None => Cell::from(" "),
            };
            let tree_cell = Cell::new("");
            let mut cells = vec![gutter_cell, tree_cell];
            let (is_gh, is_stale) = check_doc_stale(&doc.path, doc.doc_type.as_str(), config);
            cells.extend(doc_row_cells(
                &doc.id,
                &doc.title,
                &doc.status,
                &doc.tags,
                &doc.provenance,
                doc.virtual_doc,
                dim,
                is_gh,
                is_stale,
            ));
            let style = if dim {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            Row::new(cells).style(style)
        })
        .collect();

    let widths = doc_table_widths(right[0].width);

    let border_style = if relations_focused {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let highlight_style = if relations_focused {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::REVERSED)
    };

    let table = Table::new(rows, widths)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(format!(
                    " Documents ({} of {}) ",
                    filtered_count, total_count
                )),
        )
        .row_highlight_style(highlight_style);

    let mut state = TableState::default()
        .with_selected(Some(app.selected_doc))
        .with_offset(app.doc_list_offset);
    f.render_stateful_widget(table, right[0], &mut state);

    let doc = app.selected_filtered_doc().cloned();
    let preview_title = if app.preview_tab == PreviewTab::Preview {
        Line::from(vec![
            Span::styled(
                " Preview ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("| "),
            Span::styled("Relations ", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Preview ", Style::default().fg(Color::DarkGray)),
            Span::raw("| "),
            Span::styled(
                "Relations ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    };

    let preview_border_style = if app.preview_tab == PreviewTab::Relations {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(preview_border_style)
        .title(preview_title);

    match app.preview_tab {
        PreviewTab::Preview => render_document_preview(f, app, right[1], block, doc.as_ref()),
        PreviewTab::Relations => {
            render_relationship_sections(f, app, right[1], block, doc.as_ref())
        }
    }
}

#[cfg(feature = "agent")]
fn agent_status_icon(status: Option<&AgentStatus>) -> (&'static str, Color) {
    match status {
        Some(AgentStatus::Running) => ("●", Color::Yellow),
        Some(AgentStatus::Crashed) => ("!", Color::Red),
        None => ("○", Color::DarkGray),
    }
}

#[cfg(feature = "agent")]
fn short_session(session_id: &str) -> &str {
    session_id.split('-').next().unwrap_or(session_id)
}

#[cfg(feature = "agent")]
fn agent_row_cells(
    status: Option<&AgentStatus>,
    session_id: &str,
    doc_id: &str,
    elapsed: std::time::Duration,
    tokens_in: u64,
    tokens_out: u64,
) -> Vec<Cell<'static>> {
    let (icon, color) = agent_status_icon(status);
    let short = short_session(session_id);
    vec![
        Cell::from(Span::styled(
            format!(" {} ", icon),
            Style::default().fg(color),
        )),
        Cell::from(Span::raw(format!("{:<10.10}", short))),
        Cell::from(Span::raw(truncate_with_ellipsis(doc_id, 12))),
        Cell::from(Span::raw(super::layout::format_elapsed(elapsed))),
        Cell::from(Span::raw(format!("{}/{}", tokens_in, tokens_out))),
    ]
}

#[cfg(feature = "agent")]
pub fn draw_agents_screen(f: &mut Frame, app: &App, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let main_area = layout[0];
    let footer_area = layout[1];

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main_area);

    let left_area = main[0];
    let right_area = main[1];

    let left_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Agents ");

    if app.agents_view.snapshots.is_empty() {
        let paragraph = Paragraph::new("No agents yet. Press n to start one.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(ratatui::layout::Alignment::Center)
            .block(left_block);
        f.render_widget(paragraph, left_area);
    } else {
        let rows: Vec<Row> = app
            .agents_view
            .snapshots
            .values()
            .map(|snap| {
                let status = app.agents_view.statuses.get(&snap.session_id);
                let elapsed_ms = app
                    .agents_view
                    .effective_elapsed_ms(&snap.session_id)
                    .unwrap_or(snap.elapsed_ms);
                let cells = agent_row_cells(
                    status,
                    &snap.session_id,
                    &snap.doc_id,
                    std::time::Duration::from_millis(elapsed_ms),
                    snap.tokens_in,
                    snap.tokens_out,
                );
                Row::new(cells)
            })
            .collect();

        let widths = [
            Constraint::Length(3),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(9),
            Constraint::Length(16),
        ];

        let table = Table::new(rows, widths)
            .block(left_block)
            .header(
                Row::new(vec![
                    "  ",
                    "Session",
                    "Document",
                    "Elapsed",
                    "Tokens (in/out)",
                ])
                .style(
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                ),
            )
            .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        let mut state = TableState::default().with_selected(Some(app.agents_view.selected));
        f.render_stateful_widget(table, left_area, &mut state);
    }

    let selected_session = app.agents_view.selected_session();
    let right_title = match selected_session {
        Some(sid) => format!(" Output -- {} ", short_session(sid)),
        None => " Output ".to_string(),
    };
    let right_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(right_title);

    match selected_session {
        None => {
            let paragraph = Paragraph::new("No agent selected.")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(ratatui::layout::Alignment::Center)
                .block(right_block);
            f.render_widget(paragraph, right_area);
        }
        Some(sid) => {
            let buf = app.agents_view.output.get(sid);
            match buf {
                None => {
                    let paragraph = Paragraph::new("No output yet...")
                        .style(Style::default().fg(Color::DarkGray))
                        .alignment(ratatui::layout::Alignment::Center)
                        .block(right_block);
                    f.render_widget(paragraph, right_area);
                }
                Some(text) if text.is_empty() => {
                    let paragraph = Paragraph::new("No output yet...")
                        .style(Style::default().fg(Color::DarkGray))
                        .alignment(ratatui::layout::Alignment::Center)
                        .block(right_block);
                    f.render_widget(paragraph, right_area);
                }
                Some(text) => {
                    let lines_in_buffer = text.lines().count() as u16;
                    let visible_lines = right_area.height.saturating_sub(2);
                    let scroll_offset = lines_in_buffer.saturating_sub(visible_lines);
                    let paragraph = Paragraph::new(text.as_str())
                        .block(right_block)
                        .wrap(Wrap { trim: false })
                        .scroll((scroll_offset, 0));
                    f.render_widget(paragraph, right_area);
                }
            }
        }
    }

    let footer = Line::from(vec![
        Span::styled("j/k", Style::default().fg(Color::Cyan)),
        Span::raw(": select  "),
        Span::styled("n", Style::default().fg(Color::Cyan)),
        Span::raw(": new agent  "),
        Span::styled("`", Style::default().fg(Color::Cyan)),
        Span::raw(": switch view"),
    ]);
    f.render_widget(
        Paragraph::new(footer).style(Style::default().fg(Color::DarkGray)),
        footer_area,
    );
}

#[cfg(all(test, feature = "agent"))]
pub(super) fn agent_row_cells_for_test(
    status: Option<AgentStatus>,
    session_id: &str,
    doc_id: &str,
    elapsed: std::time::Duration,
    tokens_in: u64,
    tokens_out: u64,
) -> Vec<Cell<'static>> {
    agent_row_cells(
        status.as_ref(),
        session_id,
        doc_id,
        elapsed,
        tokens_in,
        tokens_out,
    )
}

#[cfg(feature = "metrics")]
pub fn draw_metrics_skeleton(f: &mut Frame, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(area);

    let left = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Metrics ");
    f.render_widget(left, layout[0]);

    let right = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Status Flow ");
    f.render_widget(right, layout[1]);
}

pub fn draw_graph(f: &mut Frame, app: &App, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(area);

    let left = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Graph ");
    f.render_widget(left, layout[0]);

    let items: Vec<ListItem> = app
        .graph_nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let mut spans = Vec::new();

            if node.depth > 0 {
                let leading = "   ".repeat(node.depth - 1);
                let is_last = match app.graph_nodes.get(i + 1) {
                    Some(next) => next.depth <= node.depth,
                    None => true,
                };
                let connector = if is_last {
                    " └─▶ "
                } else {
                    " ├─▶ "
                };
                spans.push(Span::styled(
                    format!("{}{}", leading, connector),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            let type_icon = app
                .type_icons
                .get(&node.doc_type.to_string())
                .map(|s| s.as_str())
                .unwrap_or("○");
            spans.push(Span::styled(
                format!("{} ", type_icon),
                Style::default().fg(Color::Gray),
            ));

            spans.push(Span::styled(
                format!("{} ", node.title),
                Style::default().fg(Color::White),
            ));

            spans.push(Span::styled(
                format!("{}", node.status),
                Style::default().fg(status_color(&node.status)),
            ));

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Dependency Graph "),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default().with_selected(Some(app.graph_selected));
    f.render_stateful_widget(list, layout[1], &mut state);
}

#[cfg(test)]
pub(super) fn doc_row_cells_for_test(
    id: &str,
    title: &str,
    status: &Status,
    tags: &[String],
    provenance: &[String],
    is_virtual: bool,
    dim: bool,
) -> Vec<Cell<'static>> {
    doc_row_cells(
        id, title, status, tags, provenance, is_virtual, dim, false, false,
    )
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) fn doc_row_cells_gh_for_test(
    id: &str,
    title: &str,
    status: &Status,
    tags: &[String],
    provenance: &[String],
    is_virtual: bool,
    dim: bool,
    is_gh: bool,
) -> Vec<Cell<'static>> {
    doc_row_cells(
        id, title, status, tags, provenance, is_virtual, dim, is_gh, false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_cache_stale_returns_false_within_threshold() {
        assert!(!is_cache_stale(119, 60));
    }

    #[test]
    fn is_cache_stale_returns_true_at_boundary() {
        assert!(is_cache_stale(120, 60));
    }

    #[test]
    fn is_cache_stale_returns_true_beyond_threshold() {
        assert!(is_cache_stale(121, 60));
    }

    #[test]
    fn is_cache_stale_zero_ttl() {
        assert!(is_cache_stale(1, 0));
        assert!(is_cache_stale(0, 0));
    }

    #[test]
    fn is_cache_stale_large_ttl() {
        assert!(!is_cache_stale(3599, 1800));
        assert!(is_cache_stale(3600, 1800));
        assert!(is_cache_stale(3601, 1800));
    }

    #[test]
    fn truncate_no_change_when_fits() {
        assert_eq!(truncate_with_ellipsis("ab", 5), "ab");
    }

    #[test]
    fn truncate_appends_ellipsis_when_overflows() {
        assert_eq!(truncate_with_ellipsis("abcdef", 4), "abc…");
    }

    #[test]
    fn truncate_zero_width_returns_empty() {
        assert_eq!(truncate_with_ellipsis("abc", 0), "");
    }

    #[test]
    fn truncate_one_width_returns_ellipsis() {
        assert_eq!(truncate_with_ellipsis("abc", 1), "…");
    }

    fn cell_text(cell: &Cell) -> String {
        format!("{:?}", cell)
    }

    #[test]
    fn doc_row_cells_appends_provenance_cell() {
        let provenance = vec!["Alice".to_string()];
        let cells = doc_row_cells_for_test(
            "RFC-001",
            "Title",
            &Status::Draft,
            &[],
            &provenance,
            false,
            false,
        );
        assert_eq!(cells.len(), 5);
        let dbg = cell_text(&cells[4]);
        assert!(
            dbg.contains("Alice"),
            "provenance cell should contain joined entries, got: {}",
            dbg
        );
    }

    #[test]
    fn doc_row_cells_provenance_empty_when_list_empty() {
        let cells =
            doc_row_cells_for_test("RFC-001", "Title", &Status::Draft, &[], &[], false, false);
        let dbg = cell_text(&cells[4]);
        assert!(
            !dbg.contains("Alice") && !dbg.contains('…'),
            "empty provenance cell should not show entries, got: {}",
            dbg
        );
        assert_eq!(provenance_cell_text(&[], 20), "");
    }

    #[test]
    fn doc_row_cells_provenance_comma_joined() {
        let provenance = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let text = provenance_cell_text(&provenance, 20);
        assert_eq!(text, "A, B, C");
    }

    #[test]
    fn doc_row_cells_provenance_truncated_overflow() {
        let provenance = vec!["aaaaaaaaaaaaaaa".to_string(), "bbbbbbbbbbbbbbb".to_string()];
        let text = provenance_cell_text(&provenance, 20);
        assert!(
            text.ends_with('…'),
            "overflowing provenance should end with ellipsis, got: {}",
            text
        );
    }

    fn line_text(line: &Line) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    }

    fn fixture_doc_meta() -> DocMeta {
        use crate::engine::document::DocType;
        use chrono::NaiveDate;
        DocMeta {
            id: "RFC-001".to_string(),
            doc_type: DocType::new("rfc"),
            title: "Test".to_string(),
            status: Status::Draft,
            author: "jkaloger".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 4, 29).unwrap(),
            tags: vec![],
            related: vec![],
            provenance: vec![],
            validate_ignore: false,
            path: PathBuf::from("docs/rfcs/RFC-001.md"),
            virtual_doc: false,
            assignees: vec![],
        }
    }

    #[test]
    fn preview_header_includes_provenance_when_present() {
        let mut doc = fixture_doc_meta();
        doc.provenance = vec!["X".to_string(), "Y".to_string()];
        let lines = build_preview_header_lines(&doc, false);
        let prov_line = lines
            .iter()
            .find(|l| line_text(l).contains("Provenance:"))
            .expect("provenance line should be present");
        let text = line_text(prov_line);
        assert!(text.contains('X'), "should contain X, got: {}", text);
        assert!(text.contains('Y'), "should contain Y, got: {}", text);
    }

    fn widths_for_test(title: u16, tags: u16, provenance: u16) -> DocCellWidths {
        DocCellWidths {
            title,
            tags,
            provenance,
        }
    }

    #[test]
    fn row_content_lines_single_line_inputs_returns_one() {
        let lines = row_content_lines("short", &[], &[], widths_for_test(40, 24, 20));
        assert_eq!(lines, 1);
    }

    #[test]
    fn row_content_lines_counts_explicit_newlines_in_title() {
        let title = "line1\nline2\nline3";
        let lines = row_content_lines(title, &[], &[], widths_for_test(80, 24, 20));
        assert_eq!(lines, 3);
    }

    #[test]
    fn row_content_lines_soft_wraps_long_title() {
        // 30-char title soft-wrapped into width 10 should produce >1 lines.
        let title = "alpha beta gamma delta epsilon zeta";
        let lines = row_content_lines(title, &[], &[], widths_for_test(10, 24, 20));
        assert!(lines > 1, "expected wrap, got {}", lines);
    }

    #[test]
    fn row_content_lines_takes_max_across_cells() {
        // Title fits on 1 line; provenance wraps to multiple.
        let provenance: Vec<String> = (0..5).map(|i| format!("contributor-{}", i)).collect();
        let lines = row_content_lines("t", &[], &provenance, widths_for_test(80, 24, 10));
        assert!(lines > 1);
    }

    #[test]
    fn doc_cell_widths_resolves_title_from_area() {
        // Widths come from ratatui's Layout for the doc-table constraints.
        // Title (Fill) and provenance (Min 20) flex; tags is fixed at 24.
        let widths = DocCellWidths::from_area_width(200);
        assert!(widths.title > 0);
        assert!(widths.title < 200);
        assert_eq!(widths.tags, 24);
        assert!(widths.provenance >= 20);
    }

    #[test]
    fn doc_cell_widths_title_scales_with_area() {
        let small = DocCellWidths::from_area_width(80);
        let large = DocCellWidths::from_area_width(200);
        assert!(large.title > small.title);
    }

    #[test]
    fn doc_cell_widths_clamps_to_min_one_when_area_tiny() {
        let widths = DocCellWidths::from_area_width(10);
        assert!(widths.title >= 1);
        assert!(widths.tags >= 1);
        assert!(widths.provenance >= 1);
    }

    #[test]
    fn expanded_height_is_clamped_by_config_max() {
        // Sanity: simulate the row-height clamp logic used in doc_row_for_node.
        let cfg = crate::engine::config::MultiLineConfig {
            max_expanded_height: 3,
        };
        let content_lines: usize = 10;
        let max = cfg.max_expanded_height.max(1) as u16;
        let height = (content_lines as u16).min(max).max(1);
        assert_eq!(height, 3);
    }

    #[test]
    fn tag_wrapped_lines_single_line_when_fits() {
        let tags = vec!["a".to_string(), "b".to_string()];
        let lines = tag_wrapped_lines(&tags, 24, false);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn tag_wrapped_lines_multi_line_when_overflow() {
        let tags: Vec<String> = (0..6).map(|i| format!("tag-{}", i)).collect();
        let lines = tag_wrapped_lines(&tags, 12, false);
        assert!(lines.len() > 1, "expected wrap, got {}", lines.len());
    }

    #[test]
    fn tag_wrapped_lines_empty_returns_one_blank_line() {
        let lines = tag_wrapped_lines(&[], 24, false);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn pack_tags_all_fit_returns_zero_dropped() {
        let tags = vec!["a".to_string(), "b".to_string()];
        let (taken, dropped) = pack_tags_to_width(&tags, 24);
        assert_eq!(taken, 2);
        assert_eq!(dropped, 0);
    }

    #[test]
    fn pack_tags_count_overflow_drops_remainder() {
        // Many short tags whose combined width exceeds the cell.
        let tags: Vec<String> = (0..10).map(|i| format!("tag-{}", i)).collect();
        let (taken, dropped) = pack_tags_to_width(&tags, 24);
        assert!(taken >= 1);
        assert_eq!(taken + dropped, tags.len());
        assert!(dropped > 0);
    }

    #[test]
    fn pack_tags_width_overflow_with_few_tags_drops_some() {
        // Two long tags exceeding the cell width — must drop at least one
        // and leave space for the indicator.
        let tags = vec![
            "needs-architecture-review".to_string(),
            "blocked-on-upstream".to_string(),
        ];
        let (taken, dropped) = pack_tags_to_width(&tags, 24);
        assert!(
            dropped > 0,
            "expected width overflow to drop at least one tag"
        );
        assert_eq!(taken + dropped, tags.len());
    }

    #[test]
    fn pack_tags_reserves_space_for_indicator() {
        // 3 tags that just-barely fit without indicator must still leave
        // room for the indicator when at least one is dropped.
        let tags = vec![
            "aaaaa".to_string(), // [aaaaa] = 7
            "bbbbb".to_string(), // [bbbbb] = 7 (+1 sep = 8)
            "ccccc".to_string(), // [ccccc] = 7 (+1 sep = 8); total = 23
            "dddd".to_string(),
        ];
        let (taken, dropped) = pack_tags_to_width(&tags, 24);
        // Without reservation 3 tags fit (23 ≤ 24); with reservation for
        // " +1" (3 cols) the third must drop so total ≤ 21.
        assert!(
            taken < 3,
            "should reserve indicator space, got taken={}",
            taken
        );
        assert!(dropped >= 2);
    }

    #[test]
    fn row_content_lines_includes_all_tags_not_just_first_three() {
        // Many tags in narrow column should drive row line count up.
        let tags: Vec<String> = (0..10).map(|i| format!("tag-{}", i)).collect();
        let lines = row_content_lines("t", &tags, &[], widths_for_test(80, 12, 20));
        assert!(
            lines > 1,
            "expected multi-line from tag wrap, got {}",
            lines
        );
    }

    #[test]
    fn expanded_row_cells_render_full_tag_list() {
        let tags: Vec<String> = (0..6).map(|i| format!("tag-{}", i)).collect();
        let cells = doc_row_cells_expanded(
            "RFC-001",
            "Title",
            &Status::Draft,
            &tags,
            &[],
            false,
            false,
            false,
            false,
            widths_for_test(80, 12, 20),
        );
        let dbg = format!("{:?}", cells[3]);
        for tag in &tags {
            assert!(
                dbg.contains(tag),
                "expanded tags cell should contain {}, got: {}",
                tag,
                dbg
            );
        }
        assert!(
            !dbg.contains(" +"),
            "expanded tags cell should not show '+N' counter, got: {}",
            dbg
        );
    }

    #[test]
    fn preview_header_omits_provenance_when_empty() {
        let doc = fixture_doc_meta();
        let lines = build_preview_header_lines(&doc, false);
        for line in &lines {
            assert!(
                !line_text(line).contains("Provenance:"),
                "no line should mention Provenance when empty"
            );
        }
    }

    fn resolve_doc_widths(width: u16) -> Vec<u16> {
        let inner = width.saturating_sub(2);
        Layout::default()
            .direction(Direction::Horizontal)
            .spacing(1)
            .constraints(doc_table_widths(width))
            .split(Rect::new(0, 0, inner, 1))
            .iter()
            .map(|r| r.width)
            .collect()
    }

    #[test]
    fn doc_table_widths_wide_shows_all_columns() {
        let widths = resolve_doc_widths(200);
        assert!(widths[3] >= 20, "title >= 20, got {}", widths[3]);
        assert_eq!(widths[4], 12, "status == 12");
        assert_eq!(widths[5], 24, "tags == 24");
        assert!(widths[6] >= 20, "provenance >= 20, got {}", widths[6]);
    }

    #[test]
    fn doc_table_widths_medium_drops_provenance() {
        // width=90: inner=88. After essentials+title (49) + status (12) + tags (24) = 85,
        // leaving 3 cols — below PROV_MIN_COLS so provenance collapses to 0.
        let widths = resolve_doc_widths(90);
        assert_eq!(widths[6], 0, "provenance dropped");
        assert_eq!(widths[5], 24, "tags retained");
        assert_eq!(widths[4], 12, "status retained");
        assert!(widths[3] >= 20, "title >= 20, got {}", widths[3]);
    }

    #[test]
    fn doc_table_widths_narrow_drops_tags_and_provenance() {
        // width=70: inner=68. After essentials+title (49) + status (12) = 61,
        // leaving 7 cols — below TAGS_COLS, so tags and provenance collapse.
        let widths = resolve_doc_widths(70);
        assert_eq!(widths[5], 0, "tags dropped");
        assert_eq!(widths[6], 0, "provenance dropped");
        assert_eq!(widths[4], 12, "status retained");
        assert!(widths[3] >= 20, "title >= 20, got {}", widths[3]);
    }

    #[test]
    fn doc_table_widths_very_narrow_drops_status() {
        // width=50: inner=48, after essentials (29) = 19 — below TITLE_MIN_COLS,
        // so title takes remaining and status/tags/provenance collapse.
        let widths = resolve_doc_widths(50);
        assert_eq!(widths[4], 0, "status dropped");
        assert_eq!(widths[5], 0, "tags dropped");
        assert_eq!(widths[6], 0, "provenance dropped");
        assert!(
            widths[3] > 0,
            "title gets remaining budget, got {}",
            widths[3]
        );
    }

    #[test]
    fn doc_cell_widths_match_constraint_split() {
        // `from_area_width` clamps tags/provenance to .max(1) so wrap-math
        // never divides by zero; mirror that here when comparing.
        let resolved = resolve_doc_widths(80);
        let cells = DocCellWidths::from_area_width(80);
        assert_eq!(cells.title, resolved[3].max(1), "title agrees with split");
        assert_eq!(cells.tags, resolved[5].max(1), "tags agrees with split");
        assert_eq!(
            cells.provenance,
            resolved[6].max(1),
            "provenance agrees with split"
        );
    }

    #[test]
    fn doc_table_widths_preserves_id_and_tree_at_all_widths() {
        // Lower bound 60: at narrower widths the inner area cannot fit
        // gutter+tree+id+title-min plus 6 column spacings (1+4+18+20+6=49,
        // needs inner >= 49 i.e. width >= 51), so ratatui shrinks the Length
        // constraints. Below that floor AC-5 is physically unsatisfiable.
        for width in [60u16, 80, 120, 200] {
            let widths = resolve_doc_widths(width);
            assert_eq!(widths[0], 1, "gutter == 1 at width {}", width);
            assert_eq!(widths[1], 4, "tree == 4 at width {}", width);
            assert_eq!(widths[2], 18, "ID == 18 at width {}", width);
        }
    }
}
