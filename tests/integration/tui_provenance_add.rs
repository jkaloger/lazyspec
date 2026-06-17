use crate::common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::engine::store::Store;
use lazyspec::tui::state::App;

fn make_app(fixture: &TestFixture) -> App {
    let store = fixture.store();
    App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    )
}

fn write_rfc_with_provenance(
    fixture: &TestFixture,
    filename: &str,
    title: &str,
    provenance: &[&str],
) -> std::path::PathBuf {
    let prov_yaml = if provenance.is_empty() {
        String::from("provenance: []")
    } else {
        let mut s = String::from("provenance:\n");
        for p in provenance {
            s.push_str(&format!("- {}\n", p));
        }
        s.trim_end().to_string()
    };
    let content = format!(
        "---\ntitle: \"{}\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n{}\n---\n",
        title, prov_yaml
    );
    fixture.write_doc(&format!("docs/rfcs/{}", filename), &content)
}

// AC1: opening editor on a selected doc activates it.
#[test]
fn open_provenance_editor_activates_on_selected_doc() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", "Test RFC", &[]);
    let mut app = make_app(&fixture);

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_provenance_editor();

    assert!(app.provenance_editor.active);
    assert_eq!(
        app.provenance_editor.doc_path,
        std::path::PathBuf::from("docs/rfcs/RFC-001-test.md")
    );
    assert!(app.provenance_editor.input.is_empty());
    assert!(app.provenance_editor.error.is_none());
}

// AC7: opening editor with no document selected is a no-op.
#[test]
fn open_provenance_editor_noop_when_no_selection() {
    let fixture = TestFixture::new();
    let mut app = make_app(&fixture);

    app.open_provenance_editor();

    assert!(!app.provenance_editor.active);
}

// AC2 + AC5: valid submission appends + persists, store reloads.
#[test]
fn submit_provenance_appends_and_persists() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", "Test RFC", &[]);
    let mut app = make_app(&fixture);

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_provenance_editor();
    for c in "Workshop 2026".chars() {
        app.provenance_type_char(c);
    }
    app.submit_provenance(fixture.root(), &fixture.config())
        .unwrap();

    assert!(!app.provenance_editor.active);

    let in_memory = app
        .store
        .all_docs()
        .iter()
        .find(|d| d.path == std::path::Path::new("docs/rfcs/RFC-001-test.md"))
        .map(|d| d.provenance.clone())
        .expect("doc still in store");
    assert_eq!(in_memory, vec!["Workshop 2026".to_string()]);

    let fresh = Store::load(fixture.root(), &fixture.config()).unwrap();
    let on_disk = fresh
        .all_docs()
        .iter()
        .find(|d| d.path == std::path::Path::new("docs/rfcs/RFC-001-test.md"))
        .map(|d| d.provenance.clone())
        .expect("doc on disk");
    assert_eq!(on_disk, vec!["Workshop 2026".to_string()]);
}

// AC3: empty submission keeps overlay open with error.
#[test]
fn submit_provenance_rejects_empty() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", "Test RFC", &[]);
    let mut app = make_app(&fixture);

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_provenance_editor();
    app.submit_provenance(fixture.root(), &fixture.config())
        .unwrap();

    assert!(app.provenance_editor.active);
    let err = app.provenance_editor.error.as_ref().expect("error set");
    assert!(err.contains("empty"), "error was: {err}");
}

// AC3: whitespace-only submission also rejected as empty.
#[test]
fn submit_provenance_rejects_whitespace() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", "Test RFC", &[]);
    let mut app = make_app(&fixture);

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_provenance_editor();
    for c in "   ".chars() {
        app.provenance_type_char(c);
    }
    app.submit_provenance(fixture.root(), &fixture.config())
        .unwrap();

    assert!(app.provenance_editor.active);
    let err = app.provenance_editor.error.as_ref().expect("error set");
    assert!(err.contains("empty"), "error was: {err}");
}

// AC6: duplicate citation rejected, on-disk unchanged.
#[test]
fn submit_provenance_rejects_duplicate() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", "Test RFC", &["X"]);
    let mut app = make_app(&fixture);

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_provenance_editor();
    app.provenance_type_char('X');
    app.submit_provenance(fixture.root(), &fixture.config())
        .unwrap();

    assert!(app.provenance_editor.active);
    let err = app.provenance_editor.error.as_ref().expect("error set");
    assert!(err.contains("already"), "error was: {err}");

    let fresh = Store::load(fixture.root(), &fixture.config()).unwrap();
    let on_disk = fresh
        .all_docs()
        .iter()
        .find(|d| d.path == std::path::Path::new("docs/rfcs/RFC-001-test.md"))
        .map(|d| d.provenance.clone())
        .expect("doc on disk");
    assert_eq!(on_disk, vec!["X".to_string()]);
}

