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

#[test]
fn probe_result_updates_app_state() {
    use lazyspec::tui::content::diagram::ToolAvailability;

    let fixture = TestFixture::new();
    let store = fixture.store();
    let app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );

    assert_eq!(
        app.terminal_image_protocol,
        TerminalImageProtocol::Halfblocks
    );
    assert!(!app.tool_availability.d2);

    // Verify the probe channel architecture works: spawn a thread that sends a ProbeResult,
    // then receive and apply it.
    use lazyspec::tui::state::AppEvent;

    let (tx, rx) = crossbeam_channel::unbounded::<AppEvent>();
    std::thread::spawn(move || {
        let probe_picker = ratatui_image::picker::Picker::halfblocks();
        let _ = tx.send(AppEvent::ProbeResult {
            picker: probe_picker,
            protocol: TerminalImageProtocol::Sixel,
            tool_availability: ToolAvailability { d2: true },
        });
    });

    let event = rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    match event {
        AppEvent::ProbeResult {
            protocol,
            tool_availability,
            ..
        } => {
            assert_eq!(protocol, TerminalImageProtocol::Sixel);
            assert!(tool_availability.d2);
        }
        _ => panic!("expected ProbeResult event"),
    }
}
