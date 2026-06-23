use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::engine::document::Status;
use crate::engine::git_status::GitFileStatus;
use crate::tui::state::{App, FormField};

use super::colors::status_color;
use super::keybinds::{context_label, keybinds_for};

pub fn draw_help_overlay(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let ctx = app.active_key_context();
    let lines = help_lines(ctx);

    // Width fits the longest content line (plus 2 for borders), capped at the
    // current 50 cap and clamped to the area.
    let longest = lines.iter().map(Line::width).max().unwrap_or(0) as u16;
    let popup_width = longest
        .saturating_add(2)
        .clamp(20, 50)
        .min(area.width.saturating_sub(4));
    // Height fits the content (plus 2 for borders), capped at the area.
    let popup_height = (lines.len() as u16)
        .saturating_add(2)
        .min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    // Render-feeds-state: publish the max legal scroll so the key handler can
    // tell whether the content overflows (and clamp to it). 0 when it fits.
    let inner_height = popup_height.saturating_sub(2);
    app.help_max_scroll = (lines.len() as u16).saturating_sub(inner_height);
    app.help_scroll = app.help_scroll.min(app.help_max_scroll);
    let scroll = app.help_scroll;

    let paragraph = Paragraph::new(lines).scroll((scroll, 0)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!(" Help \u{2014} {} ", context_label(ctx))),
    );
    f.render_widget(paragraph, popup_area);
}

/// The help-overlay content lines for one context: each group renders a
/// bold/cyan section-title, a blank line, one line per bind, and a trailing
/// blank between groups. Pure so it is directly unit-testable.
fn help_lines(ctx: crate::tui::views::keybinds::KeyContext) -> Vec<Line<'static>> {
    let groups = keybinds_for(ctx);
    let mut lines: Vec<Line> = Vec::new();
    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            group.title,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        for kb in &group.binds {
            lines.push(Line::from(format!("  {:<9} {}", kb.keys, kb.desc)));
        }
    }
    lines
}

pub fn draw_create_form(f: &mut Frame, app: &App) {
    let area = f.area();

    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = 14.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let form = &app.create_form;
    let title = format!(" Create {} ", form.doc_type);

    let fields = [
        ("Title", &form.title, FormField::Title),
        ("Author", &form.author, FormField::Author),
        ("Tags", &form.tags, FormField::Tags),
        ("Related", &form.related, FormField::Related),
    ];

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    for (label, value, field) in &fields {
        let is_focused = form.focused_field == *field && !form.loading;
        let label_style = if form.loading {
            Style::default().fg(Color::DarkGray)
        } else if is_focused {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let value_style = if form.loading {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };

        let cursor = if is_focused { "_" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(format!("  {:<10}", format!("{}:", label)), label_style),
            Span::styled(format!("{}{}", value, cursor), value_style),
        ]));
        lines.push(Line::from(""));
    }

    if let Some(ref msg) = form.status_message {
        lines.push(Line::from(Span::styled(
            format!("  {}", msg),
            Style::default().fg(Color::Yellow),
        )));
    }

    if let Some(ref err) = form.error {
        lines.push(Line::from(Span::styled(
            format!("  {}", err),
            Style::default().fg(Color::Red),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Tab", Style::default().fg(Color::DarkGray)),
        Span::raw(" next  "),
        Span::styled("Enter", Style::default().fg(Color::DarkGray)),
        Span::raw(" create  "),
        Span::styled("Esc", Style::default().fg(Color::DarkGray)),
        Span::raw(" cancel"),
    ]));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(title),
    );
    f.render_widget(paragraph, popup_area);
}

pub fn draw_delete_confirm(f: &mut Frame, app: &App) {
    let area = f.area();
    let dc = &app.delete_confirm;

    let ref_count = dc.references.len();
    let content_height = if ref_count > 0 {
        6 + ref_count as u16
    } else {
        4
    };
    let popup_width = 50.min(area.width.saturating_sub(4));
    let popup_height = content_height.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let mut lines = vec![
        Line::from(""),
        Line::from(format!("  Delete \"{}\"?", dc.doc_title)),
    ];

    if !dc.references.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Referenced by:",
            Style::default().fg(Color::DarkGray),
        )));
        for (rel_type, path) in &dc.references {
            let name = display_name(path);
            lines.push(Line::from(format!("    \u{2022} {} ({})", name, rel_type)));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "         [Enter: delete]  [Esc: cancel]",
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Red))
            .title(" Delete? "),
    );
    f.render_widget(paragraph, popup_area);
}

