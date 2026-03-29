mod common;

use common::TestFixture;
use lazyspec::engine::config::Config;
use lazyspec::engine::store::Store;
use lazyspec::tui::state::App;
use lazyspec::tui::views::draw;
use ratatui::{backend::TestBackend, Terminal};

fn singleton_config() -> Config {
    let toml_str = r#"
[[types]]
name = "convention"
plural = "conventions"
dir = "docs/conventions"
prefix = "CONV"
singleton = true

[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
"#;
    Config::parse(toml_str).unwrap()
}

fn buffer_contains(terminal: &Terminal<TestBackend>, needle: &str) -> bool {
    let buf = terminal.backend().buffer();
    for y in 0..buf.area.height {
        let mut line = String::new();
        for x in 0..buf.area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        if line.contains(needle) {
            return true;
        }
    }
    false
}

#[test]
fn singleton_type_skips_doc_list() {
    let fixture = TestFixture::new();
    let config = singleton_config();

    // Create the convention directory and a document
    std::fs::create_dir_all(fixture.root().join("docs/conventions")).unwrap();
    fixture.write_doc(
        "docs/conventions/CONV-001-coding-standards.md",
        "---\ntitle: Coding Standards\ntype: convention\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\nConvention body content.\n",
    );

    let store = Store::load(fixture.root(), &config).unwrap();
    let mut app = App::new(
        store,
        &config,
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );

    // Select the convention type (first type in config)
    app.selected_type = 0;
    assert_eq!(app.current_type().as_str(), "convention");

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| draw(f, &mut app, &config))
        .unwrap();

    // The doc list renders a table with block title " Documents ".
    // For a singleton, this should not appear.
    assert!(
        !buffer_contains(&terminal, "Documents"),
        "Singleton type should not render the doc list table (found 'Documents' title)"
    );
}

#[test]
fn non_singleton_type_shows_doc_list() {
    let fixture = TestFixture::new();
    let config = singleton_config();

    // Create the rfc directory and a document
    std::fs::create_dir_all(fixture.root().join("docs/rfcs")).unwrap();
    fixture.write_doc(
        "docs/rfcs/RFC-001-first.md",
        "---\ntitle: First RFC\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\nBody.\n",
    );

    let store = Store::load(fixture.root(), &config).unwrap();
    let mut app = App::new(
        store,
        &config,
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );

    // Select the rfc type (second type in config)
    app.selected_type = 1;
    assert_eq!(app.current_type().as_str(), "rfc");

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| draw(f, &mut app, &config))
        .unwrap();

    // The doc list table SHOULD be present for non-singleton types
    assert!(
        buffer_contains(&terminal, "Documents"),
        "Non-singleton type should render the doc list table (expected 'Documents' title)"
    );
}