// AC4: closing the editor clears all state.
#[test]
fn close_provenance_editor_clears_state() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", "Test RFC", &[]);
    let mut app = make_app(&fixture);

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_provenance_editor();
    for c in "Some text".chars() {
        app.provenance_type_char(c);
    }
    app.close_provenance_editor();

    assert!(!app.provenance_editor.active);
    assert!(app.provenance_editor.input.is_empty());
    assert!(app.provenance_editor.error.is_none());
    assert_eq!(app.provenance_editor.doc_path, std::path::PathBuf::new());
}

// AC8: engine error keeps overlay open and leaves in-memory store unchanged.
#[test]
fn submit_provenance_engine_error_keeps_overlay_open() {
    let fixture = TestFixture::new();
    let path = write_rfc_with_provenance(&fixture, "RFC-001-test.md", "Test RFC", &[]);
    let mut app = make_app(&fixture);

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_provenance_editor();
    for c in "Cite".chars() {
        app.provenance_type_char(c);
    }

    // Force the engine write to fail by deleting the file from disk after
    // opening the editor. set_provenance will fail to resolve the doc.
    std::fs::remove_file(&path).unwrap();

    app.submit_provenance(fixture.root(), &fixture.config())
        .unwrap();

    assert!(
        app.provenance_editor.active,
        "overlay should stay open on engine error"
    );
    assert!(
        app.provenance_editor.error.is_some(),
        "error should be populated"
    );

    let in_memory = app
        .store
        .all_docs()
        .iter()
        .find(|d| d.path == std::path::Path::new("docs/rfcs/RFC-001-test.md"))
        .map(|d| d.provenance.clone())
        .expect("doc still in in-memory store");
    assert!(
        in_memory.is_empty(),
        "in-memory provenance must be unchanged"
    );
}

fn app_with_selected_doc(fixture: &TestFixture) -> App {
    write_rfc_with_provenance(fixture, "RFC-001-test.md", "Test RFC", &[]);
    let mut app = make_app(fixture);
    app.selected_type = 0;
    app.selected_doc = 0;
    app
}

fn press(app: &mut App, fixture: &TestFixture, code: KeyCode) {
    app.handle_key(code, KeyModifiers::NONE, fixture.root(), &fixture.config());
}

// AC1: pressing `p` on a selected doc opens the overlay.
#[test]
fn key_p_opens_overlay_when_doc_selected() {
    let fixture = TestFixture::new();
    let mut app = app_with_selected_doc(&fixture);

    press(&mut app, &fixture, KeyCode::Char('p'));

    assert!(app.provenance_editor.active);
}

// AC7: pressing `p` with no doc selected is a no-op.
#[test]
fn key_p_noop_when_no_selection() {
    let fixture = TestFixture::new();
    let mut app = make_app(&fixture);

    press(&mut app, &fixture, KeyCode::Char('p'));

    assert!(!app.provenance_editor.active);
}

// AC4: Esc closes the overlay and clears state.
#[test]
fn key_esc_closes_overlay() {
    let fixture = TestFixture::new();
    let mut app = app_with_selected_doc(&fixture);

    press(&mut app, &fixture, KeyCode::Char('p'));
    assert!(app.provenance_editor.active);

    press(&mut app, &fixture, KeyCode::Esc);

    assert!(!app.provenance_editor.active);
    assert!(app.provenance_editor.error.is_none());
    assert!(app.provenance_editor.input.is_empty());
}

// AC5: Enter submits, closes overlay, persists to disk and in-memory store.
#[test]
fn key_enter_submits_overlay() {
    let fixture = TestFixture::new();
    let mut app = app_with_selected_doc(&fixture);

    press(&mut app, &fixture, KeyCode::Char('p'));
    for c in "Workshop 2026".chars() {
        press(&mut app, &fixture, KeyCode::Char(c));
    }
    press(&mut app, &fixture, KeyCode::Enter);

    assert!(!app.provenance_editor.active);

    let in_memory = app
        .store
        .all_docs()
        .iter()
        .find(|d| d.path == std::path::Path::new("docs/rfcs/RFC-001-test.md"))
        .map(|d| d.provenance.clone())
        .expect("doc still in store");
    assert_eq!(in_memory, vec!["Workshop 2026".to_string()]);

    let fresh = Store::load(fixture.root(), &fixture.config()).unwrap();
    let on_disk = fresh
        .all_docs()
        .iter()
        .find(|d| d.path == std::path::Path::new("docs/rfcs/RFC-001-test.md"))
        .map(|d| d.provenance.clone())
        .expect("doc on disk");
    assert_eq!(on_disk, vec!["Workshop 2026".to_string()]);
}
