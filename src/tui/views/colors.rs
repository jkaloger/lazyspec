use std::collections::BTreeMap;

use ratatui::style::{Color, Modifier, Style};

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

/// Parse a user-supplied colour value: a `#rrggbb` hex string or a named ANSI
/// colour (case-insensitive). Never yields `Color::Reset`; invalid values
/// return `None` so callers fall through the resolution order.
fn parse_color(value: &str) -> Option<Color> {
    if let Some(c) = hex_to_color(value) {
        return Some(c);
    }
    match value.to_ascii_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" => Some(Color::DarkGray),
        "lightred" => Some(Color::LightRed),
        "lightgreen" => Some(Color::LightGreen),
        "lightyellow" => Some(Color::LightYellow),
        "lightblue" => Some(Color::LightBlue),
        "lightmagenta" => Some(Color::LightMagenta),
        "lightcyan" => Some(Color::LightCyan),
        "white" => Some(Color::White),
        _ => None,
    }
}

/// Built-in colour for the seven core lifecycle statuses. `None` for anything
/// else so unknown statuses fall through to the hash palette.
fn builtin_status_color(name: &str) -> Option<Color> {
    match name {
        "draft" => Some(Color::Yellow),
        "review" => Some(Color::Blue),
        "accepted" => Some(Color::Green),
        "in-progress" => Some(Color::Cyan),
        "complete" => Some(Color::Green),
        "rejected" => Some(Color::Red),
        "superseded" => Some(Color::DarkGray),
        _ => None,
    }
}

const HASH_PALETTE: &[Color] = &[
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

/// Deterministic, visible colour for an arbitrary string, drawn from a fixed
/// palette. Shared by `tag_color` and the status fallback so unknown values
/// always get a stable, non-`Reset` colour.
pub fn hash_palette_color(s: &str) -> Color {
    let hash = s
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    HASH_PALETTE[(hash as usize) % HASH_PALETTE.len()]
}

/// Owned status-colour resolver, constructed once per frame from the user
/// config map and the ClickUp colour cache.
#[derive(Debug, Clone, Default)]
pub struct StatusPalette {
    user: BTreeMap<String, String>,
    cache: StatusColors,
}

impl StatusPalette {
    pub fn new(user: BTreeMap<String, String>, cache: StatusColors) -> Self {
        Self { user, cache }
    }
}

pub fn status_color(palette: &StatusPalette, type_name: &str, status: &Status) -> Color {
    let name = status.as_str();
    if let Some(c) = palette.user.get(name).and_then(|v| parse_color(v)) {
        return c;
    }
    if let Some(c) = palette.cache.get(type_name, name).and_then(hex_to_color) {
        return c;
    }
    if let Some(c) = builtin_status_color(name) {
        return c;
    }
    hash_palette_color(name)
}

pub fn tag_color(tag: &str) -> Color {
    hash_palette_color(tag)
}

/// Map a semantic spinner [`FrameColour`] to a ratatui style. Accent is the
/// terminal default foreground; success/error carry the conventional
/// green/red; dim is a modifier so it tracks the terminal's own palette.
pub fn frame_style(colour: crate::spinners::FrameColour) -> Style {
    use crate::spinners::FrameColour;
    match colour {
        FrameColour::Accent => Style::default(),
        FrameColour::Dim => Style::default().add_modifier(Modifier::DIM),
        FrameColour::Success => Style::default().fg(Color::Green),
        FrameColour::Error => Style::default().fg(Color::Red),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn frame_style_maps_colours() {
        use crate::spinners::FrameColour;
        assert_eq!(frame_style(FrameColour::Accent), Style::default());
        assert_eq!(
            frame_style(FrameColour::Success),
            Style::default().fg(Color::Green)
        );
        assert_eq!(
            frame_style(FrameColour::Error),
            Style::default().fg(Color::Red)
        );
        assert_eq!(
            frame_style(FrameColour::Dim),
            Style::default().add_modifier(Modifier::DIM)
        );
    }

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

    fn cache_with(type_name: &str, status: &str, hex: &str) -> StatusColors {
        let mut cache = StatusColors::default();
        cache.set_type(
            type_name,
            HashMap::from([(status.to_string(), hex.to_string())]),
        );
        cache
    }

    #[test]
    fn status_color_uses_cached_hex_on_hit() {
        let palette = StatusPalette::new(
            BTreeMap::new(),
            cache_with("clickup-tasks", "pending", "#336699"),
        );
        assert_eq!(
            status_color(&palette, "clickup-tasks", &Status::new("pending")),
            Color::Rgb(0x33, 0x66, 0x99)
        );
    }

    #[test]
    fn status_color_user_config_wins_over_cache_and_builtin() {
        let palette = StatusPalette::new(
            BTreeMap::from([("draft".to_string(), "magenta".to_string())]),
            cache_with("story", "draft", "#336699"),
        );
        assert_eq!(
            status_color(&palette, "story", &Status::new("draft")),
            Color::Magenta
        );
    }

    #[test]
    fn status_color_user_config_accepts_hex() {
        let palette = StatusPalette::new(
            BTreeMap::from([("pending".to_string(), "#336699".to_string())]),
            StatusColors::default(),
        );
        assert_eq!(
            status_color(&palette, "story", &Status::new("pending")),
            Color::Rgb(0x33, 0x66, 0x99)
        );
    }

    #[test]
    fn status_color_uses_cache_when_no_user_entry() {
        let palette = StatusPalette::new(
            BTreeMap::new(),
            cache_with("clickup-tasks", "pending", "#112233"),
        );
        assert_eq!(
            status_color(&palette, "clickup-tasks", &Status::new("pending")),
            Color::Rgb(0x11, 0x22, 0x33)
        );
    }

    #[test]
    fn status_color_falls_back_to_name_match_on_miss() {
        let palette = StatusPalette::default();
        assert_eq!(
            status_color(&palette, "story", &Status::new("draft")),
            Color::Yellow
        );
    }

    #[test]
    fn status_color_unknown_status_is_stable_and_not_reset() {
        let palette = StatusPalette::default();
        let first = status_color(&palette, "story", &Status::new("that-status"));
        let second = status_color(&palette, "story", &Status::new("that-status"));
        assert_ne!(first, Color::Reset);
        assert_eq!(first, second);
        assert_eq!(first, hash_palette_color("that-status"));
    }

    #[test]
    fn status_color_falls_back_on_invalid_user_value() {
        let palette = StatusPalette::new(
            BTreeMap::from([("draft".to_string(), "#zzzzzz".to_string())]),
            StatusColors::default(),
        );
        assert_eq!(
            status_color(&palette, "story", &Status::new("draft")),
            Color::Yellow
        );
        let palette = StatusPalette::new(
            BTreeMap::from([("draft".to_string(), "notacolour".to_string())]),
            StatusColors::default(),
        );
        assert_eq!(
            status_color(&palette, "story", &Status::new("draft")),
            Color::Yellow
        );
    }

    #[test]
    fn status_color_falls_back_on_garbage_cache_hex() {
        let palette = StatusPalette::new(BTreeMap::new(), cache_with("story", "draft", "#zzzzzz"));
        assert_eq!(
            status_color(&palette, "story", &Status::new("draft")),
            Color::Yellow
        );
    }
}
