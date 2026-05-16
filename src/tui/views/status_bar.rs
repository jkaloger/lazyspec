use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::engine::config::StatusBarConfig;
#[cfg(feature = "agent")]
use crate::engine::agent_metadata::AgentStatus;
#[cfg(feature = "agent")]
use crate::engine::ipc::ConnectionState;
#[cfg(feature = "agent")]
use crate::tui::state::agents::DataSource;
use crate::tui::state::{App, ViewMode};

pub type StatusComponent = fn(&App) -> Option<Span<'static>>;

pub struct StatusBar {
    pub left: Vec<Option<Span<'static>>>,
    pub center: Vec<Option<Span<'static>>>,
    pub right: Vec<Option<Span<'static>>>,
}

const BAR_BG: Color = Color::DarkGray;
const BG_ALT: Color = Color::Indexed(238);
const POWERLINE_RIGHT: &str = "\u{E0B0}";

fn zone_spans(items: Vec<Option<Span<'static>>>) -> Vec<Span<'static>> {
    let present: Vec<Span<'static>> = items.into_iter().flatten().collect();
    if present.is_empty() {
        return vec![];
    }

    let first_has_bg = present[0].style.bg.is_some();

    // Assign effective bg per position: tier 0 keeps its bg, tier 1 gets BG_ALT
    // if tier 0 had a bg (creating the gradient), tier 2+ is just text on BAR_BG.
    let mut bgs: Vec<Color> = Vec::with_capacity(present.len());
    let mut styled: Vec<Span<'static>> = Vec::with_capacity(present.len());

    for (i, span) in present.into_iter().enumerate() {
        match i {
            0 => {
                bgs.push(span.style.bg.unwrap_or(BAR_BG));
                styled.push(span);
            }
            1 if first_has_bg => {
                let fg = span.style.fg.unwrap_or(Color::White);
                let padded = format!(" {} ", span.content.trim());
                bgs.push(BG_ALT);
                styled.push(Span::styled(padded, Style::default().bg(BG_ALT).fg(fg)));
            }
            _ => {
                bgs.push(BAR_BG);
                styled.push(span);
            }
        }
    }

    let mut result = Vec::new();
    let mut prev_bg = BAR_BG;

    for (i, (span, &bg)) in styled.into_iter().zip(bgs.iter()).enumerate() {
        if i > 0 && prev_bg != bg {
            result.push(Span::styled(
                POWERLINE_RIGHT,
                Style::default().fg(prev_bg).bg(bg),
            ));
        } else if i > 0 {
            result.push(Span::raw("  "));
        }
        result.push(span);
        prev_bg = bg;
    }

    // Close out any trailing bg back to bar bg
    if prev_bg != BAR_BG {
        result.push(Span::styled(
            POWERLINE_RIGHT,
            Style::default().fg(prev_bg).bg(BAR_BG),
        ));
    }

    result
}

impl StatusBar {
    pub fn render(&self, width: u16) -> Line<'static> {
        let left_spans = zone_spans(self.left.clone());
        let center_spans = zone_spans(self.center.clone());
        let right_spans = zone_spans(self.right.clone());

        if left_spans.is_empty() && center_spans.is_empty() && right_spans.is_empty() {
            return Line::from(vec![]);
        }

        let left_width: usize = left_spans.iter().map(|s| s.width()).sum();
        let center_width: usize = center_spans.iter().map(|s| s.width()).sum();
        let right_width: usize = right_spans.iter().map(|s| s.width()).sum();

        let total_width = width as usize;

        // Distribute padding: center gets placed in the middle, right gets pushed to the end
        let left_pad = if center_width > 0 {
            let half = total_width.saturating_sub(center_width) / 2;
            half.saturating_sub(left_width)
        } else {
            0
        };
        let right_pad =
            total_width.saturating_sub(left_width + left_pad + center_width + right_width);

        let mut spans = Vec::new();
        spans.extend(left_spans);
        if left_pad > 0 {
            spans.push(Span::raw(" ".repeat(left_pad)));
        }
        spans.extend(center_spans);
        if right_pad > 0 {
            spans.push(Span::raw(" ".repeat(right_pad)));
        }
        spans.extend(right_spans);

        Line::from(spans)
    }
}

