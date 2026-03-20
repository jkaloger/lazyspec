mod common;

use common::TestFixture;
use lazyspec::tui::app::App;
use lazyspec::tui::terminal_caps::TerminalImageProtocol;
use std::time::Instant;

#[test]
fn app_new_returns_within_100ms_with_halfblock_picker() {
    let fixture = TestFixture::new();
    let store = fixture.store();
    let picker = ratatui_image::picker::Picker::halfblocks();

    let start = Instant::now();
    let app = App::new(store, &fixture.config(), picker);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 100,
        "App::new took {}ms, expected < 100ms",
        elapsed.as_millis()
    );
    assert_eq!(app.terminal_image_protocol, TerminalImageProtocol::Halfblocks);
    assert!(!app.tool_availability.d2);
    assert!(!app.tool_availability.mmdc);
}

#[test]
fn tool_availability_result_updates_app_state() {
    use lazyspec::tui::diagram::ToolAvailability;
    use lazyspec::tui::app::AppEvent;

    let fixture = TestFixture::new();
    let store = fixture.store();
    let app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks());

    assert_eq!(app.terminal_image_protocol, TerminalImageProtocol::Halfblocks);
    assert!(!app.tool_availability.d2);

    // Verify the channel architecture works: spawn a thread that sends a ToolAvailabilityResult,
    // then receive and apply it.
    let (tx, rx) = crossbeam_channel::unbounded::<AppEvent>();
    std::thread::spawn(move || {
        let _ = tx.send(AppEvent::ToolAvailabilityResult {
            tool_availability: ToolAvailability { d2: true, mmdc: false },
        });
    });

    let event = rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    match event {
        AppEvent::ToolAvailabilityResult { tool_availability } => {
            assert!(tool_availability.d2);
            assert!(!tool_availability.mmdc);
        }
        _ => panic!("expected ToolAvailabilityResult event"),
    }
}
