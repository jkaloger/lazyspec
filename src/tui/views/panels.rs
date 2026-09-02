use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, List, ListItem, ListState, Padding, Paragraph, Row,
        Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState, Wrap,
    },
    Frame,
};
use unicode_width::UnicodeWidthChar;

use std::path::PathBuf;

use std::collections::BTreeMap;

use crate::engine::config::{
    Config, EdgeDef, NumberingStrategy, ReservedFormat, Severity, StoreBackend, Traversal,
};
use crate::engine::document::{AttrValue, DocMeta, Status};
use crate::engine::git_status::GitFileStatus;
#[cfg(feature = "agent")]
use crate::tui::agent::AgentStatus;
use crate::tui::state::{
    anchor_to_flat, App, ConfigDep, DocListNode, EdgeKey, EditableField, FieldEditor, FieldPath,
    FilterField, GraphNode, PreviewTab, RelKey, TypeKey,
};

use super::colors::{status_color, tag_color, StatusPalette};
use super::layout::{calculate_image_height, wrapped_line_count, wrapped_lines_total};

/// Rounded panel border shared by the doc list, graph, and sidebars. `focused`
/// paints the border cyan; otherwise dim gray.
fn panel_block(title: &str, focused: bool) -> Block<'static> {
    let color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .title(format!(" {title} "))
}

/// Column-header style shared by the doc list and graph tables.
fn table_header_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

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