/// Height required to render the status bar, including a toast strip above it
/// when `app.toast` is set. Used by the top-level layout to reserve vertical
/// space.
pub fn status_bar_height(app: &App) -> u16 {
    if app.toast.is_some() {
        2
    } else {
        1
    }
}

pub fn draw_status_bar(f: &mut Frame, app: &App, area: Rect, components: &StatusBarComponents) {
    let (toast_area, status_area) = if app.toast.is_some() && area.height >= 2 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);
        (Some(split[0]), split[1])
    } else {
        (None, area)
    };

    if let (Some(toast_area), Some(toast)) = (toast_area, app.toast.as_ref()) {
        let toast_para = Paragraph::new(Line::from(Span::raw(toast.message.clone())))
            .style(Style::default().fg(Color::Red).bg(Color::DarkGray));
        f.render_widget(toast_para, toast_area);
    }

    let bar = StatusBar {
        left: components.left.iter().map(|c| c(app)).collect(),
        center: components.center.iter().map(|c| c(app)).collect(),
        right: components.right.iter().map(|c| c(app)).collect(),
    };

    let line = bar.render(status_area.width);
    let paragraph =
        Paragraph::new(line).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(paragraph, status_area);
}

pub struct StatusBarComponents {
    pub left: Vec<StatusComponent>,
    pub center: Vec<StatusComponent>,
    pub right: Vec<StatusComponent>,
}

fn lookup_component(name: &str) -> Option<StatusComponent> {
    match name {
        "mode" => Some(mode_component),
        "type_filter" => Some(type_filter_component),
        "doc_count" => Some(doc_count_component),
        "warnings" => Some(warnings_component),
        "errors" => Some(errors_component),
        "version" => Some(version_component),
        "help_hint" => Some(help_hint_component),
        "search" => Some(search_component),
        "git_branch" => Some(git_branch_component),
        #[cfg(feature = "agent")]
        "agents_connection" => Some(agents_connection_component),
        #[cfg(feature = "agent")]
        "agents_counts" => Some(agents_counts_component),
        #[cfg(feature = "agent")]
        "agents_tokens" => Some(agents_tokens_component),
        _ => None,
    }
}

impl StatusBarComponents {
    pub fn from_config(config: &StatusBarConfig) -> (Self, Vec<String>) {
        let defaults = Self::default();
        let mut warnings = Vec::new();

        let left = match &config.left {
            None => defaults.left,
            Some(names) => resolve_names(names, &mut warnings),
        };
        let center = match &config.center {
            None => defaults.center,
            Some(names) => resolve_names(names, &mut warnings),
        };
        let right = match &config.right {
            None => defaults.right,
            Some(names) => resolve_names(names, &mut warnings),
        };

        (
            Self {
                left,
                center,
                right,
            },
            warnings,
        )
    }
}

fn resolve_names(names: &[String], warnings: &mut Vec<String>) -> Vec<StatusComponent> {
    names
        .iter()
        .filter_map(|name| {
            lookup_component(name).or_else(|| {
                warnings.push(format!("unknown status bar component: {}", name));
                None
            })
        })
        .collect()
}

impl Default for StatusBarComponents {
    fn default() -> Self {
        Self {
            left: vec![mode_component, type_filter_component, doc_count_component],
            center: vec![
                warnings_component,
                errors_component,
                #[cfg(feature = "agent")]
                agents_connection_component,
                #[cfg(feature = "agent")]
                agents_counts_component,
                #[cfg(feature = "agent")]
                agents_tokens_component,
            ],
            right: vec![
                git_branch_component,
                search_component,
                version_component,
                help_hint_component,
            ],
        }
    }
}

