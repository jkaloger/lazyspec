use std::path::Path;

use console::{colors_enabled, Style};

use crate::engine::document::{DocType, Status};
use crate::engine::status_colors::StatusColors;

/// Map `#rrggbb` to the nearest ANSI-256 index (console 0.15 has no truecolor
/// variant). Considers both the 6x6x6 cube (16-231) and the grayscale ramp
/// (232-255), picking whichever is closer by squared RGB distance.
pub fn hex_to_ansi256(hex: &str) -> Option<u8> {
    let digits = hex.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    let (r, g, b) = (channel(0)?, channel(2)?, channel(4)?);

    let d2 = |a: u8, b: u8| {
        let d = i32::from(a) - i32::from(b);
        d * d
    };

    const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let nearest_cube = |v: u8| {
        let mut best = (d2(v, CUBE_LEVELS[0]), 0usize);
        for (i, &level) in CUBE_LEVELS.iter().enumerate().skip(1) {
            let d = d2(v, level);
            if d < best.0 {
                best = (d, i);
            }
        }
        best
    };
    let (rd, ri) = nearest_cube(r);
    let (gd, gi) = nearest_cube(g);
    let (bd, bi) = nearest_cube(b);
    let cube_index = 16 + 36 * ri + 6 * gi + bi;
    let cube_dist = rd + gd + bd;

    let mut gray = (i32::MAX, 0usize);
    for k in 0..24usize {
        let level = (8 + 10 * k) as u8;
        let d = d2(r, level) + d2(g, level) + d2(b, level);
        if d < gray.0 {
            gray = (d, k);
        }
    }

    if cube_dist <= gray.0 {
        Some(cube_index as u8)
    } else {
        Some((232 + gray.1) as u8)
    }
}

pub fn status_style(colors: &StatusColors, type_name: &str, status: &Status) -> Style {
    if let Some(index) = colors
        .get(type_name, status.as_str())
        .and_then(hex_to_ansi256)
    {
        return Style::new().color256(index);
    }
    let style = Style::new();
    match status.as_str() {
        "accepted" => style.green(),
        "draft" => style.yellow(),
        "review" => style.blue(),
        "in-progress" => style.cyan(),
        "complete" => style.green(),
        "rejected" => style.red(),
        "superseded" => style.color256(8),
        _ => style,
    }
}

pub fn styled_status(colors: &StatusColors, type_name: &str, status: &Status) -> String {
    status_style(colors, type_name, status)
        .apply_to(status)
        .to_string()
}

pub fn dim(text: &str) -> String {
    Style::new().dim().apply_to(text).to_string()
}

pub fn bold(text: &str) -> String {
    Style::new().bold().apply_to(text).to_string()
}

pub fn type_header(doc_type: &DocType) -> String {
    let label = doc_type.to_string();
    if colors_enabled() {
        let width = 25usize.saturating_sub(label.len() + 3);
        format!(
            "\u{256d}\u{2500} {} {}\u{256e}",
            label,
            "\u{2500}".repeat(width)
        )
    } else {
        format!("--- {} ---", label)
    }
}

pub fn doc_card(
    colors: &StatusColors,
    title: &str,
    doc_type: &DocType,
    status: &Status,
    assignee: Option<&str>,
    path: &Path,
) -> String {
    let path_str = path.display().to_string();
    let assignee_str = match assignee {
        Some(a) => format!(" {}", dim(&format!("@{}", a))),
        None => String::new(),
    };
    format!(
        "{} {} [{}]{} {}",
        bold(&format!("[{}]", doc_type)),
        bold(title),
        styled_status(colors, doc_type.as_str(), status),
        assignee_str,
        dim(&path_str),
    )
}

pub fn separator() -> String {
    if colors_enabled() {
        "\u{2500}".repeat(40)
    } else {
        "-".repeat(40)
    }
}

pub fn error_prefix() -> String {
    if colors_enabled() {
        Style::new().red().bold().apply_to("\u{2717}").to_string()
    } else {
        "error:".to_string()
    }
}

