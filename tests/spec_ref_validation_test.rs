mod common;

use common::TestFixture;
use lazyspec::engine::config::Config;

fn spec_index_with_refs(refs: &[&str]) -> String {
    let ref_lines: String = refs.iter().map(|r| format!("@ref {}\n", r)).collect();
    format!(
        "---\ntitle: \"Test Spec\"\ntype: spec\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n{}",
        ref_lines
    )
}

fn warning_messages(fixture: &TestFixture) -> Vec<String> {
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());
    result.warnings.iter().map(|w| format!("{}", w)).collect()
}

fn warning_messages_with_config(fixture: &TestFixture, config: &Config) -> Vec<String> {
    let store = lazyspec::engine::store::Store::load(fixture.root(), config).unwrap();
    let result = store.validate_full(config);
    result.warnings.iter().map(|w| format!("{}", w)).collect()
}

#[test]
fn ref_count_below_ceiling_no_warning() {
    let fixture = TestFixture::new();
    let refs: Vec<&str> = (0..14)
        .map(|i| match i {
            0 => "src/engine/a.rs",
            1 => "src/engine/b.rs",
            2 => "src/engine/c.rs",
            3 => "src/engine/d.rs",
            4 => "src/engine/e.rs",
            5 => "src/engine/f.rs",
            6 => "src/engine/g.rs",
            7 => "src/engine/h.rs",
            8 => "src/engine/i.rs",
            9 => "src/engine/j.rs",
            10 => "src/engine/k.rs",
            11 => "src/engine/l.rs",
            12 => "src/engine/m.rs",
            _ => "src/engine/n.rs",
        })
        .collect();
    let content = spec_index_with_refs(&refs);
    fixture.write_doc("docs/specs/SPEC-001-test.md", &content);

    let warnings = warning_messages(&fixture);
    let ref_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.contains("@ref targets"))
        .collect();
    assert!(
        ref_warnings.is_empty(),
        "14 refs should not exceed default ceiling of 15, got: {:?}",
        ref_warnings
    );
}

#[test]
fn ref_count_above_ceiling_produces_warning() {
    let fixture = TestFixture::new();
    let paths: Vec<String> = (0..16)
        .map(|i| format!("src/engine/file{}.rs", i))
        .collect();
    let refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
    let content = spec_index_with_refs(&refs);
    fixture.write_doc("docs/specs/SPEC-002-test.md", &content);

    let warnings = warning_messages(&fixture);
    let ref_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.contains("@ref targets"))
        .collect();
    assert_eq!(
        ref_warnings.len(),
        1,
        "16 refs should exceed default ceiling of 15, got: {:?}",
        warnings
    );
    assert!(
        ref_warnings[0].contains("16") && ref_warnings[0].contains("15"),
        "warning should mention count 16 and ceiling 15, got: {}",
        ref_warnings[0]
    );
}

#[test]
fn configurable_ceiling_overrides_default() {
    let fixture = TestFixture::new();
    let paths: Vec<String> = (0..6).map(|i| format!("src/engine/file{}.rs", i)).collect();
    let refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
    let content = spec_index_with_refs(&refs);
    fixture.write_doc("docs/specs/SPEC-003-test.md", &content);

    // ceiling=5 should trigger
    let mut config_low = Config::default();
    config_low.ref_count_ceiling = 5;
    let warnings = warning_messages_with_config(&fixture, &config_low);
    let ref_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.contains("@ref targets"))
        .collect();
    assert_eq!(
        ref_warnings.len(),
        1,
        "6 refs should exceed ceiling of 5, got: {:?}",
        warnings
    );

    // ceiling=10 should not trigger
    let mut config_high = Config::default();
    config_high.ref_count_ceiling = 10;
    let warnings = warning_messages_with_config(&fixture, &config_high);
    let ref_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.contains("@ref targets"))
        .collect();
    assert!(
        ref_warnings.is_empty(),
        "6 refs should not exceed ceiling of 10, got: {:?}",
        ref_warnings
    );
}