/// The git-status gutter cell for a document row: a colored bar for new/modified
/// docs, a blank space otherwise. Shared by the documents table, the filter
/// table, and the graph table so the left-edge gutter looks identical.
fn git_gutter_cell(app: &App, path: &std::path::Path) -> Cell<'static> {
    match app.git_status_cache.get(path) {
        Some(GitFileStatus::New) => Cell::from("┃").style(Style::default().fg(Color::Green)),
        Some(GitFileStatus::Modified) => Cell::from("┃").style(Style::default().fg(Color::Yellow)),
        None => Cell::from(" "),
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
const ASSIGNEE_COLS: u16 = 16;
const PROV_MIN_COLS: u16 = 20;
const ATTR_COLS: u16 = 16;

/// Desired width for a configured doc-table column and whether it flexes.
/// Built-ins keep today's dimensions (status/tags fixed, provenance a flexible
/// trailing min); any other id is a custom attribute rendered in a slim fixed
/// column. The `bool` is `true` for `Constraint::Min`, `false` for a fixed
/// `Constraint::Length` that collapses to 0 when the row is too narrow.
fn doc_column_width_spec(col: &str) -> (u16, bool) {
    match col {
        "status" => (STATUS_COLS, false),
        "tags" => (TAGS_COLS, false),
        "assignee" => (ASSIGNEE_COLS, false),
        "provenance" => (PROV_MIN_COLS, true),
        _ => (ATTR_COLS, false),
    }
}

/// Table constraints for the doc list: fixed gutter/tree/ID/title leading
/// columns followed by one constraint per configured column. Optional columns
/// collapse to width 0 (right to left) when the row is too narrow, so the
/// default `["status", "tags", "provenance"]` set reproduces the historical
/// responsive layout exactly.
fn doc_table_widths(area_width: u16, columns: &[String]) -> Vec<Constraint> {
    let inner = area_width.saturating_sub(2);
    let spacing = (3 + columns.len()) as u16;
    let essentials = GUTTER_COLS + TREE_COLS + ID_COLS + spacing;
    let mut remaining = inner
        .saturating_sub(essentials)
        .saturating_sub(TITLE_MIN_COLS);

    let mut widths = vec![
        Constraint::Length(GUTTER_COLS),
        Constraint::Length(TREE_COLS),
        Constraint::Length(ID_COLS),
        Constraint::Min(TITLE_MIN_COLS),
    ];
    for col in columns {
        let (desired, is_min) = doc_column_width_spec(col);
        if remaining >= desired {
            remaining -= desired;
            widths.push(if is_min {
                Constraint::Min(desired)
            } else {
                Constraint::Length(desired)
            });
        } else {
            widths.push(Constraint::Length(0));
        }
    }
    widths
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
    /// Resolve wrap-measurement widths from the available table area width for
    /// the configured `columns`. Mirrors `doc_table_widths` ordering: gutter(1),
    /// tree(4), id(18), title(Min 20), then one rect per configured column.
    /// `tags`/`provenance` widths are read from their configured positions (0 if
    /// the column is absent); only these plus the title drive soft-wrap height.
    fn from_area_width(area_width: u16, columns: &[String]) -> Self {
        // Mirror the layout ratatui's Table will compute for these
        // constraints so wrap measurements match the real cell rects.
        // `column_spacing` defaults to 1 between adjacent cells.
        let inner_width = area_width.saturating_sub(2);
        let rects = Layout::default()
            .direction(Direction::Horizontal)
            .spacing(1)
            .constraints(doc_table_widths(area_width, columns))
            .split(Rect::new(0, 0, inner_width, 1));
        let col_width = |id: &str| {
            columns
                .iter()
                .position(|c| c == id)
                .and_then(|i| rects.get(4 + i))
                .map(|r| r.width)
                .unwrap_or(0)
                .max(1)
        };
        DocCellWidths {
            title: rects[3].width.max(1),
            tags: col_width("tags"),
            provenance: col_width("provenance"),
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
/// resolved cell widths. Returns the maximum across the title cell and
/// whichever of the tags/provenance cells are present in `columns` -- this
/// must mirror `doc_row_cells_expanded`'s gating so measured height never
/// diverges from what's actually rendered.
fn row_content_lines(
    title: &str,
    tags: &[String],
    provenance: &[String],
    widths: DocCellWidths,
    columns: &[String],
) -> usize {
    let title_lines = wrap_segments(title, widths.title);
    let mut max_lines = title_lines;

    if columns.iter().any(|c| c == "tags") {
        let tags_lines = tag_wrapped_lines(tags, widths.tags, false).len();
        max_lines = max_lines.max(tags_lines);
    }

    if columns.iter().any(|c| c == "provenance") {
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
        max_lines = max_lines.max(prov_lines);
    }

    max_lines
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

/// Resolve a configured column id to its plain display text, shared by the graph
/// table and the doc table's non-bespoke columns. `status` and `related` are
/// built-ins; any other id is read as a custom attribute rendered via its typed
/// value, or an empty string when the attribute is absent/undeclared on the
/// row's type. Unknown id == custom attribute name matches the graph semantics.
fn column_cell_text(
    status: &Status,
    related: &[String],
    attributes: &BTreeMap<String, AttrValue>,
    col: &str,
) -> String {
    match col {
        "status" => status.to_string(),
        "related" => related.join(", "),
        attr => attributes
            .get(attr)
            .map(attr_value_display)
            .unwrap_or_default(),
    }
}

/// The single-line `status` cell: status text padded to the status column width,
/// coloured by the resolved palette, with a red `[!]` suffix for stale docs.
fn status_column_cell(
    status: &Status,
    dim: bool,
    is_stale: bool,
    type_name: &str,
    colors: &StatusPalette,
) -> Cell<'static> {
    let dim_style = Style::default().fg(Color::DarkGray);
    let status_style = if dim {
        dim_style
    } else {
        Style::default().fg(status_color(colors, type_name, status))
    };
    if is_stale {
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
    }
}

/// The single-line `tags` cell: coloured `[tag]` tokens greedily packed into the
/// tags column width with a dim ` +N` overflow indicator.
fn tags_column_cell(tags: &[String], dim: bool) -> Cell<'static> {
    let dim_style = Style::default().fg(Color::DarkGray);
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
    Cell::new(Line::from(tag_spans))
}

/// One single-line doc-table cell for the configured column `col`. Built-ins
/// carry their bespoke rendering; any other id resolves to its custom-attribute
/// text (shared with the graph view via `column_cell_text`).
#[allow(clippy::too_many_arguments)]
fn doc_column_cell(
    col: &str,
    status: &Status,
    tags: &[String],
    provenance: &[String],
    related: &[String],
    assignee: Option<&str>,
    attributes: &BTreeMap<String, AttrValue>,
    dim: bool,
    is_stale: bool,
    type_name: &str,
    colors: &StatusPalette,
) -> Cell<'static> {
    let dim_style = Style::default().fg(Color::DarkGray);
    let normal_style = Style::default();
    match col {
        "status" => status_column_cell(status, dim, is_stale, type_name, colors),
        "tags" => tags_column_cell(tags, dim),
        "assignee" => {
            let text = assignee
                .map(|a| truncate_with_ellipsis(a, ASSIGNEE_COLS as usize))
                .unwrap_or_default();
            let style = if dim { dim_style } else { normal_style };
            Cell::new(Span::styled(text, style))
        }
        "provenance" => {
            let prov_text = provenance_cell_text(provenance, PROV_MIN_COLS as usize);
            let prov_style = if dim { dim_style } else { normal_style };
            Cell::new(Span::styled(prov_text, prov_style))
        }
        other => {
            let text = column_cell_text(status, related, attributes, other);
            let style = if dim { dim_style } else { normal_style };
            Cell::new(Span::styled(text, style))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn doc_row_cells(
    id: &str,
    title: &str,
    status: &Status,
    tags: &[String],
    provenance: &[String],
    related: &[String],
    assignee: Option<&str>,
    attributes: &BTreeMap<String, AttrValue>,
    is_virtual: bool,
    dim: bool,
    is_gh: bool,
    is_stale: bool,
    type_name: &str,
    colors: &StatusPalette,
    columns: &[String],
) -> Vec<Cell<'static>> {
    let dim_style = Style::default().fg(Color::DarkGray);
    let normal_style = Style::default();

    // IDs render dim gray to match the graph view's ID column.
    let id_style = dim_style;
    let id_cell = if is_gh {
        let badge_style = if dim {
            dim_style
        } else {
            Style::default().fg(Color::Magenta)
        };
        Cell::new(Line::from(vec![
            Span::styled(id.to_string(), id_style),
            Span::styled(" [gh]", badge_style),
        ]))
    } else {
        Cell::new(Span::styled(id.to_string(), id_style))
    };

    let title_text = if is_virtual {
        format!("{} (virtual)", title)
    } else {
        title.to_string()
    };
    let title_style = if dim { dim_style } else { normal_style };
    let title_cell = Cell::new(Span::styled(title_text, title_style));

    let mut cells = vec![id_cell, title_cell];
    for col in columns {
        cells.push(doc_column_cell(
            col, status, tags, provenance, related, assignee, attributes, dim, is_stale, type_name,
            colors,
        ));
    }
    cells
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
    related: &[String],
    assignee: Option<&str>,
    attributes: &BTreeMap<String, AttrValue>,
    is_virtual: bool,
    dim: bool,
    is_gh: bool,
    is_stale: bool,
    widths: DocCellWidths,
    type_name: &str,
    colors: &StatusPalette,
    columns: &[String],
) -> Vec<Cell<'static>> {
    // Reuse the single-line cell builder, then replace title and the (configured)
    // tags/provenance cells with wrapped multi-line versions.
    let mut cells = doc_row_cells(
        id, title, status, tags, provenance, related, assignee, attributes, is_virtual, dim, is_gh,
        is_stale, type_name, colors, columns,
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

    // Configured column cells begin at index 2 (after id and title).
    if !tags.is_empty() {
        if let Some(pos) = columns.iter().position(|c| c == "tags") {
            cells[2 + pos] = Cell::from(tag_wrapped_lines(tags, widths.tags, dim));
        }
    }

    if !provenance.is_empty() {
        if let Some(pos) = columns.iter().position(|c| c == "provenance") {
            let prov_text = provenance.join(", ");
            let prov_lines = wrap_to_lines(&prov_text, widths.provenance, prov_style);
            cells[2 + pos] = Cell::from(prov_lines);
        }
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
    colors: &StatusPalette,
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

    let gutter_cell = git_gutter_cell(app, &node.path);

    let doc = app.store.get(&node.path);
    let tags = doc.map(|doc| doc.tags.clone()).unwrap_or_default();
    let provenance = doc.map(|doc| doc.provenance.clone()).unwrap_or_default();
    let attributes = doc.map(|doc| doc.attributes.clone()).unwrap_or_default();
    let assignee = doc.and_then(|doc| doc.assignee.clone());
    let related: Vec<String> = doc
        .map(|doc| doc.related.iter().map(|r| r.target.clone()).collect())
        .unwrap_or_default();

    let display_id = if node.has_duplicate_id {
        format!("! {}", node.id)
    } else {
        node.id.clone()
    };

    let (is_gh, is_stale) = check_doc_stale(&node.path, node.doc_type.as_str(), config);

    let columns = &config.ui.table.columns;
    let widths = DocCellWidths::from_area_width(area_width, columns);
    let content_lines = row_content_lines(&node.title, &tags, &provenance, widths, columns);
    let expanded = app.wrap_mode && index == app.selected_doc;

    let mut cells = vec![gutter_cell, tree_cell];
    if expanded {
        cells.extend(doc_row_cells_expanded(
            &display_id,
            &node.title,
            &node.status,
            &tags,
            &provenance,
            &related,
            assignee.as_deref(),
            &attributes,
            node.is_virtual,
            dim,
            is_gh,
            is_stale,
            widths,
            node.doc_type.as_str(),
            colors,
            columns,
        ));
    } else {
        cells.extend(doc_row_cells(
            &display_id,
            &node.title,
            &node.status,
            &tags,
            &provenance,
            &related,
            assignee.as_deref(),
            &attributes,
            node.is_virtual,
            dim,
            is_gh,
            is_stale,
            node.doc_type.as_str(),
            colors,
            columns,
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
        .block(panel_block("Types", false))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default().with_selected(Some(app.selected_type));
    f.render_stateful_widget(list, area, &mut state);
}

/// Graph-view pivot picker. Reuses the types-view sidebar grammar: a list of
/// document types plus a leading "All" row for the whole-store forest
/// (`graph_anchor == None`). Highlights the current `graph_anchor`. The TUI only
/// selects the anchor here; re-rooting lives in `resolve_forest` (engine).
pub fn draw_graph_pivot_panel(f: &mut Frame, app: &App, area: Rect) {
    let mut items: Vec<ListItem> =
        Vec::with_capacity(app.doc_types.len() + app.available_tags.len() + 1);
    items.push(ListItem::new("  All".to_string()));
    for dt in &app.doc_types {
        let plural = app
            .type_plurals
            .get(&dt.to_string())
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        items.push(ListItem::new(format!("  {}", plural)));
    }
    for tag in &app.available_tags {
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("[{}]", tag), Style::default().fg(tag_color(tag))),
        ])));
    }

    let list = List::new(items)
        .block(panel_block("Pivot", false))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    // Sidebar order is All, types…, tags… (see anchor_to_flat).
    let selected = anchor_to_flat(app.graph_anchor, app.doc_types.len());
    let mut state = ListState::default().with_selected(Some(selected));
    f.render_stateful_widget(list, area, &mut state);
}

pub fn draw_doc_list(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    config: &Config,
    colors: &StatusPalette,
) {
    // Reserve 2 rows for the border and 1 for the header (matches draw_graph).
    app.doc_list_height = area.height.saturating_sub(3) as usize;
    let relations_focused = app.preview_tab == PreviewTab::Relations;
    let dim = relations_focused;

    let area_width = area.width;
    let rows: Vec<Row> = app
        .doc_tree
        .iter()
        .enumerate()
        .map(|(i, node)| doc_row_for_node(app, node, i, dim, config, area_width, colors))
        .collect();

    let columns = &config.ui.table.columns;
    let widths = doc_table_widths(area.width, columns);

    let highlight_style = if relations_focused {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::REVERSED)
    };

    // Header mirrors the graph view: blank gutter + tree columns, the fixed
    // ID/DOC labels, then one uppercased label per configured column.
    let hs = table_header_style();
    let mut header_cells = vec![
        Cell::from(""),
        Cell::from(""),
        Cell::from("ID").style(hs),
        Cell::from("DOC").style(hs),
    ];
    for col in columns {
        header_cells.push(Cell::from(col.to_uppercase()).style(hs));
    }
    let header = Row::new(header_cells);

    let table = Table::new(rows, widths)
        .header(header)
        .block(panel_block("Documents", !relations_focused))
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

pub fn draw_preview(f: &mut Frame, app: &mut App, area: Rect, colors: &StatusPalette) {
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
        PreviewTab::Preview => render_document_preview(f, app, area, block, doc.as_ref(), colors),
        PreviewTab::Relations => {
            render_relationship_sections(f, app, area, block, doc.as_ref(), colors)
        }
    }
}

pub(super) fn build_preview_header_lines(
    doc: &DocMeta,
    expanding: bool,
    colors: &StatusPalette,
) -> Vec<Line<'static>> {
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
                Style::default().fg(status_color(colors, doc.doc_type.as_str(), &doc.status)),
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

    if let Some(assignee) = &doc.assignee {
        lines.push(Line::from(vec![
            Span::raw(" Assignee: "),
            Span::raw(assignee.clone()),
        ]));
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

    for (name, value) in &doc.attributes {
        lines.push(Line::from(vec![
            Span::raw(format!(" {}: ", name)),
            Span::raw(attr_value_display(value)),
        ]));
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
    colors: &StatusPalette,
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
    let header_lines = build_preview_header_lines(doc, expanding, colors);
    let mut lines = header_lines.clone();

    let body_hash = crate::engine::cache::DiskCache::body_hash(&body);
    let diagram_blocks = match &app.diagram_blocks_cache {
        Some((p, h, b)) if p == &doc.path && *h == body_hash => b.clone(),
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
    colors: &StatusPalette,
) {
    let Some(doc) = doc else {
        let paragraph = Paragraph::new(" No document selected.")
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
        return;
    };

    let sections = app.relation_sections(doc);

    if sections.chain.is_empty() && sections.children.is_empty() && sections.related.is_empty() {
        let paragraph = Paragraph::new(" No relations.")
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
        return;
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
                    status_color(colors, target_doc.doc_type.as_str(), &target_doc.status),
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

    let labelled_sections: Vec<(&str, &Vec<std::path::PathBuf>)> = vec![
        ("chain", &sections.chain),
        ("children", &sections.children),
        ("related", &sections.related),
    ];

    for (label, paths) in &labelled_sections {
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

pub fn render_fullscreen_document(f: &mut Frame, app: &mut App, colors: &StatusPalette) {
    let area = f.area();
    app.fullscreen_height = area.height.saturating_sub(2) as usize;

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    let Some(doc) = app.selected_doc_meta() else {
        return;
    };

    let mut header_spans = vec![
        Span::styled(
            format!(" {} ", doc.title),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled(
            format!("{}", doc.status),
            Style::default().fg(status_color(colors, doc.doc_type.as_str(), &doc.status)),
        ),
        Span::raw(format!(" | {} | {} ", doc.doc_type, doc.author)),
    ];
    if let Some(assignee) = &doc.assignee {
        header_spans.push(Span::raw(format!("| @{} ", assignee)));
    }
    header_spans.push(Span::styled(
        "[Esc] back",
        Style::default().fg(Color::DarkGray),
    ));
    let header = Line::from(header_spans);
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

    let display_body_hash = crate::engine::cache::DiskCache::body_hash(&display_body);
    let fullscreen_blocks = match &app.diagram_blocks_cache {
        Some((p, h, b)) if p == &doc.path && *h == display_body_hash => b.clone(),
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

pub fn render_filter_panel(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    config: &Config,
    colors: &StatusPalette,
) {
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
            let gutter_cell = git_gutter_cell(app, &doc.path);
            let tree_cell = Cell::new("");
            let mut cells = vec![gutter_cell, tree_cell];
            let (is_gh, is_stale) = check_doc_stale(&doc.path, doc.doc_type.as_str(), config);
            let related: Vec<String> = doc.related.iter().map(|r| r.target.clone()).collect();
            cells.extend(doc_row_cells(
                &doc.id,
                &doc.title,
                &doc.status,
                &doc.tags,
                &doc.provenance,
                &related,
                doc.assignee.as_deref(),
                &doc.attributes,
                doc.virtual_doc,
                dim,
                is_gh,
                is_stale,
                doc.doc_type.as_str(),
                colors,
                &config.ui.table.columns,
            ));
            let style = if dim {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            Row::new(cells).style(style)
        })
        .collect();

    let widths = doc_table_widths(right[0].width, &config.ui.table.columns);

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
        PreviewTab::Preview => {
            render_document_preview(f, app, right[1], block, doc.as_ref(), colors)
        }
        PreviewTab::Relations => {
            render_relationship_sections(f, app, right[1], block, doc.as_ref(), colors)
        }
    }
}

#[cfg(feature = "agent")]
pub fn draw_agents_screen(f: &mut Frame, app: &App, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let main_area = layout[0];
    let footer_area = layout[1];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Agents ");

    if app.agent_spawner.records.is_empty() {
        let paragraph = Paragraph::new(
            "No agents have been invoked yet. Press `a` on a document to start one.",
        )
        .style(Style::default().fg(Color::DarkGray))
        .alignment(ratatui::layout::Alignment::Center)
        .block(block);
        f.render_widget(paragraph, main_area);
    } else {
        let rows: Vec<Row> = app
            .agent_spawner
            .records
            .iter()
            .map(|record| {
                let (icon, color) = match record.status {
                    AgentStatus::Running => ("●", Color::Yellow),
                    AgentStatus::Complete => ("✔", Color::Green),
                    AgentStatus::Failed => ("✘", Color::Red),
                };
                Row::new(vec![
                    Cell::from(Span::styled(
                        format!("  {}", icon),
                        Style::default().fg(color),
                    )),
                    Cell::from(Span::raw(format!(
                        "{:<14}",
                        record
                            .session_id
                            .split('-')
                            .next()
                            .unwrap_or(&record.session_id)
                    ))),
                    Cell::from(Span::raw(&*record.doc_title)),
                    Cell::from(Span::raw(&*record.action)),
                    Cell::from(Span::styled(
                        &*record.started_at,
                        Style::default().fg(Color::DarkGray),
                    )),
                ])
            })
            .collect();

        let widths = [
            Constraint::Length(4),
            Constraint::Length(14),
            Constraint::Fill(1),
            Constraint::Length(18),
            Constraint::Min(20),
        ];

        let table = Table::new(rows, widths)
            .block(block)
            .header(
                Row::new(vec!["  ", "Session", "Document", "Action", "Started"]).style(
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                ),
            )
            .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        let mut state = TableState::default().with_selected(Some(app.agent_selected_index));
        f.render_stateful_widget(table, main_area, &mut state);
    }

    let footer = Line::from(vec![
        Span::styled("e", Style::default().fg(Color::Cyan)),
        Span::raw(": open document  "),
        Span::styled("r", Style::default().fg(Color::Cyan)),
        Span::raw(": resume session  "),
        Span::styled("`", Style::default().fg(Color::Cyan)),
        Span::raw(": switch view"),
    ]);
    f.render_widget(
        Paragraph::new(footer).style(Style::default().fg(Color::DarkGray)),
        footer_area,
    );
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

/// Build the styled spans for one row of the dependency graph.
///
/// `type_icon`, `is_last`, and `id` are passed in so the helper has no
/// dependency on `App` (DICTUM-003). `id` is the already-uppercased doc id,
/// used only by the back-reference branch.
/// The legacy single-line graph row: the DOC-cell tree art plus the inline
/// status and related annotations. Superseded by the nested table in
/// `draw_graph` (ITERATION-209), but retained as the unit-test fixture for the
/// tree-connector / annotation rendering contracts.
#[cfg(test)]
pub(super) fn graph_node_spans(
    node: &GraphNode,
    type_icon: &str,
    is_last: bool,
    stems: &[bool],
) -> Vec<Span<'static>> {
    let mut spans = graph_doc_cell_spans(node, type_icon, is_last, stems);

    // The legacy single-line form (still used by the unit tests) appends the
    // status and related annotations after the tree art. In the nested-table
    // render these are their own columns; the doc cell carries only tree+title.
    spans.push(Span::styled(
        format!(" {}", node.status),
        Style::default().fg(status_color(
            &StatusPalette::default(),
            node.doc_type.as_str(),
            &node.status,
        )),
    ));

    for related_id in &node.related {
        spans.push(Span::styled(
            format!(" \u{2504}\u{25B7} {}", related_id),
            Style::default().fg(Color::DarkGray),
        ));
    }

    spans
}

/// Whether node `i` is the last child at its own depth, i.e. no later node
/// shares its depth before the tree steps back to a shallower level. Mirrors
/// the forward-scan in [`compute_stems`] so the `└`/`├` connector agrees with
/// the `│` spine drawn under it: a node with children is detected as last when
/// its next sibling at the same depth is absent, not merely when the next row
/// is shallower.
fn is_last_child(nodes: &[GraphNode], i: usize) -> bool {
    let d = nodes[i].depth;
    let mut j = i + 1;
    while j < nodes.len() && nodes[j].depth > d {
        j += 1;
    }
    !(j < nodes.len() && nodes[j].depth == d)
}

/// Precompute vertical-stem visibility for each node's ancestor levels.
/// `stems[i][k]` is true when the ancestor at tree depth `k+1` still has more
/// children after node `i`'s subtree — rendering a `│` at that level instead
/// of blank space.
fn compute_stems(nodes: &[GraphNode]) -> Vec<Vec<bool>> {
    let mut all_stems = Vec::with_capacity(nodes.len());

    for (i, node) in nodes.iter().enumerate() {
        let d = node.depth;
        let mut stems = Vec::with_capacity(d.saturating_sub(1));

        for k in 1..d {
            let mut j = i + 1;
            while j < nodes.len() && nodes[j].depth > k {
                j += 1;
            }
            stems.push(j < nodes.len() && nodes[j].depth == k);
        }

        all_stems.push(stems);
    }

    all_stems
}

/// The DOC-column spans for one graph row: the tree indent/connectors (the
/// tree art preserved from the original render), then the type icon + title.
/// Status and related/attribute columns are rendered as separate table cells
/// (ITERATION-209), so they are NOT included here.
///
/// A `reverse` row is a chain ANCESTOR of the row it hangs under, emitted by an
/// anchored forest (STORY-247), so its edge points back up the tree: the
/// arrowhead flips and the connector brightens. `↑` rather than `▲` because `▲`
/// is the default `story` type icon — the dominant ancestor type of the
/// iteration pivot, which would render `▲ ▲ Title` — and the header's sort-
/// direction arrow; `↑` reads as direction alone in both spots.
pub(super) fn graph_doc_cell_spans(
    node: &GraphNode,
    type_icon: &str,
    is_last: bool,
    stems: &[bool],
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    if node.depth > 0 {
        let mut stem_str = String::new();
        for &open in stems {
            if open {
                stem_str.push_str("│  ");
            } else {
                stem_str.push_str("   ");
            }
        }
        if !stem_str.is_empty() {
            spans.push(Span::styled(stem_str, Style::default().fg(Color::DarkGray)));
        }
        let (connector, style) = if node.reverse {
            (
                if is_last { "└─↑ " } else { "├─↑ " },
                Style::default()
                    .fg(Color::Gray)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            (
                if is_last { "└─▶ " } else { "├─▶ " },
                Style::default().fg(Color::DarkGray),
            )
        };
        spans.push(Span::styled(connector, style));
    }

    spans.push(Span::styled(
        format!("{} ", type_icon),
        Style::default().fg(Color::Gray),
    ));

    spans.push(Span::styled(
        node.title.clone(),
        Style::default().fg(Color::White),
    ));

    spans
}

/// Render an [`AttrValue`] as a compact cell string.
fn attr_value_display(v: &AttrValue) -> String {
    match v {
        AttrValue::Int(i) => i.to_string(),
        AttrValue::Float(f) => f.to_string(),
        AttrValue::Str(s) => s.clone(),
        AttrValue::Bool(b) => b.to_string(),
        AttrValue::Date(d) => d.format("%Y-%m-%d").to_string(),
        AttrValue::Raw(raw) => raw.as_str().map(str::to_string).unwrap_or_default(),
    }
}

/// Graph table ID column width. Slim: holds a doc id like `ITERATION-209`,
/// truncated past that.
const GRAPH_ID_COLS: u16 = 16;

pub fn draw_graph(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    config: &Config,
    colors: &StatusPalette,
) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(area);

    draw_graph_pivot_panel(f, app, layout[0]);

    let table_area = layout[1];
    // Render-feeds-state: the table reserves 2 rows for the border and 1 for the
    // header, so the visible data height is height - 3.
    app.graph_list_height = table_area.height.saturating_sub(3) as usize;

    let columns = &config.ui.graph.columns;

    // Header: a blank gutter, the slim ID column, the DOC column, then each
    // configured column. The active sort column carries a direction arrow; `path`
    // marks the DOC column.
    let arrow = if app.graph_sort_rev { " ▼" } else { " ▲" };
    let header_style = table_header_style();
    let header_cell = |label: &str, sort_id: &str| {
        let marked = app.graph_sort_col == sort_id;
        let text = if marked {
            format!("{label}{arrow}")
        } else {
            label.to_string()
        };
        Cell::from(text).style(header_style)
    };

    let mut header_cells = vec![
        Cell::from(""),
        Cell::from("ID").style(header_style),
        header_cell("DOC", "path"),
    ];
    for col in columns {
        let label = col.to_uppercase();
        header_cells.push(header_cell(&label, col));
    }
    let header = Row::new(header_cells);

    let stems = compute_stems(&app.graph_nodes);

    let rows: Vec<Row> = app
        .graph_nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let is_last = is_last_child(&app.graph_nodes, i);
            let type_icon = app
                .type_icons
                .get(&node.doc_type.to_string())
                .map(|s| s.as_str())
                .unwrap_or("○");
            let id = app
                .store
                .get(&node.path)
                .map(|d| d.id.to_uppercase())
                .unwrap_or_default();

            let gutter_cell = git_gutter_cell(app, &node.path);
            let id_cell = Cell::new(Span::styled(
                truncate_with_ellipsis(&id, GRAPH_ID_COLS as usize),
                Style::default().fg(Color::DarkGray),
            ));
            let doc_cell = Cell::from(Line::from(graph_doc_cell_spans(
                node, type_icon, is_last, &stems[i],
            )));

            let mut cells = vec![gutter_cell, id_cell, doc_cell];
            for col in columns {
                let text = column_cell_text(&node.status, &node.related, &node.attributes, col);
                let style = if col == "status" {
                    Style::default().fg(status_color(colors, node.doc_type.as_str(), &node.status))
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                cells.push(Cell::new(Span::styled(text, style)));
            }
            Row::new(cells)
        })
        .collect();

    // Gutter + slim ID, then DOC takes ~half the remaining width; the configured
    // columns split the rest evenly.
    let mut widths = vec![
        Constraint::Length(GUTTER_COLS),
        Constraint::Length(GRAPH_ID_COLS),
        Constraint::Percentage(50),
    ];
    if !columns.is_empty() {
        let each = (50 / columns.len() as u16).max(1);
        for _ in columns {
            widths.push(Constraint::Percentage(each));
        }
    }

    let table = Table::new(rows, widths)
        .header(header)
        .block(panel_block("Dependency Graph", true))
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = TableState::default()
        .with_selected(Some(app.graph_selected))
        .with_offset(app.graph_offset);
    f.render_stateful_widget(table, table_area, &mut state);

    let total = app.graph_nodes.len();
    if total > app.graph_list_height {
        render_scrollbar(
            f,
            table_area,
            total,
            app.graph_list_height,
            app.graph_selected,
        );
    }
}

fn reserved_format_str(f: &ReservedFormat) -> &'static str {
    match f {
        ReservedFormat::Incremental => "incremental",
        ReservedFormat::Sqids => "sqids",
    }
}

fn numbering_str(n: &NumberingStrategy) -> &'static str {
    match n {
        NumberingStrategy::Incremental => "incremental",
        NumberingStrategy::Sqids => "sqids",
        NumberingStrategy::Reserved => "reserved",
    }
}

const NUMBERING_VARIANTS: &[&str] = &["incremental", "sqids", "reserved"];
const STORE_VARIANTS: &[&str] = &[
    "filesystem",
    "github-issues",
    "github-milestones",
    "github-projects",
    "git-ref",
];
const RESERVED_FORMAT_VARIANTS: &[&str] = &["incremental", "sqids"];

/// How the panel spells a value that is not set. Shared by the render and by
/// the optional enum cyclers' first entry, so the string a row shows and the
/// variant the cycler matches against are one string.
pub const UNSET_VARIANT: &str = "(unset)";

/// `required` and `traversal` lead with the unset entry. Absence is a claim on
/// an edge row -- no requiredness, no traversal role (RFC-067) -- so the cycler
/// has to reach it, and `EnumCycle` indexes a flat variant list with no `Option`
/// spelling of its own.
const EDGE_REQUIRED_VARIANTS: &[&str] = &[UNSET_VARIANT, "error", "warning"];
const EDGE_TRAVERSAL_VARIANTS: &[&str] = &[UNSET_VARIANT, "chain", "related"];

fn field(label: &str, value: String, editor: FieldEditor, path: FieldPath) -> EditableField {
    EditableField {
        label: label.to_string(),
        value,
        editor,
        path,
    }
}

fn nullable_value(opt: Option<&str>) -> String {
    opt.unwrap_or(UNSET_VARIANT).to_string()
}

fn statusbar_value(slot: Option<&Vec<String>>) -> String {
    slot.map_or_else(
        || "(unset)".to_string(),
        |v| {
            if v.is_empty() {
                "(unset)".to_string()
            } else {
                v.join(", ")
            }
        },
    )
}

/// The FIELD list for the current settings field-view, mirroring the display
/// rendered by `settings_lines_inner` exactly (same fields, order, values) with
/// the editor kind and buffer path per field. Entry-LIST views (a collection
/// category that is not drilled) carry no fields and return empty; the cat-6
/// override entries below `normalize` are likewise an entry-list, not fields, so
/// the not-drilled cat-6 view returns only the top-level `normalize` field.
pub fn settings_fields(
    category: usize,
    _entry: usize,
    drill: Option<usize>,
    config: &Config,
) -> Vec<EditableField> {
    let mut fields = Vec::new();
    match category {
        0 => {
            fields.push(field(
                "naming.pattern",
                config.documents.naming.pattern.clone(),
                FieldEditor::Text,
                FieldPath::Naming,
            ));
            fields.push(field(
                "ref_count_ceiling",
                config.ref_count_ceiling.to_string(),
                FieldEditor::BoundedNum { min: 1, max: 1000 },
                FieldPath::RefCountCeiling,
            ));
            fields.push(field(
                "templates.dir",
                config.filesystem.templates.dir.clone(),
                FieldEditor::Text,
                FieldPath::TemplatesDir,
            ));
        }
        1 => {
            if let Some(d) = drill {
                if let Some(t) = config.documents.types.get(d) {
                    let key = |k| FieldPath::Type { index: d, key: k };
                    fields.push(field(
                        "name",
                        t.name.clone(),
                        FieldEditor::Text,
                        key(TypeKey::Name),
                    ));
                    fields.push(field(
                        "plural",
                        t.plural.clone(),
                        FieldEditor::Text,
                        key(TypeKey::Plural),
                    ));
                    fields.push(field(
                        "dir",
                        t.dir.clone(),
                        FieldEditor::Text,
                        key(TypeKey::Dir),
                    ));
                    fields.push(field(
                        "prefix",
                        t.prefix.clone(),
                        FieldEditor::Text,
                        key(TypeKey::Prefix),
                    ));
                    fields.push(field(
                        "icon",
                        nullable_value(t.icon.as_deref()),
                        FieldEditor::Nullable,
                        key(TypeKey::Icon),
                    ));
                    fields.push(field(
                        "numbering",
                        numbering_str(&t.numbering).to_string(),
                        FieldEditor::EnumCycle {
                            variants: NUMBERING_VARIANTS,
                        },
                        key(TypeKey::Numbering),
                    ));
                    fields.push(field(
                        "subdirectory",
                        t.subdirectory.to_string(),
                        FieldEditor::Toggle,
                        key(TypeKey::Subdirectory),
                    ));
                    fields.push(field(
                        "store",
                        t.store.to_string(),
                        FieldEditor::EnumCycle {
                            variants: STORE_VARIANTS,
                        },
                        key(TypeKey::Store),
                    ));
                    fields.push(field(
                        "singleton",
                        t.singleton.to_string(),
                        FieldEditor::Toggle,
                        key(TypeKey::Singleton),
                    ));
                    fields.push(field(
                        "parent_type",
                        nullable_value(t.parent_type.as_deref()),
                        FieldEditor::Nullable,
                        key(TypeKey::ParentType),
                    ));
                    let agents = if t.agents.is_empty() {
                        "(unset)".to_string()
                    } else {
                        t.agents.join(", ")
                    };
                    fields.push(field(
                        "agents",
                        agents,
                        FieldEditor::List,
                        key(TypeKey::Agents),
                    ));
                }
            }
        }
        2 => {
            if let Some(d) = drill {
                if let Some(r) = config.relationships.get(d) {
                    fields.push(field(
                        "name",
                        r.name.clone(),
                        FieldEditor::Text,
                        FieldPath::Rel {
                            index: d,
                            key: RelKey::Name,
                        },
                    ));
                    fields.push(field(
                        "inverse",
                        nullable_value(r.inverse.as_deref()),
                        FieldEditor::Nullable,
                        FieldPath::Rel {
                            index: d,
                            key: RelKey::Inverse,
                        },
                    ));
                }
            }
        }
        3 => {
            if let Some(d) = drill {
                if let Some(e) = config.edges.get(d) {
                    let key = |k| FieldPath::Edge { index: d, key: k };
                    fields.push(field(
                        "name",
                        e.name.clone(),
                        FieldEditor::Text,
                        key(EdgeKey::Name),
                    ));
                    // The two type positions take the member-at-a-time picker
                    // (AC3), whose vocabulary is `*` plus the declared type
                    // names. `via` keeps the comma editor: its vocabulary is the
                    // relationships, and see `rel_selector_from` for why it is
                    // not a cycler.
                    fields.push(field(
                        "from",
                        e.from.spelling(),
                        FieldEditor::TypeSet,
                        key(EdgeKey::From),
                    ));
                    fields.push(field(
                        "to",
                        e.to.spelling(),
                        FieldEditor::TypeSet,
                        key(EdgeKey::To),
                    ));
                    fields.push(field(
                        "via",
                        e.via.spelling(),
                        FieldEditor::List,
                        key(EdgeKey::Via),
                    ));
                    fields.push(field(
                        "required",
                        nullable_value(e.required.as_ref().map(Severity::as_str)),
                        FieldEditor::EnumCycle {
                            variants: EDGE_REQUIRED_VARIANTS,
                        },
                        key(EdgeKey::Required),
                    ));
                    fields.push(field(
                        "traversal",
                        nullable_value(e.traversal.as_ref().map(Traversal::as_str)),
                        FieldEditor::EnumCycle {
                            variants: EDGE_TRAVERSAL_VARIANTS,
                        },
                        key(EdgeKey::Traversal),
                    ));
                }
            }
        }
        4 => {
            match &config.documents.sqids {
                Some(s) => {
                    fields.push(field(
                        "sqids.salt",
                        s.salt.clone(),
                        FieldEditor::Text,
                        FieldPath::SqidsSalt,
                    ));
                    fields.push(field(
                        "sqids.min_length",
                        s.min_length.to_string(),
                        FieldEditor::BoundedNum { min: 1, max: 10 },
                        FieldPath::SqidsMinLength,
                    ));
                }
                None => {
                    fields.push(field(
                        "sqids.salt",
                        "(unset)".to_string(),
                        FieldEditor::ReadOnly,
                        FieldPath::Unset,
                    ));
                    fields.push(field(
                        "sqids.min_length",
                        "(unset)".to_string(),
                        FieldEditor::ReadOnly,
                        FieldPath::Unset,
                    ));
                }
            }
            match &config.documents.reserved {
                Some(r) => {
                    fields.push(field(
                        "reserved.remote",
                        r.remote.clone(),
                        FieldEditor::Text,
                        FieldPath::ReservedRemote,
                    ));
                    fields.push(field(
                        "reserved.format",
                        reserved_format_str(&r.format).to_string(),
                        FieldEditor::EnumCycle {
                            variants: RESERVED_FORMAT_VARIANTS,
                        },
                        FieldPath::ReservedFormat,
                    ));
                    fields.push(field(
                        "reserved.max_retries",
                        r.max_retries.to_string(),
                        FieldEditor::BoundedNum { min: 0, max: 1000 },
                        FieldPath::ReservedMaxRetries,
                    ));
                }
                None => {
                    fields.push(field(
                        "reserved.remote",
                        "(unset)".to_string(),
                        FieldEditor::ReadOnly,
                        FieldPath::Unset,
                    ));
                    fields.push(field(
                        "reserved.format",
                        "(unset)".to_string(),
                        FieldEditor::ReadOnly,
                        FieldPath::Unset,
                    ));
                    fields.push(field(
                        "reserved.max_retries",
                        "(unset)".to_string(),
                        FieldEditor::ReadOnly,
                        FieldPath::Unset,
                    ));
                }
            }
        }
        5 => match &config.documents.github {
            Some(g) => {
                fields.push(field(
                    "repo",
                    nullable_value(g.repo.as_deref()),
                    FieldEditor::Nullable,
                    FieldPath::GithubRepo,
                ));
                fields.push(field(
                    "cache_ttl",
                    g.cache_ttl.to_string(),
                    FieldEditor::BoundedNum {
                        min: 0,
                        max: u64::MAX,
                    },
                    FieldPath::GithubCacheTtl,
                ));
            }
            None => {
                fields.push(field(
                    "repo",
                    "(unset)".to_string(),
                    FieldEditor::ReadOnly,
                    FieldPath::Unset,
                ));
                fields.push(field(
                    "cache_ttl",
                    "(unset)".to_string(),
                    FieldEditor::ReadOnly,
                    FieldPath::Unset,
                ));
            }
        },
        6 => {
            if let Some(d) = drill {
                let mut keys: Vec<&String> = config.certification.overrides.keys().collect();
                keys.sort();
                if let Some(key) = keys.get(d) {
                    if let Some(ov) = config.certification.overrides.get(*key) {
                        fields.push(field(
                            "normalize",
                            ov.normalize.to_string(),
                            FieldEditor::Toggle,
                            FieldPath::CertOverride {
                                key: (*key).clone(),
                            },
                        ));
                    }
                }
            } else {
                fields.push(field(
                    "normalize",
                    config.certification.normalize.to_string(),
                    FieldEditor::Toggle,
                    FieldPath::CertNormalize,
                ));
            }
        }
        7 => {
            fields.push(field(
                "interactive",
                nullable_value(config.agents.interactive.as_deref()),
                FieldEditor::Nullable,
                FieldPath::AgentsInteractive,
            ));
        }
        8 => {
            fields.push(field(
                "ascii_diagrams",
                config.ui.ascii_diagrams.to_string(),
                FieldEditor::Toggle,
                FieldPath::UiAsciiDiagrams,
            ));
            fields.push(field(
                "statusbar.enabled",
                config.ui.statusbar.enabled.to_string(),
                FieldEditor::Toggle,
                FieldPath::StatusbarEnabled,
            ));
            fields.push(field(
                "statusbar.left",
                statusbar_value(config.ui.statusbar.left.as_ref()),
                FieldEditor::ZoneOrdering,
                FieldPath::StatusbarLeft,
            ));
            fields.push(field(
                "statusbar.center",
                statusbar_value(config.ui.statusbar.center.as_ref()),
                FieldEditor::ZoneOrdering,
                FieldPath::StatusbarCenter,
            ));
            fields.push(field(
                "statusbar.right",
                statusbar_value(config.ui.statusbar.right.as_ref()),
                FieldEditor::ZoneOrdering,
                FieldPath::StatusbarRight,
            ));
            fields.push(field(
                "multiline.max_expanded_height",
                config.ui.multiline.max_expanded_height.to_string(),
                FieldEditor::BoundedNum { min: 1, max: 1000 },
                FieldPath::MultilineMaxExpandedHeight,
            ));
        }
        _ => {}
    }
    fields
}

/// Render one field-view field as a display line, the single source of truth
/// shared with `settings_fields`.
fn field_line(f: &EditableField) -> String {
    format!("{}: {}", f.label, f.value)
}

/// True when the sqids salt is required but empty: the `[numbering.sqids]` section
/// exists (so a type uses sqids numbering) but its salt is blank. Drives the AC2
/// required-but-empty flag in the Numbering view. Reads buffer state only, so the
/// flag persists after a scaffold offer is dismissed and clears once the salt is
/// filled. A non-empty salt is never flagged.
pub(super) fn sqids_salt_required_empty(config: &Config) -> bool {
    config
        .documents
        .sqids
        .as_ref()
        .is_some_and(|s| s.salt.is_empty())
}

/// The `[section]` label shown in a scaffold offer prompt.
fn scaffold_section_label(dep: ConfigDep) -> &'static str {
    match dep {
        ConfigDep::NumberingSqids => "numbering.sqids",
        ConfigDep::NumberingReserved => "numbering.reserved",
        ConfigDep::Github => "github",
    }
}

/// The value to show for a field's value column. While the field is the focused
/// row AND an edit is in progress, echo the live `edit_input` with the house
/// caret (`_`) appended so the user sees what they are typing; otherwise show
/// the buffer-derived `field.value`.
fn settings_display_value(
    field: &EditableField,
    is_focused_editing: bool,
    edit_input: &str,
) -> String {
    if is_focused_editing {
        format!("{}_", edit_input)
    } else {
        field.value.clone()
    }
}

/// One `[[edges]]` row as its entry-list line: the row's name, then the triple
/// it selects, read `from -via-> to`. Every position is spelled by the engine
/// (`TypeSelector::spelling`), so the list shows the TOML the user wrote.
fn edge_entry_line(edge: &EdgeDef) -> String {
    format!(
        "{}: {} -{}-> {}",
        edge.name,
        edge.from.spelling(),
        edge.via.spelling(),
        edge.to.spelling()
    )
}

/// The navigable entry names for an entry-list collection (categories 1/2/3 and
/// cat 6's certification overrides), read straight from the config model. The
/// single source of truth for entry-list content: the render, the drilled-view
/// title (`drill_entry_name`), and the legacy display-line builder all derive
/// from it, so nothing reconstructs entry text from a rendered string.
///
/// An edge's "name" here is a display line rather than its identity: STORY-260
/// AC1 wants the whole triple readable in the list, which is one screen, not
/// two. `drill_entry_name` keeps the identity half.
pub(super) fn settings_entry_names(category: usize, config: &Config) -> Vec<String> {
    match category {
        1 => config
            .documents
            .types
            .iter()
            .map(|t| t.name.clone())
            .collect(),
        2 => config
            .relationships
            .iter()
            .map(|r| r.name.clone())
            .collect(),
        3 => config.edges.iter().map(edge_entry_line).collect(),
        6 => {
            let mut keys: Vec<&String> = config.certification.overrides.keys().collect();
            keys.sort();
            keys.into_iter().cloned().collect()
        }
        _ => Vec::new(),
    }
}

/// Compose the full settings display lines for one view. The render no longer
/// uses this (it derives field-views from `settings_fields` two-column and
/// entry-lists from `settings_entry_names`); it survives as a test reference that
/// pins field order, values, and entry-list content against those same accessors.
#[cfg(test)]
fn settings_lines_inner(
    category: usize,
    entry: usize,
    drill: Option<usize>,
    config: &Config,
) -> Vec<String> {
    const COLLECTIONS: [usize; 3] = [1, 2, 3];
    // Entry-list categories (1,2,3) that are NOT drilled render a navigable name
    // list, not a field list. cat 6 is a hybrid: the top `normalize` field plus an
    // override entry-list below it. Everything else (including drilled collections)
    // is a pure field-view derived from `settings_fields`.
    if COLLECTIONS.contains(&category) && drill.is_none() {
        return entry_list_rows(category, config)
            .into_iter()
            .enumerate()
            .map(|(i, name)| {
                let pfx = if i == entry { "▸ " } else { "  " };
                format!("{}{}", pfx, name)
            })
            .collect();
    }

    if category == 6 {
        let mut lines: Vec<String> = settings_fields(category, entry, drill, config)
            .iter()
            .map(field_line)
            .collect();
        if drill.is_none() {
            for (i, name) in entry_list_rows(category, config).iter().enumerate() {
                let pfx = if i == entry { "▸ " } else { "  " };
                lines.push(format!("{}{}", pfx, name));
            }
        }
        return lines;
    }

    settings_fields(category, entry, drill, config)
        .iter()
        .map(field_line)
        .collect()
}

/// The drilled-view breadcrumb for one entry. Every collection but Edges titles
/// by the same string it lists, so this is `settings_entry_names` indexed. An
/// edge lists as its whole triple and is titled by its `name` alone: a
/// breadcrumb names the row that was drilled into.
fn drill_entry_name(cat: usize, idx: usize, config: &Config) -> String {
    if cat == 3 {
        return config
            .edges
            .get(idx)
            .map(|e| e.name.clone())
            .unwrap_or_default();
    }
    settings_entry_names(cat, config)
        .get(idx)
        .cloned()
        .unwrap_or_default()
}

/// The rows an undrilled collection category lists: its entry names, or -- when
/// it declares none, which every one of these categories permits -- one row
/// naming what is missing and the key that adds it, since a blank pane names
/// neither.
fn entry_list_rows(category: usize, config: &Config) -> Vec<String> {
    let names = settings_entry_names(category, config);
    if !names.is_empty() {
        return names;
    }
    let missing = match category {
        1 => "document types",
        2 => "relationships",
        3 => "edges",
        _ => "certification overrides",
    };
    vec![format!("(no {missing} configured -- press n to add one)")]
}

pub fn draw_settings(f: &mut Frame, app: &App, area: Rect, config: &Config) {
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(area);

    let left_items: Vec<ListItem> = App::settings_categories()
        .iter()
        .enumerate()
        .map(|(i, cat)| {
            let prefix = if i == app.settings_category {
                "▸ "
            } else {
                "  "
            };
            ListItem::new(format!("{}{}", prefix, cat))
        })
        .collect();

    let left_list = List::new(left_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Categories "),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default().with_selected(Some(app.settings_category));
    f.render_stateful_widget(left_list, main[0], &mut state);

    let cat_name = App::settings_categories()[app.settings_category];
    let dirty = if app.settings_dirty { "● " } else { "" };
    let title = match app.settings_drill {
        Some(i) => format!(
            " {}{} > {} ",
            dirty,
            cat_name,
            drill_entry_name(app.settings_category, i, config)
        ),
        None => format!(" {}{} ", dirty, cat_name),
    };

    // AC10: a save error, an in-progress field-validation error, and a pending
    // scaffold prompt all surface in a single footer line UNDER the table (not
    // spliced into it). At most one is meaningful at a time: an edit error only
    // exists while editing, a save error only after a save, a scaffold prompt only
    // with an active offer; priority edit > save > scaffold keeps the focused
    // interaction's message foremost.
    let footer_msg: Option<(String, Color)> = if app.settings_editing {
        app.settings_edit_error
            .as_deref()
            .map(|e| (e.to_string(), Color::Red))
    } else {
        None
    }
    .or_else(|| {
        app.settings_footer_error
            .as_deref()
            .map(|e| (e.to_string(), Color::Red))
    })
    .or_else(|| {
        app.settings_scaffold_offer.as_ref().and_then(|offer| {
            offer.required_empty_field.is_some().then(|| {
                (
                    format!(
                        "Scaffolded [{}] -- press g to set salt",
                        scaffold_section_label(offer.inserted)
                    ),
                    Color::Yellow,
                )
            })
        })
    });

    let right = if let Some((msg, color)) = &footer_msg {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(main[1]);
        let footer = Paragraph::new(Line::from(Span::styled(
            format!(" {}", msg),
            Style::default().fg(*color).add_modifier(Modifier::BOLD),
        )));
        f.render_widget(footer, split[1]);
        split[0]
    } else {
        main[1]
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .padding(Padding::horizontal(1))
        .title(title);

    // Collection categories (1,2,3,6) that are not drilled render a navigable
    // entry-name LIST; everything else is a two-column field-view.
    let in_entry_list =
        matches!(app.settings_category, 1 | 2 | 3 | 6) && app.settings_drill.is_none();

    let highlight_style = Style::default().add_modifier(Modifier::REVERSED);

    if in_entry_list {
        // AC1 sibling: a single-column table whose selection cursor replaces the
        // former inline `▸` marker. Rows are sourced straight from the config model
        // (`settings_entry_names`), not by stripping a prefix off a rendered line.
        let mut rows: Vec<Row> = Vec::new();
        // cat 6 keeps a non-selectable `normalize` field above its override entries.
        if app.settings_category == 6 {
            if let Some(fld) = settings_fields(6, app.settings_entry, None, config).first() {
                rows.push(Row::new([Cell::from(field_line(fld))]));
            }
        }
        rows.extend(
            entry_list_rows(app.settings_category, config)
                .into_iter()
                .map(|n| Row::new([Cell::from(n)])),
        );
        // cat 6's leading `normalize` field offsets the entry cursor by one; other
        // collections select the entry row directly.
        let selected = if app.settings_category == 6 {
            app.settings_entry + 1
        } else {
            app.settings_entry
        };
        let selected = if rows.is_empty() {
            None
        } else {
            Some(selected.min(rows.len() - 1))
        };
        let table = Table::new(rows, [Constraint::Percentage(100)])
            .block(block)
            .row_highlight_style(highlight_style);
        let mut state = TableState::default().with_selected(selected);
        f.render_stateful_widget(table, right, &mut state);
        return;
    }

    let fields = settings_fields(
        app.settings_category,
        app.settings_entry,
        app.settings_drill,
        config,
    );
    let field_count = fields.len();
    let highlight_row = if field_count > 0 {
        Some(app.settings_field.min(field_count - 1))
    } else {
        None
    };
    // AC2: flag the sqids salt as required-but-empty whenever the section exists
    // with a blank salt (driven off buffer state, not the scaffold offer).
    let salt_required_empty = sqids_salt_required_empty(config);

    let rows: Vec<Row> = fields
        .iter()
        .enumerate()
        .map(|(i, fld)| {
            let is_salt_required = salt_required_empty && matches!(fld.path, FieldPath::SqidsSalt);
            // While editing the focused row, echo the live input + caret in the
            // value cell instead of the stale buffer value.
            let value = if highlight_row == Some(i) && app.settings_editing {
                settings_display_value(fld, true, &app.settings_edit_input)
            } else if is_salt_required {
                format!("{} (required)", fld.value)
            } else {
                fld.value.clone()
            };
            let value_cell = if is_salt_required && !app.settings_editing {
                Cell::from(Span::styled(
                    value,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Cell::from(value)
            };
            Row::new([Cell::from(fld.label.clone()), value_cell])
        })
        .collect();

    let widths = [Constraint::Percentage(40), Constraint::Percentage(60)];
    let table = Table::new(rows, widths)
        .header(
            Row::new(["Field", "Value"]).style(
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .block(block)
        .row_highlight_style(highlight_style);

    let mut state = TableState::default().with_selected(highlight_row);
    f.render_stateful_widget(table, right, &mut state);
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) fn doc_row_cells_for_test(
    id: &str,
    title: &str,
    status: &Status,
    tags: &[String],
    provenance: &[String],
    is_virtual: bool,
    dim: bool,
    type_name: &str,
    colors: &StatusPalette,
) -> Vec<Cell<'static>> {
    doc_row_cells(
        id,
        title,
        status,
        tags,
        provenance,
        &[],
        None,
        &BTreeMap::new(),
        is_virtual,
        dim,
        false,
        false,
        type_name,
        colors,
        &crate::engine::config::default_table_columns(),
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
    type_name: &str,
    colors: &StatusPalette,
) -> Vec<Cell<'static>> {
    doc_row_cells(
        id,
        title,
        status,
        tags,
        provenance,
        &[],
        None,
        &BTreeMap::new(),
        is_virtual,
        dim,
        is_gh,
        false,
        type_name,
        colors,
        &crate::engine::config::default_table_columns(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{RelSelector, TypeSelector};
    use crate::engine::status_colors::StatusColors;

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
            &Status::new("draft"),
            &[],
            &provenance,
            false,
            false,
            "rfc",
            &StatusPalette::default(),
        );
        // id + title + one cell per default column [status, tags, assignee, provenance].
        assert_eq!(cells.len(), 6);
        let dbg = cell_text(&cells[5]);
        assert!(
            dbg.contains("Alice"),
            "provenance cell should contain joined entries, got: {}",
            dbg
        );
    }

    // AC5 (TUI list): the assignee column renders the assignee name.
    #[test]
    fn doc_column_cell_renders_assignee_when_set() {
        let cell = doc_column_cell(
            "assignee",
            &Status::new("draft"),
            &[],
            &[],
            &[],
            Some("alice"),
            &BTreeMap::new(),
            false,
            false,
            "rfc",
            &StatusPalette::default(),
        );
        let dbg = cell_text(&cell);
        assert!(
            dbg.contains("alice"),
            "assignee cell should render name, got: {dbg}"
        );
    }

    // AC5 (TUI list): an unassigned doc's assignee column is blank.
    #[test]
    fn doc_column_cell_assignee_blank_when_none() {
        let cell = doc_column_cell(
            "assignee",
            &Status::new("draft"),
            &[],
            &[],
            &[],
            None,
            &BTreeMap::new(),
            false,
            false,
            "rfc",
            &StatusPalette::default(),
        );
        let dbg = cell_text(&cell);
        assert!(
            dbg.contains(r#"content: """#) || dbg.contains(r#""""#),
            "unset assignee should render empty, got: {dbg}"
        );
    }

    // AC5 (TUI detail): the preview header shows an Assignee line when set, and
    // omits it (with no extra line) when unset.
    #[test]
    fn preview_header_shows_assignee_when_set() {
        let mut doc = fixture_doc_meta();
        doc.assignee = Some("alice".to_string());
        let lines = build_preview_header_lines(&doc, false, &StatusPalette::default());
        let assignee_line = lines
            .iter()
            .find(|l| line_text(l).contains("Assignee:"))
            .map(line_text)
            .expect("assignee line should be present");
        assert!(assignee_line.contains("alice"), "got: {assignee_line}");
    }

    #[test]
    fn preview_header_omits_assignee_when_none() {
        let doc = fixture_doc_meta();
        let lines = build_preview_header_lines(&doc, false, &StatusPalette::default());
        for line in &lines {
            assert!(
                !line_text(line).contains("Assignee:"),
                "no Assignee line when unset"
            );
        }
    }

    #[test]
    fn doc_row_cells_provenance_empty_when_list_empty() {
        let cells = doc_row_cells_for_test(
            "RFC-001",
            "Title",
            &Status::new("draft"),
            &[],
            &[],
            false,
            false,
            "rfc",
            &StatusPalette::default(),
        );
        let dbg = cell_text(&cells[5]);
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

    #[test]
    fn doc_row_cells_status_uses_derived_colour_on_cache_hit() {
        let mut cache = StatusColors::default();
        cache.set_type(
            "rfc",
            std::collections::HashMap::from([("draft".to_string(), "#d33d44".to_string())]),
        );
        let colors = StatusPalette::new(std::collections::BTreeMap::new(), cache);
        let cells = doc_row_cells_for_test(
            "RFC-001",
            "Title",
            &Status::new("draft"),
            &[],
            &[],
            false,
            false,
            "rfc",
            &colors,
        );
        let dbg = cell_text(&cells[2]);
        assert!(
            dbg.contains("Rgb(211, 61, 68)"),
            "status cell fg should be the derived Rgb colour, got: {}",
            dbg
        );
    }

    #[test]
    fn doc_row_cells_status_falls_back_with_empty_cache() {
        let cells = doc_row_cells_for_test(
            "RFC-001",
            "Title",
            &Status::new("draft"),
            &[],
            &[],
            false,
            false,
            "rfc",
            &StatusPalette::default(),
        );
        let dbg = cell_text(&cells[2]);
        assert!(
            dbg.contains("Yellow") || dbg.contains("yellow"),
            "status cell fg should keep the hardcoded draft colour, got: {}",
            dbg
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
            status: Status::new("draft"),
            author: "jkaloger".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 4, 29).unwrap(),
            tags: vec![],
            related: vec![],
            provenance: vec![],
            validate_ignore: false,
            path: PathBuf::from("docs/rfcs/RFC-001.md"),
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
        }
    }

    #[test]
    fn preview_header_includes_provenance_when_present() {
        let mut doc = fixture_doc_meta();
        doc.provenance = vec!["X".to_string(), "Y".to_string()];
        let lines = build_preview_header_lines(&doc, false, &StatusPalette::default());
        let prov_line = lines
            .iter()
            .find(|l| line_text(l).contains("Provenance:"))
            .expect("provenance line should be present");
        let text = line_text(prov_line);
        assert!(text.contains('X'), "should contain X, got: {}", text);
        assert!(text.contains('Y'), "should contain Y, got: {}", text);
    }

    #[test]
    fn preview_header_lists_custom_attributes() {
        use crate::engine::document::AttrValue;
        let mut doc = fixture_doc_meta();
        doc.attributes
            .insert("estimate".to_string(), AttrValue::Int(5));
        doc.attributes.insert(
            "owner".to_string(),
            AttrValue::Raw(serde_yaml::Value::String("ada".to_string())),
        );
        let lines = build_preview_header_lines(&doc, false, &StatusPalette::default());
        let estimate = lines
            .iter()
            .find(|l| line_text(l).contains("estimate:"))
            .map(line_text)
            .expect("estimate attribute line should be present");
        assert!(estimate.contains('5'), "got: {}", estimate);
        let owner = lines
            .iter()
            .find(|l| line_text(l).contains("owner:"))
            .map(line_text)
            .expect("Raw attribute line should be present");
        assert!(owner.contains("ada"), "got: {}", owner);
    }

    #[test]
    fn preview_header_omits_attributes_when_empty() {
        let with_attrs = {
            use crate::engine::document::AttrValue;
            let mut doc = fixture_doc_meta();
            doc.attributes
                .insert("estimate".to_string(), AttrValue::Int(5));
            build_preview_header_lines(&doc, false, &StatusPalette::default()).len()
        };
        let empty = fixture_doc_meta();
        let without = build_preview_header_lines(&empty, false, &StatusPalette::default()).len();
        assert_eq!(without + 1, with_attrs);
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
        let columns = crate::engine::config::default_table_columns();
        let lines = row_content_lines("short", &[], &[], widths_for_test(40, 24, 20), &columns);
        assert_eq!(lines, 1);
    }

    #[test]
    fn row_content_lines_counts_explicit_newlines_in_title() {
        let columns = crate::engine::config::default_table_columns();
        let title = "line1\nline2\nline3";
        let lines = row_content_lines(title, &[], &[], widths_for_test(80, 24, 20), &columns);
        assert_eq!(lines, 3);
    }

    #[test]
    fn row_content_lines_soft_wraps_long_title() {
        // 30-char title soft-wrapped into width 10 should produce >1 lines.
        let columns = crate::engine::config::default_table_columns();
        let title = "alpha beta gamma delta epsilon zeta";
        let lines = row_content_lines(title, &[], &[], widths_for_test(10, 24, 20), &columns);
        assert!(lines > 1, "expected wrap, got {}", lines);
    }

    #[test]
    fn row_content_lines_takes_max_across_cells() {
        // Title fits on 1 line; provenance wraps to multiple.
        let columns = crate::engine::config::default_table_columns();
        let provenance: Vec<String> = (0..5).map(|i| format!("contributor-{}", i)).collect();
        let lines = row_content_lines("t", &[], &provenance, widths_for_test(80, 24, 10), &columns);
        assert!(lines > 1);
    }

    #[test]
    fn doc_cell_widths_resolves_title_from_area() {
        // Widths come from ratatui's Layout for the doc-table constraints.
        // Title (Fill) and provenance (Min 20) flex; tags is fixed at 24.
        let cols = crate::engine::config::default_table_columns();
        let widths = DocCellWidths::from_area_width(200, &cols);
        assert!(widths.title > 0);
        assert!(widths.title < 200);
        assert_eq!(widths.tags, 24);
        assert!(widths.provenance >= 20);
    }

    #[test]
    fn doc_cell_widths_title_scales_with_area() {
        let cols = crate::engine::config::default_table_columns();
        let small = DocCellWidths::from_area_width(80, &cols);
        let large = DocCellWidths::from_area_width(200, &cols);
        assert!(large.title > small.title);
    }

    #[test]
    fn doc_cell_widths_clamps_to_min_one_when_area_tiny() {
        let cols = crate::engine::config::default_table_columns();
        let widths = DocCellWidths::from_area_width(10, &cols);
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
        let columns = crate::engine::config::default_table_columns();
        let tags: Vec<String> = (0..10).map(|i| format!("tag-{}", i)).collect();
        let lines = row_content_lines("t", &tags, &[], widths_for_test(80, 12, 20), &columns);
        assert!(
            lines > 1,
            "expected multi-line from tag wrap, got {}",
            lines
        );
    }

    #[test]
    fn row_content_lines_ignores_tags_and_provenance_when_columns_exclude_them() {
        // Many tags and provenance entries, narrow widths that would wrap
        // to multiple lines if measured -- but columns only configure
        // "status", so tags/provenance must not inflate the row height.
        let tags: Vec<String> = (0..10).map(|i| format!("tag-{}", i)).collect();
        let provenance: Vec<String> = (0..5).map(|i| format!("contributor-{}", i)).collect();
        let columns = vec!["status".to_string()];
        let lines = row_content_lines(
            "short title",
            &tags,
            &provenance,
            widths_for_test(80, 12, 10),
            &columns,
        );
        assert_eq!(
            lines, 1,
            "tags/provenance wrap should not inflate height when their columns aren't configured, got {}",
            lines
        );
    }

    #[test]
    fn row_content_lines_still_measures_tags_and_provenance_when_columns_include_them() {
        // Same many-tags/provenance setup, but columns include "tags" and
        // "provenance" -- wrapping should still inflate the row height.
        let tags: Vec<String> = (0..10).map(|i| format!("tag-{}", i)).collect();
        let provenance: Vec<String> = (0..5).map(|i| format!("contributor-{}", i)).collect();
        let columns = vec![
            "status".to_string(),
            "tags".to_string(),
            "provenance".to_string(),
        ];
        let lines = row_content_lines(
            "short title",
            &tags,
            &provenance,
            widths_for_test(80, 12, 10),
            &columns,
        );
        assert!(
            lines > 1,
            "expected tag/provenance wrap to inflate height when columns configured, got {}",
            lines
        );
    }

    #[test]
    fn expanded_row_cells_render_full_tag_list() {
        let tags: Vec<String> = (0..6).map(|i| format!("tag-{}", i)).collect();
        let cells = doc_row_cells_expanded(
            "RFC-001",
            "Title",
            &Status::new("draft"),
            &tags,
            &[],
            &[],
            None,
            &BTreeMap::new(),
            false,
            false,
            false,
            false,
            widths_for_test(80, 12, 20),
            "rfc",
            &StatusPalette::default(),
            &crate::engine::config::default_table_columns(),
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
    fn doc_row_cells_renders_custom_attribute_column() {
        let mut attributes = BTreeMap::new();
        attributes.insert("priority".to_string(), AttrValue::Int(3));
        let columns = vec!["status".to_string(), "priority".to_string()];
        let cells = doc_row_cells(
            "RFC-001",
            "Title",
            &Status::new("draft"),
            &[],
            &[],
            &[],
            None,
            &attributes,
            false,
            false,
            false,
            false,
            "rfc",
            &StatusPalette::default(),
            &columns,
        );
        assert_eq!(
            cells.len(),
            4,
            "id + title + one cell per configured column"
        );
        let dbg = format!("{:?}", cells[3]);
        assert!(
            dbg.contains('3'),
            "custom attribute cell should render the value, got: {dbg}"
        );
    }

    #[test]
    fn doc_row_cells_absent_attribute_renders_empty() {
        let columns = vec!["priority".to_string()];
        let cells = doc_row_cells(
            "RFC-001",
            "Title",
            &Status::new("draft"),
            &[],
            &[],
            &[],
            None,
            &BTreeMap::new(),
            false,
            false,
            false,
            false,
            "rfc",
            &StatusPalette::default(),
            &columns,
        );
        let dbg = format!("{:?}", cells[2]);
        assert!(
            dbg.contains(r#"content: """#) || dbg.contains(r#""""#),
            "absent attribute should render empty, got: {dbg}"
        );
    }

    #[test]
    fn doc_row_cells_renders_related_column() {
        let related = vec!["STORY-001".to_string(), "STORY-002".to_string()];
        let columns = vec!["related".to_string()];
        let cells = doc_row_cells(
            "RFC-001",
            "Title",
            &Status::new("draft"),
            &[],
            &[],
            &related,
            None,
            &BTreeMap::new(),
            false,
            false,
            false,
            false,
            "rfc",
            &StatusPalette::default(),
            &columns,
        );
        let dbg = format!("{:?}", cells[2]);
        assert!(
            dbg.contains("STORY-001, STORY-002"),
            "related column should join neighbour ids, got: {dbg}"
        );
    }

    #[test]
    fn preview_header_omits_provenance_when_empty() {
        let doc = fixture_doc_meta();
        let lines = build_preview_header_lines(&doc, false, &StatusPalette::default());
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
            .constraints(doc_table_widths(
                width,
                &crate::engine::config::default_table_columns(),
            ))
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
        assert_eq!(widths[6], 16, "assignee == 16");
        assert!(widths[7] >= 20, "provenance >= 20, got {}", widths[7]);
    }

    #[test]
    fn doc_table_widths_medium_drops_assignee_and_provenance() {
        // width=90: inner=88. essentials+title (50) + status (12) + tags (24) = 86,
        // leaving 2 cols — below both ASSIGNEE_COLS and PROV_MIN_COLS, so assignee
        // and provenance collapse to 0.
        let widths = resolve_doc_widths(90);
        assert_eq!(widths[7], 0, "provenance dropped");
        assert_eq!(widths[6], 0, "assignee dropped");
        assert_eq!(widths[5], 24, "tags retained");
        assert_eq!(widths[4], 12, "status retained");
        assert!(widths[3] >= 20, "title >= 20, got {}", widths[3]);
    }

    #[test]
    fn doc_table_widths_narrow_drops_tags_assignee_and_provenance() {
        // width=70: inner=68. essentials+title (50) + status (12) = 62, leaving 6
        // cols — below TAGS_COLS, so tags, assignee, and provenance all collapse.
        let widths = resolve_doc_widths(70);
        assert_eq!(widths[5], 0, "tags dropped");
        assert_eq!(widths[6], 0, "assignee dropped");
        assert_eq!(widths[7], 0, "provenance dropped");
        assert_eq!(widths[4], 12, "status retained");
        assert!(widths[3] >= 20, "title >= 20, got {}", widths[3]);
    }

    #[test]
    fn doc_table_widths_very_narrow_drops_status() {
        // width=50: inner=48, after essentials (30) = 18 — below TITLE_MIN_COLS,
        // so title takes remaining and every optional column collapses.
        let widths = resolve_doc_widths(50);
        assert_eq!(widths[4], 0, "status dropped");
        assert_eq!(widths[5], 0, "tags dropped");
        assert_eq!(widths[6], 0, "assignee dropped");
        assert_eq!(widths[7], 0, "provenance dropped");
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
        let cells =
            DocCellWidths::from_area_width(80, &crate::engine::config::default_table_columns());
        assert_eq!(cells.title, resolved[3].max(1), "title agrees with split");
        assert_eq!(cells.tags, resolved[5].max(1), "tags agrees with split");
        assert_eq!(
            cells.provenance,
            resolved[7].max(1),
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

    fn graph_node_fixture(depth: usize, related: Vec<String>) -> GraphNode {
        use crate::engine::document::DocType;
        GraphNode {
            path: PathBuf::from("docs/iterations/ITERATION-001.md"),
            title: "Design".to_string(),
            doc_type: DocType::new("iteration"),
            status: Status::new("draft"),
            depth,
            related,
            attributes: std::collections::BTreeMap::new(),
            reverse: false,
        }
    }

    #[test]
    fn is_last_child_detects_last_sibling_that_has_children() {
        // RFC(0) -> A(1) [has child], B(1). A is NOT last (B follows at depth 1).
        // B(1) -> C(2). B IS last at depth 1 even though its next row (C) is deeper.
        let nodes = vec![
            graph_node_fixture(0, vec![]),
            graph_node_fixture(1, vec![]),
            graph_node_fixture(2, vec![]),
            graph_node_fixture(1, vec![]),
            graph_node_fixture(2, vec![]),
        ];
        assert!(!is_last_child(&nodes, 1), "A has sibling B at depth 1");
        assert!(is_last_child(&nodes, 2), "C is the only child of A");
        assert!(
            is_last_child(&nodes, 3),
            "B is the last depth-1 node despite having a deeper child after it"
        );
        assert!(is_last_child(&nodes, 4), "C2 is the last node");
    }

    fn spans_text(spans: &[Span]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect::<String>()
    }

    #[test]
    fn graph_node_full_node_appends_related_annotation() {
        let node = graph_node_fixture(1, vec!["ADR-001".to_string()]);
        let spans = graph_node_spans(&node, "◆", true, &[]);
        let text = spans_text(&spans);

        assert!(
            text.contains(" \u{2504}\u{25B7} ADR-001"),
            "full node should append the related annotation, got: {}",
            text
        );
        // Annotation follows the status, not before it.
        let status_idx = text.find("draft").expect("status present");
        let anno_idx = text.find("\u{2504}\u{25B7}").expect("annotation present");
        assert!(
            anno_idx > status_idx,
            "annotation must come after status, got: {}",
            text
        );
        let anno_span = spans
            .iter()
            .find(|s| s.content.contains("\u{2504}\u{25B7}"))
            .expect("annotation span present");
        assert_eq!(anno_span.style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn graph_node_multiple_related_annotations_each_appended() {
        let node = graph_node_fixture(1, vec!["ADR-001".to_string(), "STORY-005".to_string()]);
        let text = spans_text(&graph_node_spans(&node, "◆", true, &[]));
        assert!(
            text.contains(" \u{2504}\u{25B7} ADR-001 \u{2504}\u{25B7} STORY-005"),
            "each related id gets its own annotation glyph, got: {}",
            text
        );
    }

    #[test]
    fn graph_node_full_node_no_related_is_backward_compatible() {
        // Pre-Task-3 format: connector + icon + "title " + status, nothing else.
        let node = graph_node_fixture(1, vec![]);
        let text = spans_text(&graph_node_spans(&node, "◆", true, &[]));
        assert_eq!(text, "\u{2514}\u{2500}\u{25B6} \u{25C6} Design draft");
        assert!(!text.contains('\u{21B3}'), "no back-ref glyph");
        assert!(!text.contains('\u{2504}'), "no annotation glyph");
    }

    #[test]
    fn graph_node_root_full_node_has_no_connector() {
        // depth 0 keeps the original no-indent shape.
        let node = graph_node_fixture(0, vec![]);
        let text = spans_text(&graph_node_spans(&node, "◆", true, &[]));
        assert_eq!(text, "\u{25C6} Design draft");
    }

    /// A reverse row: a chain ancestor re-parented under its anchor by an
    /// anchored forest (STORY-247).
    fn reverse_graph_node_fixture(depth: usize, title: &str) -> GraphNode {
        GraphNode {
            reverse: true,
            title: title.to_string(),
            ..graph_node_fixture(depth, vec![])
        }
    }

    #[test]
    fn graph_reverse_row_flips_the_arrowhead_upward() {
        let node = reverse_graph_node_fixture(1, "Design");
        let text = spans_text(&graph_doc_cell_spans(&node, "\u{25C6}", true, &[]));

        assert_eq!(text, "\u{2514}\u{2500}\u{2191} \u{25C6} Design");
        assert!(
            !text.contains('\u{25B6}'),
            "an upward edge keeps no forward arrowhead, got: {text}"
        );
        assert!(
            !text.contains('\u{25B2}'),
            "the marker must not read as the story icon or the header sort arrow, got: {text}"
        );
    }

    #[test]
    fn graph_reverse_and_forward_siblings_render_distinct_connectors() {
        // The mid-chain (`story`) pivot shape: an anchor with one forward child and
        // one inverted ancestor, both at depth 1 -- the pair AC8 asks to be
        // distinguishable.
        let nodes = vec![
            GraphNode {
                title: "Story".to_string(),
                ..graph_node_fixture(0, vec![])
            },
            GraphNode {
                title: "Iteration".to_string(),
                ..graph_node_fixture(1, vec![])
            },
            reverse_graph_node_fixture(1, "Rfc"),
        ];
        let stems = compute_stems(&nodes);

        let cells: Vec<Vec<Span>> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| graph_doc_cell_spans(n, "\u{25C6}", is_last_child(&nodes, i), &stems[i]))
            .collect();

        assert_eq!(
            cells.iter().map(|c| spans_text(c)).collect::<Vec<_>>(),
            vec![
                "\u{25C6} Story",
                "\u{251C}\u{2500}\u{25B6} \u{25C6} Iteration",
                "\u{2514}\u{2500}\u{2191} \u{25C6} Rfc",
            ]
        );

        let connector_style = |cell: &[Span]| cell[0].style;
        assert_ne!(
            connector_style(&cells[2]),
            connector_style(&cells[1]),
            "the upward marker is styled apart from the forward connector"
        );
    }

    #[test]
    fn graph_reverse_chain_indents_each_ancestor_one_level_deeper() {
        // The leaf (`iteration`) pivot's read: anchor, then its story, then its RFC.
        let nodes = vec![
            GraphNode {
                title: "Iteration".to_string(),
                ..graph_node_fixture(0, vec![])
            },
            reverse_graph_node_fixture(1, "Story"),
            reverse_graph_node_fixture(2, "Rfc"),
        ];
        let stems = compute_stems(&nodes);

        let lines: Vec<String> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                spans_text(&graph_doc_cell_spans(
                    n,
                    "\u{25C6}",
                    is_last_child(&nodes, i),
                    &stems[i],
                ))
            })
            .collect();

        assert_eq!(
            lines,
            vec![
                "\u{25C6} Iteration",
                "\u{2514}\u{2500}\u{2191} \u{25C6} Story",
                "   \u{2514}\u{2500}\u{2191} \u{25C6} Rfc",
            ]
        );
    }

    #[test]
    fn settings_lines_general_shows_three_fields() {
        let config = Config::default();
        let lines = settings_lines_inner(0, 0, None, &config);
        assert_eq!(lines.len(), 3);
        assert!(
            lines.iter().any(|l| l.starts_with("naming.pattern:")),
            "missing naming.pattern, got: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.starts_with("ref_count_ceiling:")),
            "missing ref_count_ceiling, got: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.starts_with("templates.dir:")),
            "missing templates.dir, got: {lines:?}"
        );
    }

    #[test]
    fn settings_lines_general_includes_values() {
        let config = Config::default();
        let lines = settings_lines_inner(0, 0, None, &config);
        assert!(lines
            .iter()
            .any(|l| l.contains(&config.documents.naming.pattern)));
        assert!(lines
            .iter()
            .any(|l| l.contains(&config.ref_count_ceiling.to_string())));
        assert!(lines
            .iter()
            .any(|l| l.contains(&config.filesystem.templates.dir)));
    }

    #[test]
    fn settings_lines_github_absent_shows_unset() {
        let config = Config::default();
        let lines = settings_lines_inner(5, 0, None, &config);
        assert_eq!(lines.len(), 2);
        assert!(lines.contains(&"repo: (unset)".to_string()));
        assert!(lines.contains(&"cache_ttl: (unset)".to_string()));
    }

    #[test]
    fn settings_lines_doc_types_drilled_shows_eleven_fields() {
        let config = Config::default();
        let lines = settings_lines_inner(1, 0, Some(0), &config);
        let expected_labels = [
            "name:",
            "plural:",
            "dir:",
            "prefix:",
            "icon:",
            "numbering:",
            "subdirectory:",
            "store:",
            "singleton:",
            "parent_type:",
            "agents:",
        ];
        for label in &expected_labels {
            assert!(
                lines.iter().any(|l| l.starts_with(label)),
                "missing field {label}, got: {lines:?}"
            );
        }
        // rfc (index 0) has icon Some("●"), not (unset)
        assert!(
            lines.contains(&"icon: ●".to_string()),
            "expected icon: ●, got: {lines:?}"
        );
    }

    #[test]
    fn settings_lines_doc_types_not_drilled_shows_entries() {
        let config = Config::default();
        let lines = settings_lines_inner(1, 0, None, &config);
        assert_eq!(lines.len(), config.documents.types.len());
        assert!(lines.iter().any(|l| l.contains("rfc")));
        assert!(lines.iter().any(|l| l.contains("▸")));
    }

    // The render and the legacy display-line builder share one model accessor:
    // entry names come straight from config, with no prefix decoration.
    #[test]
    fn settings_entry_names_source_from_model_without_prefix() {
        let config = Config::default();
        let names = settings_entry_names(1, &config);
        assert_eq!(
            names,
            config
                .documents
                .types
                .iter()
                .map(|t| t.name.clone())
                .collect::<Vec<_>>()
        );
        assert!(
            names
                .iter()
                .all(|n| !n.starts_with('▸') && !n.starts_with(' ')),
            "names carry no selection prefix"
        );
        assert!(
            settings_entry_names(0, &config).is_empty(),
            "non-collection categories have no entries"
        );
    }

    #[test]
    fn settings_lines_agents_absent_shows_unset() {
        let config = Config::default();
        let lines = settings_lines_inner(7, 0, None, &config);
        assert!(lines.contains(&"interactive: (unset)".to_string()));
    }

    // AC2 (ITERATION-189): the render keys the required-but-empty salt flag off
    // this buffer-state predicate. A scaffolded (empty-salt) section flags; a
    // non-empty salt does not; an absent section does not.
    #[test]
    fn sqids_salt_required_empty_flags_only_empty_present_salt() {
        let mut config = Config::default();
        assert!(
            !sqids_salt_required_empty(&config),
            "no section => not flagged"
        );

        config.documents.sqids = Some(crate::engine::config::SqidsConfig {
            salt: String::new(),
            min_length: 3,
        });
        assert!(
            sqids_salt_required_empty(&config),
            "present section with empty salt => flagged (AC2)"
        );

        config.documents.sqids = Some(crate::engine::config::SqidsConfig {
            salt: "seed".to_string(),
            min_length: 3,
        });
        assert!(
            !sqids_salt_required_empty(&config),
            "a non-empty salt is never flagged"
        );
    }

    #[test]
    fn settings_lines_numbering_all_unset() {
        let config = Config::default();
        let lines = settings_lines_inner(4, 0, None, &config);
        assert_eq!(lines.len(), 5);
        assert!(lines.contains(&"sqids.salt: (unset)".to_string()));
        assert!(lines.contains(&"sqids.min_length: (unset)".to_string()));
        assert!(lines.contains(&"reserved.remote: (unset)".to_string()));
        assert!(lines.contains(&"reserved.format: (unset)".to_string()));
        assert!(lines.contains(&"reserved.max_retries: (unset)".to_string()));
    }

    #[test]
    fn settings_lines_interface_default() {
        let config = Config::default();
        let lines = settings_lines_inner(8, 0, None, &config);
        assert_eq!(lines.len(), 6);
        assert!(lines.contains(&"ascii_diagrams: false".to_string()));
        assert!(lines.contains(&"statusbar.enabled: true".to_string()));
        assert!(lines.contains(&"statusbar.left: (unset)".to_string()));
        assert!(lines.contains(&"statusbar.center: (unset)".to_string()));
        assert!(lines.contains(&"statusbar.right: (unset)".to_string()));
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("multiline.max_expanded_height:")),
            "missing multiline field, got: {lines:?}"
        );
    }

    #[test]
    fn settings_lines_relationships_not_drilled_shows_entries() {
        let config = Config::default();
        let lines = settings_lines_inner(2, 0, None, &config);
        assert_eq!(lines.len(), config.relationships.len());
        assert!(lines.iter().any(|l| l.contains("implements")));
        assert!(lines.iter().any(|l| l.contains("related-to")));
    }

    #[test]
    fn settings_lines_relationships_drilled_shows_fields() {
        let config = Config::default();
        let lines = settings_lines_inner(2, 0, Some(0), &config);
        assert!(lines.iter().any(|l| l.starts_with("name:")));
        assert!(lines.iter().any(|l| l.starts_with("inverse:")));
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn settings_lines_certification_empty_overrides() {
        let config = Config::default();
        let lines = settings_lines_inner(6, 0, None, &config);
        assert!(lines.iter().any(|l| l.starts_with("normalize:")));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("no certification overrides")),
            "got: {lines:?}"
        );
    }

    #[test]
    fn settings_lines_unknown_category_returns_empty() {
        let config = Config::default();
        let lines = settings_lines_inner(999, 0, None, &config);
        assert!(lines.is_empty());
    }

    // STORY-259 retired the `[[rules]]` category; STORY-260 puts Edges in the
    // slot it vacated, so index 3 is a collection again -- of `[[edges]]` rows,
    // the only place the DAG is declared. Numbering moves down one.
    #[test]
    fn settings_categories_offer_edges_where_validation_rules_was() {
        let cats = App::settings_categories();
        assert!(
            !cats.iter().any(|c| c.contains("Rule")),
            "the rules editor is retired: {cats:?}"
        );
        assert_eq!(
            cats[3], "Edges",
            "Edges took the retired category's index: {cats:?}"
        );
        assert_eq!(cats[4], "Numbering", "Numbering sits below Edges: {cats:?}");
    }

    /// A config carrying one fully-stated edge and one wildcard edge: between
    /// them they exercise every position spelling the panel has to render.
    fn edges_fixture() -> Config {
        Config {
            edges: vec![
                EdgeDef {
                    name: "iterations-implement-work".to_string(),
                    from: TypeSelector::Types(vec!["iteration".to_string()]),
                    to: TypeSelector::Types(vec!["story".to_string(), "bug".to_string()]),
                    via: RelSelector::Named(vec!["implements".to_string()]),
                    required: Some(Severity::Error),
                    traversal: Some(Traversal::Chain),
                },
                EdgeDef {
                    name: "general-relatedness".to_string(),
                    from: TypeSelector::Any,
                    to: TypeSelector::Any,
                    via: RelSelector::Any,
                    required: None,
                    traversal: None,
                },
            ],
            ..Default::default()
        }
    }

    // STORY-260 AC1: the entry list is where a designer reads the DAG off, so
    // an edge's line carries the whole triple rather than a bare name.
    #[test]
    fn settings_entry_names_carry_each_edge_triple() {
        let names = settings_entry_names(3, &edges_fixture());

        assert_eq!(
            names,
            vec![
                "iterations-implement-work: iteration -implements-> [story, bug]",
                "general-relatedness: * -*-> *",
            ]
        );
    }

    // STORY-260 AC1: drilling an edge shows its keys in `EdgeDef`'s order.
    #[test]
    fn settings_fields_edge_keys_render_in_declaration_order() {
        let fields = settings_fields(3, 0, Some(0), &edges_fixture());

        assert_eq!(
            fields.iter().map(|f| f.label.as_str()).collect::<Vec<_>>(),
            ["name", "from", "to", "via", "required", "traversal"]
        );
        assert_eq!(
            fields.iter().map(|f| f.value.as_str()).collect::<Vec<_>>(),
            [
                "iterations-implement-work",
                "iteration",
                "[story, bug]",
                "implements",
                "error",
                "chain",
            ]
        );
        assert_eq!(
            fields.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
            [
                EdgeKey::Name,
                EdgeKey::From,
                EdgeKey::To,
                EdgeKey::Via,
                EdgeKey::Required,
                EdgeKey::Traversal,
            ]
            .map(|key| FieldPath::Edge { index: 0, key })
        );
    }

    // STORY-260 AC3: the two type positions carry the member-at-a-time picker
    // and nothing else -- leaving the interim comma editor live on either would
    // be two spellings of one edit. `via` keeps the comma editor: it is a
    // relationship set, and ITERATION-387 settled that.
    #[test]
    fn settings_fields_edge_type_positions_carry_the_picker_and_via_the_comma_editor() {
        let fields = settings_fields(3, 0, Some(0), &edges_fixture());

        assert_eq!(field_by_label(&fields, "from").editor, FieldEditor::TypeSet);
        assert_eq!(field_by_label(&fields, "to").editor, FieldEditor::TypeSet);
        assert_eq!(field_by_label(&fields, "via").editor, FieldEditor::List);
    }

    // STORY-260 AC2: the optional qualifiers cycle over a list that leads with
    // the unset entry, so the cycler can say everything the file can -- an
    // absent key included.
    #[test]
    fn settings_fields_edge_optional_qualifiers_cycle_through_unset() {
        let fields = settings_fields(3, 0, Some(0), &edges_fixture());

        assert_eq!(
            field_by_label(&fields, "required").editor,
            FieldEditor::EnumCycle {
                variants: &["(unset)", "error", "warning"]
            }
        );
        assert_eq!(
            field_by_label(&fields, "traversal").editor,
            FieldEditor::EnumCycle {
                variants: &["(unset)", "chain", "related"]
            }
        );
    }

    // A row silent on `required`/`traversal` states no requiredness and joins
    // no walk (ADR-030); the panel shows that as unset, not as a default value.
    #[test]
    fn settings_fields_edge_shows_an_unstated_qualifier_as_unset() {
        let fields = settings_fields(3, 1, Some(1), &edges_fixture());

        assert_eq!(field_by_label(&fields, "required").value, "(unset)");
        assert_eq!(field_by_label(&fields, "traversal").value, "(unset)");
        assert_eq!(field_by_label(&fields, "via").value, "*");
    }

    // The drilled view is titled by the edge's identity, not by the triple its
    // entry-list line carries: a breadcrumb names the row it drilled into.
    #[test]
    fn drill_entry_name_titles_an_edge_by_name_alone() {
        assert_eq!(
            drill_entry_name(3, 0, &edges_fixture()),
            "iterations-implement-work"
        );
    }

    fn field_by_label<'a>(fields: &'a [EditableField], label: &str) -> &'a EditableField {
        fields
            .iter()
            .find(|f| f.label == label)
            .unwrap_or_else(|| panic!("no field labelled {label}, got: {fields:?}"))
    }

    #[test]
    fn settings_fields_general_ref_count_ceiling_is_bounded_num() {
        let config = Config::default();
        let fields = settings_fields(0, 0, None, &config);
        let f = field_by_label(&fields, "ref_count_ceiling");
        assert_eq!(f.editor, FieldEditor::BoundedNum { min: 1, max: 1000 });
        assert_eq!(f.value, config.ref_count_ceiling.to_string());
        assert_eq!(f.path, FieldPath::RefCountCeiling);
    }

    #[test]
    fn settings_fields_type_numbering_is_enum_cycle() {
        let config = Config::default();
        let fields = settings_fields(1, 0, Some(0), &config);
        let f = field_by_label(&fields, "numbering");
        assert_eq!(
            f.editor,
            FieldEditor::EnumCycle {
                variants: &["incremental", "sqids", "reserved"]
            }
        );
    }

    #[test]
    fn settings_fields_type_subdirectory_is_toggle() {
        let config = Config::default();
        let fields = settings_fields(1, 0, Some(0), &config);
        let f = field_by_label(&fields, "subdirectory");
        assert_eq!(f.editor, FieldEditor::Toggle);
    }

    #[test]
    fn settings_fields_github_repo_nullable_when_present() {
        let toml_str = r#"
[github]
repo = "owner/repo"

[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "related-to"
"#;
        let config = Config::parse(toml_str).unwrap();
        let fields = settings_fields(5, 0, None, &config);
        let repo = field_by_label(&fields, "repo");
        assert_eq!(repo.editor, FieldEditor::Nullable);
        assert_eq!(repo.path, FieldPath::GithubRepo);
        let ttl = field_by_label(&fields, "cache_ttl");
        assert!(matches!(ttl.editor, FieldEditor::BoundedNum { .. }));
    }

    #[test]
    fn settings_fields_sqids_read_only_when_section_absent() {
        let config = Config::default();
        let fields = settings_fields(4, 0, None, &config);
        let salt = field_by_label(&fields, "sqids.salt");
        assert_eq!(salt.editor, FieldEditor::ReadOnly);
        assert_eq!(salt.path, FieldPath::Unset);
        let min = field_by_label(&fields, "sqids.min_length");
        assert_eq!(min.editor, FieldEditor::ReadOnly);
    }

    // AC1: the Interface category surfaces the full UiConfig as editable rows,
    // reflecting defaults when [tui] is unset.
    #[test]
    fn settings_fields_interface_surfaces_full_uiconfig_with_defaults() {
        let config = Config::default();
        let fields = settings_fields(8, 0, None, &config);

        let expected: &[(&str, FieldEditor, FieldPath)] = &[
            (
                "ascii_diagrams",
                FieldEditor::Toggle,
                FieldPath::UiAsciiDiagrams,
            ),
            (
                "statusbar.enabled",
                FieldEditor::Toggle,
                FieldPath::StatusbarEnabled,
            ),
            (
                "statusbar.left",
                FieldEditor::ZoneOrdering,
                FieldPath::StatusbarLeft,
            ),
            (
                "statusbar.center",
                FieldEditor::ZoneOrdering,
                FieldPath::StatusbarCenter,
            ),
            (
                "statusbar.right",
                FieldEditor::ZoneOrdering,
                FieldPath::StatusbarRight,
            ),
            (
                "multiline.max_expanded_height",
                FieldEditor::BoundedNum { min: 1, max: 1000 },
                FieldPath::MultilineMaxExpandedHeight,
            ),
        ];
        assert_eq!(fields.len(), expected.len(), "got: {fields:?}");
        for (label, editor, path) in expected {
            let f = field_by_label(&fields, label);
            assert_eq!(&f.editor, editor, "editor mismatch for {label}");
            assert_eq!(&f.path, path, "path mismatch for {label}");
        }

        // Default values surface, not (unset): ascii false, statusbar enabled true,
        // multiline 5.
        assert_eq!(field_by_label(&fields, "ascii_diagrams").value, "false");
        assert_eq!(field_by_label(&fields, "statusbar.enabled").value, "true");
        assert_eq!(
            field_by_label(&fields, "multiline.max_expanded_height").value,
            "5"
        );
    }

    // AC1: explicit [tui] values surface in the rows.
    #[test]
    fn settings_fields_interface_reflects_explicit_values() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "related-to"

[tui]
ascii_diagrams = true

[tui.statusbar]
enabled = false
left = ["mode", "git_branch"]

[tui.multiline]
max_expanded_height = 8
"#;
        let config = Config::parse(toml_str).unwrap();
        let fields = settings_fields(8, 0, None, &config);
        assert_eq!(field_by_label(&fields, "ascii_diagrams").value, "true");
        assert_eq!(field_by_label(&fields, "statusbar.enabled").value, "false");
        assert_eq!(
            field_by_label(&fields, "multiline.max_expanded_height").value,
            "8"
        );
        assert_eq!(
            field_by_label(&fields, "statusbar.left").value,
            "mode, git_branch"
        );
    }

    // An entry list is entry names: the field list stays empty until a row is
    // drilled, Edges (cat 3) included.
    #[test]
    fn settings_fields_entry_list_views_are_empty() {
        assert!(settings_fields(1, 0, None, &Config::default()).is_empty());
        assert!(settings_fields(2, 0, None, &Config::default()).is_empty());
        assert!(settings_fields(3, 0, None, &edges_fixture()).is_empty());
    }

    // A legal config can declare no edges, and none of the other collections
    // has to be populated either, so the pane says what is missing and names
    // the key that adds one rather than rendering blank.
    #[test]
    fn an_empty_entry_list_names_what_is_missing_and_how_to_add_it() {
        let mut empty = Config::default();
        empty.documents.types.clear();
        empty.relationships.clear();
        empty.edges.clear();
        empty.certification.overrides.clear();

        for (category, missing) in [
            (1, "document types"),
            (2, "relationships"),
            (3, "edges"),
            (6, "overrides"),
        ] {
            let rows = entry_list_rows(category, &empty);
            assert_eq!(rows.len(), 1, "cat {category} lists nothing: {rows:?}");
            assert!(
                rows[0].contains(missing),
                "cat {category} names what is missing: {}",
                rows[0]
            );
            assert!(
                rows[0].contains("press n"),
                "cat {category} names the key that adds one: {}",
                rows[0]
            );
        }
    }

    #[test]
    fn a_populated_entry_list_is_its_entry_names_alone() {
        let rows = entry_list_rows(3, &edges_fixture());

        assert_eq!(rows.len(), edges_fixture().edges.len());
        assert!(
            !rows.iter().any(|row| row.contains("press n")),
            "no hint where there are rows: {rows:?}"
        );
    }

    #[test]
    fn settings_fields_cert_normalize_top_is_toggle() {
        let config = Config::default();
        let fields = settings_fields(6, 0, None, &config);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].label, "normalize");
        assert_eq!(fields[0].editor, FieldEditor::Toggle);
        assert_eq!(fields[0].path, FieldPath::CertNormalize);
    }

    #[test]
    fn settings_fields_and_lines_agree_for_field_view() {
        // Two drilled collections (a doc type, an edge), the first and last
        // scalar categories, and one past the last category.
        let cases: [(usize, Option<usize>, Config); 5] = [
            (0, None, Config::default()),
            (1, Some(0), Config::default()),
            (3, Some(0), edges_fixture()),
            (8, None, Config::default()),
            (9, None, Config::default()),
        ];
        for (cat, drill, config) in cases {
            let fields = settings_fields(cat, 0, drill, &config);
            let lines = settings_lines_inner(cat, 0, drill, &config);
            let derived: Vec<String> = fields
                .iter()
                .map(|f| format!("{}: {}", f.label, f.value))
                .collect();
            assert_eq!(
                derived, lines,
                "settings_fields and settings_lines_inner must agree for cat {cat}"
            );
        }
    }

    #[test]
    fn settings_display_value_echoes_input_with_caret_while_editing() {
        let field = EditableField {
            label: "naming.pattern".to_string(),
            value: "{type}-{title}.md".to_string(),
            editor: FieldEditor::Text,
            path: FieldPath::Naming,
        };
        // The focused, editing row shows the live input + caret, NOT the buffer value.
        let shown = settings_display_value(&field, true, "edited-{title}");
        assert_eq!(shown, "edited-{title}_");
        assert!(
            !shown.contains(&field.value),
            "stale buffer value must not leak while editing: {shown}"
        );
    }

    #[test]
    fn settings_display_value_shows_buffer_when_not_editing() {
        let field = EditableField {
            label: "naming.pattern".to_string(),
            value: "{type}-{title}.md".to_string(),
            editor: FieldEditor::Text,
            path: FieldPath::Naming,
        };
        // A non-editing row ignores the input buffer and shows the buffer value.
        assert_eq!(
            settings_display_value(&field, false, "ignored"),
            "{type}-{title}.md"
        );
    }

    #[test]
    fn settings_display_value_empty_input_shows_just_caret() {
        let field = EditableField {
            label: "github.repo".to_string(),
            value: "(unset)".to_string(),
            editor: FieldEditor::Nullable,
            path: FieldPath::GithubRepo,
        };
        // Editing an unset nullable starts from empty input -- caret only, not `(unset)`.
        assert_eq!(settings_display_value(&field, true, ""), "_");
    }
}