pub fn draw_settings_quit_prompt(f: &mut Frame) {
    let area = f.area();

    let popup_width = 50u16.min(area.width.saturating_sub(4));
    let popup_height = 5u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let lines = vec![
        Line::from(""),
        Line::from("  Unsaved settings changes"),
        Line::from(""),
        Line::from(Span::styled(
            "  (s)ave  (d)iscard  (Esc) cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Quit? "),
    );
    f.render_widget(paragraph, popup_area);
}

pub fn draw_settings_delete_confirm(f: &mut Frame, app: &App) {
    let area = f.area();
    let dc = &app.settings_delete_confirm;

    let popup_width = 50u16.min(area.width.saturating_sub(4));
    let popup_height = 4u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let lines = vec![
        Line::from(""),
        Line::from(format!("  Delete \"{}\"?", dc.entry_label)),
        Line::from(""),
        Line::from(Span::styled(
            "         [Enter: delete]  [Esc: cancel]",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Red))
            .title(" Delete? "),
    );
    f.render_widget(paragraph, popup_area);
}

pub fn draw_settings_impact_confirm(f: &mut Frame, app: &App) {
    use crate::tui::state::settings_guard::impact_consequence;

    let area = f.area();
    let impacts = &app.settings_impact_confirm.impacts;

    // Per type: a header line, a field old->new line, and a wrapped consequence
    // line; blank line between blocks. Plus a leading blank, a blank, and a
    // footer line, on top of the 2 border rows.
    let block_lines = (impacts.len() as u16).saturating_mul(4);
    let content_height = block_lines.saturating_add(4);
    let popup_width = 64u16.min(area.width.saturating_sub(4));
    let popup_height = content_height
        .saturating_add(2)
        .min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let mut lines = vec![Line::from("")];

    for impact in impacts {
        lines.push(Line::from(Span::styled(
            format!("  {}", impact.type_name),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(format!(
            "    {}.{}: {} -> {}",
            impact.type_name, impact.field, impact.old, impact.new
        )));
        lines.push(Line::from(Span::styled(
            format!("    {}", impact_consequence(impact)),
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        "  [Enter/y: confirm write]  [Esc/n: cancel]",
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Red))
                .title(" Affected documents "),
        );
    f.render_widget(paragraph, popup_area);
}

pub fn draw_override_key_prompt(f: &mut Frame, app: &App) {
    let area = f.area();

    let popup_width = 60u16.min(area.width.saturating_sub(4));
    let popup_height = 5u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let lines = vec![
        Line::from(""),
        Line::from(format!("  {}", app.override_key_prompt.input)),
        Line::from(""),
        Line::from(Span::styled(
            "  [Enter: add]  [Esc: cancel]",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Override spec-path "),
    );
    f.render_widget(paragraph, popup_area);
}

/// The two-pane status-bar zone ordering editor: `Selected` (ordered, left) and
/// `Available` (the remaining RFC-022 vocabulary, right). The focused pane gets a
/// cyan border, the cursor row is highlighted. Render-only; all state lives in
/// `app.settings_zone_editor`.
pub fn draw_settings_zone_editor(f: &mut Frame, app: &App) {
    use crate::tui::state::forms::ZonePane;

    let Some(editor) = app.settings_zone_editor.as_ref() else {
        return;
    };

    let area = f.area();
    let popup_width = 60u16.min(area.width.saturating_sub(4));
    let popup_height = 18u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Status-bar zone order ");
    let inner = outer.inner(popup_area);
    f.render_widget(outer, popup_area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[0]);

    let pane = |title: &'static str, names: &[String], active: bool| {
        let border = if active { Color::Cyan } else { Color::DarkGray };
        let items: Vec<ListItem> = names.iter().map(|n| ListItem::new(n.clone())).collect();
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border))
                    .title(format!(" {} ", title)),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
    };

    let selected_active = editor.pane == ZonePane::Selected;
    let available_active = editor.pane == ZonePane::Available;
    let selected_list = pane("Selected", &editor.selected, selected_active);
    let available_list = pane("Available", &editor.available, available_active);

    // The single cursor highlights only the active pane.
    let mut selected_state = ListState::default();
    if selected_active && !editor.selected.is_empty() {
        selected_state.select(Some(editor.cursor.min(editor.selected.len() - 1)));
    }
    let mut available_state = ListState::default();
    if available_active && !editor.available.is_empty() {
        available_state.select(Some(editor.cursor.min(editor.available.len() - 1)));
    }

    f.render_stateful_widget(selected_list, panes[0], &mut selected_state);
    f.render_stateful_widget(available_list, panes[1], &mut available_state);

    let hint = Paragraph::new(Line::from(Span::styled(
        "[Tab: pane] [Space/Enter: add/remove] [K/J: reorder] [c: commit] [Esc: cancel]",
        Style::default().fg(Color::DarkGray),
    )));
    f.render_widget(hint, rows[1]);
}

/// The enum variant-picker overlay: a centered list of the field's variants with
/// the current cursor highlighted. Render-only; all state lives in
/// `app.settings_variant_picker` (RFC-023 / STORY-144).
pub fn draw_settings_variant_picker(f: &mut Frame, app: &App) {
    let Some(picker) = app.settings_variant_picker.as_ref() else {
        return;
    };

    let area = f.area();
    let popup_width = 30u16.min(area.width.saturating_sub(4));
    let popup_height = (picker.variants.len() as u16 + 4).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Select value ");
    let inner = outer.inner(popup_area);
    f.render_widget(outer, popup_area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let items: Vec<ListItem> = picker.variants.iter().map(|v| ListItem::new(*v)).collect();
    let list = List::new(items).highlight_style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    if !picker.variants.is_empty() {
        state.select(Some(picker.selected.min(picker.variants.len() - 1)));
    }
    f.render_stateful_widget(list, rows[0], &mut state);

    let hint = Paragraph::new(Line::from(Span::styled(
        "[j/k: move] [Enter: select] [Esc: cancel]",
        Style::default().fg(Color::DarkGray),
    )));
    f.render_widget(hint, rows[1]);
}

pub fn draw_status_picker(f: &mut Frame, app: &App) {
    let area = f.area();

    let statuses: Vec<Status> = app
        .status_picker
        .states
        .iter()
        .map(|s| Status::new(s))
        .collect();

    let popup_width = 25u16.min(area.width.saturating_sub(4));
    // states + blank line + keybind hint + top/bottom border
    let content_height = statuses.len() as u16 + 4;
    let popup_height = content_height.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let mut lines: Vec<Line> = statuses
        .iter()
        .enumerate()
        .map(|(i, status)| {
            let prefix = if i == app.status_picker.selected {
                "> "
            } else {
                "  "
            };
            let mut style = Style::default().fg(status_color(status));
            if i == app.status_picker.selected {
                style = style.add_modifier(Modifier::BOLD);
            }
            Line::from(Span::styled(format!("{}{}", prefix, status), style))
        })
        .collect();

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[j/k] [Enter] [Esc]",
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Status "),
    );
    f.render_widget(paragraph, popup_area);
}

pub fn draw_link_editor(f: &mut Frame, app: &App) {
    let area = f.area();
    let editor = &app.link_editor;

    let popup_width = 40u16.min(area.width.saturating_sub(4));
    let popup_height = 16u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let rel_label = app
        .rel_types
        .get(editor.rel_type_index)
        .map(|s| s.as_str())
        .unwrap_or("implements");

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("  Type: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("< {} >", rel_label),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("  Find: ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("{}_", editor.query)),
    ]));

    lines.push(Line::from(""));

    let max_results = (popup_height as usize).saturating_sub(6);
    for (i, path) in editor.results.iter().take(max_results).enumerate() {
        let label = app
            .store
            .get(path)
            .map(|d| format!("{}: {}", d.id.to_uppercase(), d.title))
            .unwrap_or_else(|| path.display().to_string());

        let prefix = if i == editor.selected { "> " } else { "  " };
        let style = if i == editor.selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, label),
            style,
        )));
    }

    if editor.results.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no matches)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Tab", Style::default().fg(Color::DarkGray)),
        Span::raw(" type  "),
        Span::styled("Enter", Style::default().fg(Color::DarkGray)),
        Span::raw(" link  "),
        Span::styled("Esc", Style::default().fg(Color::DarkGray)),
        Span::raw(" cancel"),
    ]));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Add Relation "),
    );
    f.render_widget(paragraph, popup_area);
}

