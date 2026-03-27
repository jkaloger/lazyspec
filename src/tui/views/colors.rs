use ratatui::style::Color;

use crate::engine::document::Status;

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
    let hash = tag.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    PALETTE[(hash as usize) % PALETTE.len()]
}