pub fn warning_prefix() -> String {
    if colors_enabled() {
        Style::new().yellow().bold().apply_to("!").to_string()
    } else {
        "warning:".to_string()
    }
}

/// A section divider for wizard flow transitions and DAG-summary headers. Bold +
/// underlined when colours are on; the bare text otherwise, so callers that embed
/// it in a returned string keep byte-for-byte parity with the plain form.
pub fn section_header(text: &str) -> String {
    if colors_enabled() {
        Style::new().bold().underlined().apply_to(text).to_string()
    } else {
        text.to_string()
    }
}

/// A success cue, mirroring `error_prefix`/`warning_prefix` but returning the
/// whole line: a green check prefix when colours are on, the bare text otherwise.
pub fn success_line(text: &str) -> String {
    if colors_enabled() {
        format!(
            "{} {}",
            Style::new().green().bold().apply_to("\u{2713}"),
            text
        )
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn colors_with(type_name: &str, status: &str, hex: &str) -> StatusColors {
        let mut colors = StatusColors::default();
        colors.set_type(
            type_name,
            HashMap::from([(status.to_string(), hex.to_string())]),
        );
        colors
    }

    #[test]
    fn hex_to_ansi256_maps_cube_exact_hits() {
        assert_eq!(hex_to_ansi256("#000000"), Some(16));
        assert_eq!(hex_to_ansi256("#ffffff"), Some(231));
        assert_eq!(hex_to_ansi256("#ff0000"), Some(196));
    }

    #[test]
    fn hex_to_ansi256_prefers_grayscale_ramp_for_mid_gray() {
        // 0x80 = 128; ramp level 8 + 10*12 = 128 exact (index 244), while the
        // nearest cube gray is 135,135,135 at squared distance 147.
        assert_eq!(hex_to_ansi256("#808080"), Some(244));
    }

    #[test]
    fn hex_to_ansi256_rejects_garbage() {
        assert_eq!(hex_to_ansi256("#zzzzzz"), None);
        assert_eq!(hex_to_ansi256(""), None);
        assert_eq!(hex_to_ansi256("ff0000"), None);
        assert_eq!(hex_to_ansi256("#ff00"), None);
        assert_eq!(hex_to_ansi256("#ff00000"), None);
    }

    #[test]
    fn status_style_uses_cache_hit() {
        let colors = colors_with("clickup-tasks", "pending", "#ff0000");
        let style = status_style(&colors, "clickup-tasks", &Status::new("pending"));
        assert_eq!(style, Style::new().color256(196));
    }

    #[test]
    fn status_style_falls_back_to_name_match_on_empty_cache() {
        let colors = StatusColors::default();
        let style = status_style(&colors, "story", &Status::new("draft"));
        assert_eq!(style, Style::new().yellow());
    }

    #[test]
    fn status_style_falls_back_on_garbage_hex() {
        let colors = colors_with("story", "draft", "#zzzzzz");
        let style = status_style(&colors, "story", &Status::new("draft"));
        assert_eq!(style, Style::new().yellow());
    }

    #[test]
    fn status_style_ignores_hit_for_other_type() {
        let colors = colors_with("clickup-tasks", "draft", "#ff0000");
        let style = status_style(&colors, "story", &Status::new("draft"));
        assert_eq!(style, Style::new().yellow());
    }

    // AC5 (CLI list): an assigned doc surfaces its assignee in the list card.
    #[test]
    fn doc_card_shows_assignee_when_set() {
        let card = doc_card(
            &StatusColors::default(),
            "My Story",
            &DocType::new("story"),
            &Status::new("draft"),
            Some("alice"),
            Path::new("docs/stories/STORY-001.md"),
        );
        assert!(card.contains("@alice"), "got: {card}");
    }

    // AC5 (CLI list): an unassigned doc renders no assignee marker.
    #[test]
    fn doc_card_omits_assignee_when_none() {
        let card = doc_card(
            &StatusColors::default(),
            "My Story",
            &DocType::new("story"),
            &Status::new("draft"),
            None,
            Path::new("docs/stories/STORY-001.md"),
        );
        assert!(!card.contains('@'), "got: {card}");
    }
}