pub fn draw_provenance_editor(f: &mut Frame, app: &App) {
    let area = f.area();
    let editor = &app.provenance_editor;

    let popup_width = 60u16.min(area.width.saturating_sub(4));
    let popup_height = 8u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let doc_id = app
        .store
        .get(&editor.doc_path)
        .map(|d| d.id.to_uppercase())
        .unwrap_or_else(|| {
            editor
                .doc_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string()
        });

    let title = format!(" Add Provenance \u{2014} {} ", doc_id);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Citation: ", Style::default().fg(Color::DarkGray)),
        Span::raw(editor.input.clone()),
        Span::styled(" ", Style::default().bg(Color::White)),
    ]));
    lines.push(Line::from(""));

    if let Some(ref err) = editor.error {
        lines.push(Line::from(Span::styled(
            format!("  {}", err),
            Style::default().fg(Color::Red),
        )));
    }

    lines.push(Line::from(Span::styled(
        "  Enter to add, Esc to cancel",
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(title),
    );
    f.render_widget(paragraph, popup_area);
}

/// The dialog row label for one agent action. Interactive templates carry a
/// visible `(interactive)` marker (AC2) so the user knows the selection hands the
/// terminal over to the configured command; headless templates and Custom do not.
/// Pure (no `app`/frame) so the marking is directly unit-testable.
#[cfg(feature = "agent")]
pub fn action_label(action: &crate::tui::state::forms::AgentAction) -> String {
    use crate::engine::prompt::RunMode;
    use crate::tui::state::forms::AgentAction;

    match action {
        AgentAction::Template(p) => {
            let marker = match p.mode {
                RunMode::Interactive => "  (interactive)",
                RunMode::Headless => "",
            };
            format!("  {} — {}{}", p.name, p.description, marker)
        }
        AgentAction::Custom => "  Custom prompt".to_string(),
    }
}

#[cfg(feature = "agent")]
pub fn draw_agent_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let dialog = &app.agent_dialog;

    if let Some(ref buffer) = dialog.text_input {
        let popup_width = (area.width * 50 / 100)
            .max(30)
            .min(area.width.saturating_sub(4));
        let popup_height = 6u16.min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(popup_width)) / 2;
        let y = (area.height.saturating_sub(popup_height)) / 2;
        let popup_area = Rect::new(x, y, popup_width, popup_height);

        f.render_widget(Clear, popup_area);

        let title = format!(" Custom Prompt — {} ", dialog.doc_title);
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  > ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{}_", buffer)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Enter", Style::default().fg(Color::DarkGray)),
                Span::raw(" submit  "),
                Span::styled("Esc", Style::default().fg(Color::DarkGray)),
                Span::raw(" back"),
            ]),
        ];

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan))
                .title(title),
        );
        f.render_widget(paragraph, popup_area);
        return;
    }

    let action_count = dialog.actions.len() as u16;
    let missing_lines = if dialog.missing.is_empty() { 0 } else { 1 };
    let content_height = action_count + missing_lines + 2;
    let popup_width = (area.width * 40 / 100)
        .max(20)
        .min(area.width.saturating_sub(4));
    let popup_height = content_height.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let mut items: Vec<ListItem> = dialog
        .actions
        .iter()
        .map(|action| ListItem::new(action_label(action)))
        .collect();

    // Render-only footer: the named-but-missing templates. It is NOT part of
    // `dialog.actions`, so Up/Down/Enter (which clamp to actions.len()) never land
    // on it.
    if !dialog.missing.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            format!("  ! missing templates: {}", dialog.missing.join(", ")),
            Style::default().fg(Color::DarkGray),
        ))));
    }

    let title = format!(" Agent Actions \u{2014} {} ", dialog.doc_title);

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan))
                .title(title),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = ListState::default().with_selected(Some(dialog.selected_index));
    f.render_stateful_widget(list, popup_area, &mut state);
}

