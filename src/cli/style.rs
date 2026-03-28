use std::path::Path;

use console::{colors_enabled, Style};

use crate::engine::document::{DocType, Status};

pub fn status_style(status: &Status) -> Style {
    let style = Style::new();
    match status {
        Status::Accepted => style.green(),
        Status::Draft => style.yellow(),
        Status::Review => style.blue(),
        Status::InProgress => style.cyan(),
        Status::Complete => style.green(),
        Status::Rejected => style.red(),
        Status::Superseded => style.color256(8),
    }
}

pub fn styled_status(status: &Status) -> String {
    status_style(status).apply_to(status).to_string()
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
        format!("\u{256d}\u{2500} {} {}\u{256e}", label, "\u{2500}".repeat(width))
    } else {
        format!("--- {} ---", label)
    }
}

pub fn doc_card(title: &str, doc_type: &DocType, status: &Status, path: &Path) -> String {
    let path_str = path.display().to_string();
    format!(
        "{} {} [{}] {}",
        bold(&format!("[{}]", doc_type)),
        bold(title),
        styled_status(status),
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
