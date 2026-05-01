use ratatui::style::{Color, Modifier, Style};

use crate::engine::document::Status;
use crate::engine::sequencing::EdgeKind;

pub fn status_color(status: &Status) -> Color {
    match status {
        Status::Draft => Color::Yellow,
        Status::Review => Color::Blue,
        Status::Accepted => Color::Green,
        Status::InProgress => Color::Cyan,
        Status::Complete => Color::Green,
        Status::Rejected => Color::Red,
        Status::Superseded => Color::DarkGray,
    }
}

pub fn tag_color(tag: &str) -> Color {
    const PALETTE: &[Color] = &[
        Color::Magenta,
        Color::Cyan,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Red,
        Color::LightMagenta,
        Color::LightCyan,
        Color::LightGreen,
        Color::LightBlue,
    ];
    let hash = tag
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    PALETTE[(hash as usize) % PALETTE.len()]
}

/// Colour applied to a sequencing-graph node label, derived from doc status.
///
/// AC2: every `Status` variant maps to a distinct foreground colour. This
/// differs from `status_color` (which intentionally collapses Accepted /
/// Complete to the same green) because the sequencing graph wants every
/// status visually distinguishable.
pub fn node_color(status: &Status) -> Color {
    match status {
        Status::Draft => Color::Yellow,
        Status::Review => Color::Blue,
        Status::Accepted => Color::LightGreen,
        Status::InProgress => Color::Cyan,
        Status::Complete => Color::Green,
        Status::Rejected => Color::Red,
        Status::Superseded => Color::DarkGray,
    }
}

/// Style for a sequencing-graph node label.
pub fn node_style(status: &Status) -> Style {
    Style::default().fg(node_color(status))
}

/// Glyph + style for a sequencing-graph edge, distinguishing `Blocks` from
/// `Implements` (AC1). Blocks edges render as solid arrows in red; implements
/// edges as dashed arrows in magenta.
pub struct EdgeRender {
    pub glyph: &'static str,
    pub style: Style,
}

pub fn edge_render(kind: EdgeKind) -> EdgeRender {
    match kind {
        EdgeKind::Blocks => EdgeRender {
            glyph: "──▶",
            style: Style::default().fg(Color::Red),
        },
        EdgeKind::Implements => EdgeRender {
            glyph: "╌╌▷",
            style: Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::DIM),
        },
    }
}

/// Convenience accessor for just the style of an edge render. Used by AC1
/// helper-style tests; production callers use [`edge_render`] which returns
/// the glyph alongside the style.
#[cfg(test)]
pub fn edge_style(kind: EdgeKind) -> Style {
    edge_render(kind).style
}

/// Wrap a base style for an out-of-scope node: switches the foreground to
/// `DarkGray` and adds `Modifier::DIM`. Pass-through when `dimmed` is false.
pub fn dimmed(style: Style, dimmed: bool) -> Style {
    if dimmed {
        style.fg(Color::DarkGray).add_modifier(Modifier::DIM)
    } else {
        style
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ac1_blocks_and_implements_edges_have_distinct_styles_and_glyphs() {
        let blocks = edge_render(EdgeKind::Blocks);
        let implements = edge_render(EdgeKind::Implements);

        assert_ne!(
            blocks.style, implements.style,
            "blocks and implements edges must render with distinct styles"
        );
        assert_ne!(
            blocks.glyph, implements.glyph,
            "blocks and implements edges must use distinct glyphs"
        );
    }

    #[test]
    fn ac1_edge_style_helper_returns_distinct_styles_per_kind() {
        assert_ne!(edge_style(EdgeKind::Blocks), edge_style(EdgeKind::Implements));
    }

    #[test]
    fn ac2_every_status_variant_maps_to_distinct_node_colour() {
        let variants = [
            Status::Draft,
            Status::Review,
            Status::Accepted,
            Status::InProgress,
            Status::Complete,
            Status::Rejected,
            Status::Superseded,
        ];

        let colours: HashSet<Color> = variants.iter().map(node_color).collect();

        assert_eq!(
            colours.len(),
            variants.len(),
            "expected one distinct colour per Status variant; got {:?}",
            colours
        );
    }

    #[test]
    fn ac2_node_style_returns_status_coloured_foreground() {
        let s = node_style(&Status::Draft);
        assert_eq!(s.fg, Some(Color::Yellow));
    }

    #[test]
    fn dimmed_true_switches_fg_to_dark_gray_and_adds_dim_modifier() {
        let base = Style::default().fg(Color::Yellow);
        let out = dimmed(base, true);
        assert_eq!(out.fg, Some(Color::DarkGray));
        assert!(out.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn dimmed_false_returns_base_style_untouched() {
        let base = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
        let out = dimmed(base, false);
        assert_eq!(out, base);
    }
}