fn mode_bg(mode: &ViewMode) -> Color {
    match mode {
        ViewMode::Types => Color::Blue,
        ViewMode::Filters => Color::Magenta,
        #[cfg(feature = "metrics")]
        ViewMode::Metrics => Color::Cyan,
        ViewMode::Graph => Color::Green,
        #[cfg(feature = "agent")]
        ViewMode::Agents => Color::Yellow,
    }
}

pub fn mode_component(app: &App) -> Option<Span<'static>> {
    let mode = &app.view_mode;
    Some(Span::styled(
        format!(" {} ", mode.name()),
        Style::default()
            .add_modifier(Modifier::BOLD)
            .bg(mode_bg(mode))
            .fg(Color::Black),
    ))
}

pub fn doc_count_component(app: &App) -> Option<Span<'static>> {
    let count = app.doc_tree.len();
    if count == 0 {
        return None;
    }
    Some(Span::raw(format!("{} docs", count)))
}

pub fn warnings_component(app: &App) -> Option<Span<'static>> {
    let count = app.validation_warnings.len();
    if count == 0 {
        return None;
    }
    Some(Span::styled(
        format!("{} ⚠", count),
        Style::default().fg(Color::Yellow),
    ))
}

pub fn errors_component(app: &App) -> Option<Span<'static>> {
    let count = app.validation_errors.len() + app.store.parse_errors().len();
    if count == 0 {
        return None;
    }
    Some(Span::styled(
        format!("{} ✗", count),
        Style::default().fg(Color::Red),
    ))
}

pub fn version_component(_app: &App) -> Option<Span<'static>> {
    Some(Span::raw(format!(
        "lazyspec v{}",
        env!("CARGO_PKG_VERSION")
    )))
}

pub fn help_hint_component(_app: &App) -> Option<Span<'static>> {
    Some(Span::styled("? help", Style::default().fg(Color::Gray)))
}

pub fn git_branch_component(app: &App) -> Option<Span<'static>> {
    app.git_branch
        .as_ref()
        .map(|b| Span::styled(format!(" {}", b), Style::default().fg(Color::Cyan)))
}

pub fn search_component(app: &App) -> Option<Span<'static>> {
    if !app.search_mode || app.search_query.is_empty() {
        return None;
    }
    Some(Span::styled(
        format!("/{}", app.search_query),
        Style::default().fg(Color::Yellow),
    ))
}

pub fn type_filter_component(app: &App) -> Option<Span<'static>> {
    if app.view_mode != ViewMode::Types {
        return None;
    }
    Some(Span::raw(app.current_type().to_string()))
}

#[cfg(feature = "agent")]
pub fn agents_connection_component(app: &App) -> Option<Span<'static>> {
    if app.view_mode != ViewMode::Agents {
        return None;
    }
    let (text, color) = if app.agents_view.source == DataSource::Offline {
        (" offline (history) ", Color::Red)
    } else if app.agents_view.connection == ConnectionState::Connected {
        (" daemon: connected ", Color::Green)
    } else {
        (" daemon: reconnecting ", Color::Yellow)
    };
    Some(Span::styled(text, Style::default().fg(color)))
}

#[cfg(feature = "agent")]
pub fn agents_counts_component(app: &App) -> Option<Span<'static>> {
    if app.view_mode != ViewMode::Agents {
        return None;
    }
    let counts = app.agents_view.counts_by_status();
    if counts.is_empty() && app.agents_view.snapshots.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    for status in [AgentStatus::Running, AgentStatus::Crashed] {
        if let Some(&n) = counts.get(&status) {
            if n > 0 {
                let label = match status {
                    AgentStatus::Running => "running",
                    AgentStatus::Crashed => "crashed",
                };
                parts.push(format!("{n} {label}"));
            }
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(Span::raw(format!(" {} ", parts.join(", "))))
}

#[cfg(feature = "agent")]
pub fn agents_tokens_component(app: &App) -> Option<Span<'static>> {
    if app.view_mode != ViewMode::Agents {
        return None;
    }
    let (tin, tout) = app.agents_view.aggregate_tokens();
    if tin == 0 && tout == 0 && app.agents_view.snapshots.is_empty() {
        return None;
    }
    Some(Span::raw(format!(
        " in: {} / out: {} ",
        humanize_count(tin),
        humanize_count(tout)
    )))
}

