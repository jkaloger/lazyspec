mod common;

use lazyspec::tui::ui::calculate_image_height;

#[test]
fn square_image_matches_available_width() {
    // 100x100 image at 50 cells wide => height = (100/100) * 50 = 50
    let height = calculate_image_height(100, 100, 50, 40);
    assert_eq!(height, 32); // clamped to 80% of 40 = 32
}

#[test]
fn wide_image_shorter_than_panel() {
    // 200x100 image at 50 cells wide => height = (100/200) * 50 = 25
    let height = calculate_image_height(200, 100, 50, 40);
    assert_eq!(height, 25);
}

#[test]
fn tall_image_clamped_to_max() {
    // 100x400 image at 50 cells wide => height = (400/100) * 50 = 200
    // clamped to 80% of 40 = 32
    let height = calculate_image_height(100, 400, 50, 40);
    assert_eq!(height, 32);
}

#[test]
fn zero_width_image_returns_minimum() {
    let height = calculate_image_height(0, 100, 50, 40);
    assert_eq!(height, 1);
}

#[test]
fn zero_available_width_returns_minimum() {
    let height = calculate_image_height(100, 100, 0, 40);
    assert_eq!(height, 1);
}

#[test]
fn typical_diagram_dimensions() {
    // 800x400 image at 80 cells wide => height = (400/800) * 80 = 40
    // panel_height = 60, max = 48
    let height = calculate_image_height(800, 400, 80, 60);
    assert_eq!(height, 40);
}

#[test]
fn small_panel_clamps_appropriately() {
    // 800x400 image at 80 cells wide => height = 40
    // but panel_height = 10, max = 8
    let height = calculate_image_height(800, 400, 80, 10);
    assert_eq!(height, 8);
}
