use ratatui::style::Color;

use crate::engine::document::Status;
use crate::engine::status_colors::StatusColors;

pub fn hex_to_color(hex: &str) -> Option<Color> {
    let digits = hex.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&digits[0..2], 16).ok()?;
    let g = u8::from_str_radix(&digits[2..4], 16).ok()?;
    let b = u8::from_str_radix(&digits[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

pub fn status_color(colors: &StatusColors, type_name: &str, status: &Status) -> Color {
    if let Some(color) = colors
        .get(type_name, status.as_str())
        .and_then(hex_to_color)
    {
        return color;
    }
    match status.as_str() {
        "draft" => Color::Yellow,
        "review" => Color::Blue,
        "accepted" => Color::Green,
        "in-progress" => Color::Cyan,
        "complete" => Color::Green,
        "rejected" => Color::Red,
        "superseded" => Color::DarkGray,
        _ => Color::Reset,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn hex_to_color_parses_rrggbb() {
        assert_eq!(hex_to_color("#ff0000"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(hex_to_color("#336699"), Some(Color::Rgb(0x33, 0x66, 0x99)));
    }

    #[test]
    fn hex_to_color_rejects_garbage() {
        assert_eq!(hex_to_color("#xyzxyz"), None);
        assert_eq!(hex_to_color("zzz"), None);
        assert_eq!(hex_to_color(""), None);
        assert_eq!(hex_to_color("#ff00"), None);
        assert_eq!(hex_to_color("ff0000"), None);
        assert_eq!(hex_to_color("#ff000000"), None);
    }

    #[test]
    fn status_color_uses_cached_hex_on_hit() {
        let mut colors = StatusColors::default();
        colors.set_type(
            "clickup-tasks",
            HashMap::from([("pending".to_string(), "#336699".to_string())]),
        );
        assert_eq!(
            status_color(&colors, "clickup-tasks", &Status::new("pending")),
            Color::Rgb(0x33, 0x66, 0x99)
        );
    }

    #[test]
    fn status_color_falls_back_to_name_match_on_miss() {
        let colors = StatusColors::default();
        assert_eq!(
            status_color(&colors, "story", &Status::new("draft")),
            Color::Yellow
        );
        assert_eq!(
            status_color(&colors, "story", &Status::new("unknown-status")),
            Color::Reset
        );
    }

    #[test]
    fn status_color_falls_back_on_garbage_hex() {
        let mut colors = StatusColors::default();
        colors.set_type(
            "story",
            HashMap::from([("draft".to_string(), "#zzzzzz".to_string())]),
        );
        assert_eq!(
            status_color(&colors, "story", &Status::new("draft")),
            Color::Yellow
        );
    }
}
