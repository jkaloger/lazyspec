mod common;

use common::TestFixture;
use lazyspec::tui::infra::terminal_caps::TerminalImageProtocol;
use lazyspec::tui::state::App;
use std::time::Instant;

#[test]
fn app_new_returns_within_100ms_with_halfblock_picker() {
    let fixture = TestFixture::new();
    let store = fixture.store();
    let picker = ratatui_image::picker::Picker::halfblocks();

    let start = Instant::now();
    let app = App::new(
        store,
        &fixture.config(),
        picker,
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 100,
        "App::new took {}ms, expected < 100ms",
        elapsed.as_millis()
    );
    assert_eq!(
        app.terminal_image_protocol,
        TerminalImageProtocol::Halfblocks
    );
    assert!(!app.tool_availability.d2);
}
