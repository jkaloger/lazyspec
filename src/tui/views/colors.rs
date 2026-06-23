use ratatui::style::Color;

use crate::engine::document::Status;

pub fn status_color(status: &Status) -> Color {
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
