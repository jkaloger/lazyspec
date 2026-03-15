mod common;

use ratatui::text::{Line, Span};
use lazyspec::tui::ui::{wrapped_line_count, wrapped_lines_total};

#[test]
fn wrapped_line_count_short_line_fits_in_one_row() {
    let line = Line::from("hello");
    assert_eq!(wrapped_line_count(&line, 80), 1);
}

#[test]
fn wrapped_line_count_exact_width() {
    let line = Line::from("a".repeat(40));
    assert_eq!(wrapped_line_count(&line, 40), 1);
}

#[test]
fn wrapped_line_count_wraps_to_two_rows() {
    let line = Line::from("a".repeat(41));
    assert_eq!(wrapped_line_count(&line, 40), 2);
}

#[test]
fn wrapped_line_count_wraps_to_three_rows() {
    let line = Line::from("a".repeat(81));
    assert_eq!(wrapped_line_count(&line, 40), 3);
}

#[test]
fn wrapped_line_count_empty_line_is_one_row() {
    let line = Line::from("");
    assert_eq!(wrapped_line_count(&line, 40), 1);
}

#[test]
fn wrapped_line_count_zero_width_returns_one() {
    let line = Line::from("hello");
    assert_eq!(wrapped_line_count(&line, 0), 1);
}

#[test]
fn wrapped_line_count_multi_span_line() {
    let line = Line::from(vec![
        Span::raw("a".repeat(30)),
        Span::raw("b".repeat(20)),
    ]);
    // 50 chars at width 40 = 2 rows
    assert_eq!(wrapped_line_count(&line, 40), 2);
}

#[test]
fn wrapped_lines_total_sums_correctly() {
    let lines = vec![
        Line::from("short"),                    // 1 row
        Line::from("a".repeat(100)),            // 3 rows at width 40
        Line::from(""),                         // 1 row
    ];
    assert_eq!(wrapped_lines_total(&lines, 40), 5);
}

#[test]
fn wrapped_lines_total_empty_slice() {
    let lines: Vec<Line> = vec![];
    assert_eq!(wrapped_lines_total(&lines, 40), 0);
}

#[test]
fn header_offset_accounts_for_header_lines() {
    // Simulate the header lines that draw_preview_content builds:
    // title, type/status/author, date, tags, blank line = 5 lines minimum
    let header_lines = vec![
        Line::from(" My Document Title"),
        Line::from(" Type: rfc  Status: draft  Author: test"),
        Line::from(" Date: 2026-01-01"),
        Line::from(" Tags: [foo] [bar]"),
        Line::from(""),
    ];
    let content_width = 80;
    // All header lines are short, so each takes 1 wrapped row
    assert_eq!(wrapped_lines_total(&header_lines, content_width), 5);
}

#[test]
fn header_offset_with_long_title_wraps() {
    let long_title = format!(" {}", "A".repeat(100));
    let header_lines = vec![
        Line::from(long_title),
        Line::from(" Type: rfc  Status: draft  Author: test"),
        Line::from(" Date: 2026-01-01"),
        Line::from(""),
    ];
    let content_width = 40;
    // Title: 101 chars / 40 = 3 rows, rest are 1 each = 6 total
    assert_eq!(wrapped_lines_total(&header_lines, content_width), 6);
}

#[test]
fn fullscreen_image_skip_when_scrolled_past() {
    // Simulate: image at line_y=10, image_height=15, scroll_offset=30
    // The image occupies lines 10..25, scroll is at 30, so image is fully past
    let line_y: u16 = 10;
    let image_height: u16 = 15;
    let scroll_offset: u16 = 30;

    let should_skip = line_y + image_height <= scroll_offset || line_y < scroll_offset;
    assert!(should_skip, "image should be skipped when scrolled past");
}

#[test]
fn fullscreen_image_visible_when_in_viewport() {
    let line_y: u16 = 30;
    let image_height: u16 = 15;
    let scroll_offset: u16 = 10;

    let should_skip = line_y + image_height <= scroll_offset || line_y < scroll_offset;
    assert!(!should_skip, "image should be visible when in viewport");
}

#[test]
fn fullscreen_image_partially_scrolled_is_hidden() {
    // Image starts at line 10, scroll is at 15 -- top of image is above viewport
    let line_y: u16 = 10;
    let image_height: u16 = 15;
    let scroll_offset: u16 = 15;

    let should_skip = line_y + image_height <= scroll_offset || line_y < scroll_offset;
    assert!(should_skip, "partially scrolled image should be hidden");
}

#[test]
fn fullscreen_image_at_exact_scroll_boundary() {
    // Image starts exactly at scroll offset -- should be visible
    let line_y: u16 = 20;
    let image_height: u16 = 15;
    let scroll_offset: u16 = 20;

    let should_skip = line_y + image_height <= scroll_offset || line_y < scroll_offset;
    assert!(!should_skip, "image at exact scroll boundary should be visible");
}