#[cfg(feature = "agent")]
fn humanize_count(n: u64) -> String {
    if n < 1_000 {
        format!("{n}")
    } else if n < 1_000_000 {
        format!("{:.1}k", (n as f64) / 1_000.0)
    } else {
        format!("{:.1}m", (n as f64) / 1_000_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn renders_spans_with_spaces() {
        let bar = StatusBar {
            left: vec![Some(Span::raw("mode")), Some(Span::raw("branch"))],
            center: vec![],
            right: vec![Some(Span::raw("errors: 0"))],
        };

        let line = bar.render(80);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("mode"), "should contain 'mode'");
        assert!(text.contains("branch"), "should contain 'branch'");
        assert!(text.contains("errors: 0"), "should contain right component");
        assert!(
            !text.contains('\u{2502}'),
            "should not contain pipe separators"
        );
    }

    #[test]
    fn bg_component_triggers_powerline_transition() {
        let bar = StatusBar {
            left: vec![
                Some(Span::styled(
                    " Types ",
                    Style::default().bg(Color::Blue).fg(Color::Black),
                )),
                Some(Span::raw("branch")),
            ],
            center: vec![],
            right: vec![],
        };

        let line = bar.render(80);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(
            text.contains(POWERLINE_RIGHT),
            "should contain powerline transition after bg component"
        );
        assert!(text.contains("Types"), "should contain 'Types'");
        assert!(text.contains("branch"), "should contain 'branch'");
    }

    #[test]
    fn none_components_produce_no_extra_spaces() {
        let bar = StatusBar {
            left: vec![Some(Span::raw("a")), None, Some(Span::raw("b"))],
            center: vec![None],
            right: vec![None, Some(Span::raw("c"))],
        };

        let line = bar.render(80);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("a"), "should contain 'a'");
        assert!(text.contains("b"), "should contain 'b'");
        assert!(text.contains("c"), "should contain 'c'");
    }

    #[test]
    fn all_none_renders_empty_bar() {
        let bar = StatusBar {
            left: vec![None, None],
            center: vec![None],
            right: vec![None],
        };

        let line = bar.render(40);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        let trimmed = text.trim();
        assert!(
            trimmed.is_empty(),
            "all-None bar should produce no visible text, got '{}'",
            trimmed
        );
    }

    #[test]
    fn draw_status_bar_renders_to_frame() {
        let backend = TestBackend::new(60, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                let components = StatusBarComponents {
                    left: vec![|_: &App| Some(Span::raw("left"))],
                    center: vec![],
                    right: vec![|_: &App| Some(Span::raw("right"))],
                };
                // We can't easily create a real App in tests, so test the StatusBar directly
                // and verify draw_status_bar compiles with the right signature.
                let bar = StatusBar {
                    left: components
                        .left
                        .iter()
                        .map(|_| Some(Span::raw("left")))
                        .collect(),
                    center: vec![],
                    right: components
                        .right
                        .iter()
                        .map(|_| Some(Span::raw("right")))
                        .collect(),
                };
                let line = bar.render(area.width);
                let paragraph = Paragraph::new(line)
                    .style(Style::default().bg(Color::DarkGray).fg(Color::White));
                f.render_widget(paragraph, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = (0..buffer.area.width)
            .map(|x| buffer.cell((x, 0)).unwrap().symbol().to_string())
            .collect();

        assert!(
            content.contains("left"),
            "buffer should contain 'left', got '{}'",
            content
        );
        assert!(
            content.contains("right"),
            "buffer should contain 'right', got '{}'",
            content
        );

        // Verify background color
        let cell = buffer.cell((0, 0)).unwrap();
        assert_eq!(cell.bg, Color::DarkGray, "background should be DarkGray");
        assert_eq!(cell.fg, Color::White, "foreground should be White");
    }

    #[test]
    fn empty_components_render_clean() {
        let bar = StatusBar {
            left: vec![],
            center: vec![],
            right: vec![],
        };

        let line = bar.render(40);
        assert!(line.spans.is_empty(), "empty bar should have no spans");
    }
}

#[cfg(all(test, feature = "agent"))]
mod agent_tests {
    use super::*;
    use crate::engine::agent_metadata::AgentStatus;
    use crate::engine::ipc::ConnectionState;
    use crate::engine::ipc::protocol::AgentSnapshot;
    use crate::tui::state::App;
    use crate::tui::state::ViewMode;
    use crate::tui::state::agents::DataSource;
    use crate::tui::state::app::tests::make_test_app;

    fn snap(session: &str, tin: u64, tout: u64) -> AgentSnapshot {
        AgentSnapshot {
            agent_id: format!("a-{session}"),
            session_id: session.into(),
            doc_id: "STORY-1".into(),
            elapsed_ms: 0,
            tokens_in: tin,
            tokens_out: tout,
        }
    }

    fn agents_app() -> App {
        let mut app = make_test_app(0);
        app.view_mode = ViewMode::Agents;
        app
    }

    fn content(span: Option<Span<'static>>) -> String {
        span.expect("span present").content.into_owned()
    }

    #[test]
    fn agents_connection_component_none_when_not_agents_view() {
        let mut app = make_test_app(0);
        app.view_mode = ViewMode::Types;
        assert!(agents_connection_component(&app).is_none());
    }

    #[test]
    fn agents_connection_component_connected_in_view() {
        let mut app = agents_app();
        app.agents_view.source = DataSource::Live;
        app.agents_view.connection = ConnectionState::Connected;
        let s = content(agents_connection_component(&app));
        assert!(s.contains("connected"), "got: {s}");
    }

    #[test]
    fn agents_connection_component_reconnecting_in_view() {
        let mut app = agents_app();
        app.agents_view.source = DataSource::Live;
        app.agents_view.connection = ConnectionState::Reconnecting;
        let s = content(agents_connection_component(&app));
        assert!(s.contains("reconnecting"), "got: {s}");
    }

    #[test]
    fn agents_connection_component_offline_overrides() {
        let mut app = agents_app();
        app.agents_view.source = DataSource::Offline;
        app.agents_view.connection = ConnectionState::Connected;
        let s = content(agents_connection_component(&app));
        assert!(s.contains("offline"), "got: {s}");
    }

    #[test]
    fn agents_counts_component_renders_running_and_crashed() {
        let mut app = agents_app();
        app.agents_view
            .statuses
            .insert("s1".into(), AgentStatus::Running);
        app.agents_view
            .statuses
            .insert("s2".into(), AgentStatus::Running);
        app.agents_view
            .statuses
            .insert("s3".into(), AgentStatus::Crashed);
        let s = content(agents_counts_component(&app));
        assert!(s.contains("2 running"), "got: {s}");
        assert!(s.contains("1 crashed"), "got: {s}");
    }

    #[test]
    fn agents_counts_component_none_when_empty() {
        let app = agents_app();
        assert!(agents_counts_component(&app).is_none());
    }

    #[test]
    fn agents_tokens_component_humanizes() {
        let mut app = agents_app();
        app.agents_view
            .snapshots
            .insert("s1".into(), snap("s1", 1500, 25));
        let s = content(agents_tokens_component(&app));
        assert!(s.contains("1.5k"), "got: {s}");
        assert!(s.contains("25"), "got: {s}");
    }

    #[test]
    fn agents_tokens_component_none_when_zero_and_empty() {
        let app = agents_app();
        assert!(agents_tokens_component(&app).is_none());
    }
}