#[test]
fn refs_in_three_or_fewer_modules_no_warning() {
    let fixture = TestFixture::new();
    let refs = &[
        "src/engine/store.rs",
        "src/engine/config.rs",
        "src/cli/main.rs",
        "src/tui/app.rs",
    ];
    let content = spec_index_with_refs(refs);
    fixture.write_doc("docs/specs/SPEC-004-test.md", &content);

    let warnings = warning_messages(&fixture);
    let cross_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.contains("cross-cutting"))
        .collect();
    assert!(
        cross_warnings.is_empty(),
        "3 modules should not trigger cross-module warning, got: {:?}",
        cross_warnings
    );
}

#[test]
fn refs_spanning_more_than_three_modules_produces_advisory() {
    let fixture = TestFixture::new();
    let refs = &[
        "src/engine/store.rs",
        "src/cli/main.rs",
        "src/tui/app.rs",
        "src/utils/helpers.rs",
    ];
    let content = spec_index_with_refs(refs);
    fixture.write_doc("docs/specs/SPEC-005-test.md", &content);

    let warnings = warning_messages(&fixture);
    let cross_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.contains("cross-cutting"))
        .collect();
    assert_eq!(
        cross_warnings.len(),
        1,
        "4 modules should trigger cross-module warning, got: {:?}",
        warnings
    );
    assert!(
        cross_warnings[0].contains("4"),
        "warning should mention 4 modules, got: {}",
        cross_warnings[0]
    );
}

#[test]
fn non_spec_documents_skip_ref_validation() {
    let fixture = TestFixture::new();
    let paths: Vec<String> = (0..20).map(|i| format!("src/mod{}/file.rs", i)).collect();
    let ref_lines: String = paths.iter().map(|r| format!("@ref {}\n", r)).collect();
    let content = format!(
        "---\ntitle: \"Test RFC\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n{}",
        ref_lines
    );
    fixture.write_doc("docs/rfcs/RFC-001-test.md", &content);

    let warnings = warning_messages(&fixture);
    let ref_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.contains("@ref targets") || w.contains("cross-cutting"))
        .collect();
    assert!(
        ref_warnings.is_empty(),
        "RFC should not trigger ref validation, got: {:?}",
        ref_warnings
    );
}

#[test]
fn orphan_ref_produces_warning() {
    let fixture = TestFixture::new();
    let content = spec_index_with_refs(&["src/nonexistent.rs"]);
    fixture.write_doc("docs/specs/SPEC-010-orphan.md", &content);

    let warnings = warning_messages(&fixture);
    let orphan_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.contains("orphan ref"))
        .collect();
    assert_eq!(
        orphan_warnings.len(),
        1,
        "nonexistent ref target should produce orphan warning, got: {:?}",
        warnings
    );
    assert!(
        orphan_warnings[0].contains("src/nonexistent.rs"),
        "warning should mention the missing target, got: {}",
        orphan_warnings[0]
    );
}

#[test]
fn valid_ref_produces_no_orphan_warning() {
    let fixture = TestFixture::new();
    // Create the file that the ref points to
    let target_dir = fixture.root().join("src/engine");
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(target_dir.join("real.rs"), "fn main() {}").unwrap();

    let content = spec_index_with_refs(&["src/engine/real.rs"]);
    fixture.write_doc("docs/specs/SPEC-011-valid.md", &content);

    let warnings = warning_messages(&fixture);
    let orphan_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.contains("orphan ref"))
        .collect();
    assert!(
        orphan_warnings.is_empty(),
        "existing ref target should not produce orphan warning, got: {:?}",
        orphan_warnings
    );
}

#[test]
fn non_spec_documents_skip_orphan_ref_validation() {
    let fixture = TestFixture::new();
    let ref_lines = "@ref src/nonexistent.rs\n";
    let content = format!(
        "---\ntitle: \"Test RFC\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n{}",
        ref_lines
    );
    fixture.write_doc("docs/rfcs/RFC-002-orphan.md", &content);

    let warnings = warning_messages(&fixture);
    let orphan_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.contains("orphan ref"))
        .collect();
    assert!(
        orphan_warnings.is_empty(),
        "RFC should not trigger orphan ref validation, got: {:?}",
        orphan_warnings
    );
}
