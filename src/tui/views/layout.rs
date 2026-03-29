use ratatui::text::Line;
use unicode_width::UnicodeWidthStr;

pub fn wrapped_line_count(line: &Line, content_width: usize) -> usize {
    if content_width == 0 {
        return 1;
    }
    let line_width: usize = line
        .spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum();
    if line_width == 0 {
        return 1;
    }
    line_width.div_ceil(content_width)
}

pub fn wrapped_lines_total(lines: &[Line], content_width: usize) -> usize {
    lines
        .iter()
        .map(|l| wrapped_line_count(l, content_width))
        .sum()
}

pub fn calculate_image_height(
    image_width: u32,
    image_height: u32,
    available_width_cells: u16,
    panel_height: u16,
) -> u16 {
    if image_width == 0 || available_width_cells == 0 {
        return 1;
    }
    let ratio_height = (image_height as f64 / image_width as f64) * available_width_cells as f64;
    let max_height = (panel_height as f64 * 0.8) as u16;
    let clamped = (ratio_height as u16).min(max_height);
    clamped.max(1)
}