pub fn draw_warnings_panel(f: &mut Frame, app: &App) {
    let area = f.area();
    let parse_errors = app.store.parse_errors();

    let total_count = app.total_warnings_count();

    let popup_width = 70.min(area.width.saturating_sub(4));
    let content_height = if total_count == 0 {
        match &app.fix_result {
            Some(output) => (output.lines().count() as u16).max(1) + 2,
            None => 3,
        }
    } else {
        (total_count as u16) * 2 + 2
    };
    let popup_height = (content_height + 2).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Warnings (f: fix, q/w/Esc: close) ");

    if total_count == 0 {
        let message = match &app.fix_result {
            Some(output) => output.clone(),
            None => "  No warnings".to_string(),
        };
        let lines: Vec<Line> = message
            .lines()
            .map(|l| {
                Line::from(Span::styled(
                    l.to_string(),
                    Style::default().fg(Color::DarkGray),
                ))
            })
            .collect();
        let msg = Paragraph::new(lines).block(block);
        f.render_widget(msg, popup_area);
        return;
    }

    let mut items: Vec<ListItem> = parse_errors
        .iter()
        .map(|err| {
            let lines = vec![
                Line::from(Span::styled(
                    format!("  {}", err.path.display()),
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(Span::styled(
                    format!("    {}", err.error),
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            ListItem::new(lines)
        })
        .collect();

    for msg in &app.validation_errors {
        let lines = vec![
            Line::from(Span::styled(
                format!("  error: {}", msg),
                Style::default().fg(Color::Red),
            )),
            Line::from(Span::styled(
                "    validation error".to_string(),
                Style::default().fg(Color::DarkGray),
            )),
        ];
        items.push(ListItem::new(lines));
    }

    for msg in &app.validation_warnings {
        let lines = vec![
            Line::from(Span::styled(
                format!("  warn: {}", msg),
                Style::default().fg(Color::Yellow),
            )),
            Line::from(Span::styled(
                "    validation warning".to_string(),
                Style::default().fg(Color::DarkGray),
            )),
        ];
        items.push(ListItem::new(lines));
    }

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default().with_selected(Some(app.warnings_selected));
    f.render_stateful_widget(list, popup_area, &mut state);
}

pub fn draw_search_overlay(f: &mut Frame, app: &App) {
    let area = f.area();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let input = Paragraph::new(Line::from(vec![
        Span::styled(" / ", Style::default().fg(Color::Cyan)),
        Span::raw(&app.search_query),
        Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Search ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(input, layout[0]);

    let items: Vec<ListItem> = app
        .search_results
        .iter()
        .map(|path| {
            let doc = app.store.get(path);
            let (title, status_str, status_clr) = match doc {
                Some(d) => (
                    d.title.as_str(),
                    format!("{}", d.status),
                    status_color(&d.status),
                ),
                None => ("?", "?".to_string(), Color::White),
            };
            let gutter_span = match app.git_status_cache.get(path) {
                Some(GitFileStatus::New) => Span::styled("┃", Style::default().fg(Color::Green)),
                Some(GitFileStatus::Modified) => {
                    Span::styled("┃", Style::default().fg(Color::Yellow))
                }
                None => Span::raw(" "),
            };
            let line = Line::from(vec![
                gutter_span,
                Span::raw(format!("  {:<40} ", title)),
                Span::styled(status_str, Style::default().fg(status_clr)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Results ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut state = ListState::default().with_selected(Some(app.search_selected));
    f.render_stateful_widget(list, layout[1], &mut state);
}

pub fn draw_gh_conflict(f: &mut Frame, app: &App) {
    let area = f.area();
    let message = match &app.gh_conflict_message {
        Some(m) => m.as_str(),
        None => return,
    };

    let popup_width = 55.min(area.width.saturating_sub(4));
    let popup_height = 8.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", message),
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Wait for background sync or restart TUI.",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "         [Esc: dismiss]",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Conflict "),
    );
    f.render_widget(paragraph, popup_area);
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
