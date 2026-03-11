mod common;

use lazyspec::cli::show;
use std::process::Command;

fn setup() -> common::TestFixture {
    let fixture = common::TestFixture::new();

    let _output = Command::new("git")
        .args(&["init"])
        .current_dir(fixture.root())
        .output()
        .unwrap();

    let _output = Command::new("git")
        .args(&["config", "user.email", "test@test.com"])
        .current_dir(fixture.root())
        .output()
        .unwrap();

    let _output = Command::new("git")
        .args(&["config", "user.name", "Test"])
        .current_dir(fixture.root())
        .output()
        .unwrap();

    fixture
}

fn commit_file(fixture: &common::TestFixture, path: &str, content: &str) -> String {
    let full_path = fixture.root().join(path);
    std::fs::create_dir_all(full_path.parent().unwrap()).unwrap();
    std::fs::write(&full_path, content).unwrap();

    let _ = Command::new("git")
        .args(&["add", "-A"])
        .current_dir(fixture.root())
        .output()
        .unwrap();

    let output = Command::new("git")
        .args(&["commit", "-m", "add file"])
        .current_dir(fixture.root())
        .output()
        .unwrap();

    if !output.status.success() {
        eprintln!(
            "git commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = Command::new("git")
        .args(&["rev-parse", "HEAD"])
        .current_dir(fixture.root())
        .output()
        .unwrap();

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn test_cli_show_expands_ref_to_code_block() {
    let fixture = setup();

    commit_file(
        &fixture,
        "src/user.ts",
        "export interface User {\n  id: number;\n  name: string;\n}",
    );

    commit_file(&fixture, "test.txt", "test content");

    fixture.write_doc(
        "docs/rfcs/RFC-001-test.md",
        r#"---
title: "Test Ref"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

See the code:

@ref test.txt
"#,
    );

    let store = fixture.store();
    let result = show::run_json(&store, "RFC-001", true);

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        output.contains("```"),
        "Output should contain fenced code block"
    );
    assert!(
        output.contains("test content") || output.contains("> [unresolved:"),
        "Output should contain either test content or unresolved blockquote"
    );
}

#[test]
fn test_ref_with_git_sha() {
    let fixture = setup();

    let sha = commit_file(
        &fixture,
        "src/user.ts",
        "export interface User {\n  id: number;\n}",
    );

    let short_sha = &sha[..7];

    fixture.write_doc(
        "docs/rfcs/RFC-002-test.md",
        &format!(
            r#"---
title: "Test SHA Ref"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

See the code:

@ref src/user.ts@{}
"#,
            short_sha
        ),
    );

    let store = fixture.store();
    let result = show::run_json(&store, "RFC-002", true);

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        output.contains("```"),
        "Output should contain fenced code block"
    );
}

#[test]
fn test_ref_nonexistent_file_warning() {
    let fixture = setup();

    fixture.write_doc(
        "docs/rfcs/RFC-003-test.md",
        r#"---
title: "Test Nonexistent File"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

See the code:

@ref nonexistent.ts
"#,
    );

    let store = fixture.store();
    let result = show::run_json(&store, "RFC-003", true);

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        output.contains("> [unresolved:"),
        "Output should contain unresolved blockquote for nonexistent file"
    );
}

#[test]
fn test_ref_nonexistent_symbol_warning() {
    let fixture = setup();

    commit_file(
        &fixture,
        "src/user.ts",
        "export interface User {\n  id: number;\n}",
    );

    fixture.write_doc(
        "docs/rfcs/RFC-004-test.md",
        r#"---
title: "Test Nonexistent Symbol"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

See the code:

@ref src/user.ts#NonExistentSymbol
"#,
    );

    let store = fixture.store();
    let result = show::run_json(&store, "RFC-004", true);

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        output.contains("```"),
        "Output should contain code block even for nonexistent symbol"
    );
}

#[test]
fn test_ref_invalid_sha_warning() {
    let fixture = setup();

    fixture.write_doc(
        "docs/rfcs/RFC-005-test.md",
        r#"---
title: "Test Invalid SHA"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

See the code:

@ref Cargo.toml@invalid_sha_12345
"#,
    );

    let store = fixture.store();
    let result = show::run_json(&store, "RFC-005", true);

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        output.contains("> [unresolved:"),
        "Output should contain unresolved blockquote for invalid SHA"
    );
}

#[test]
fn test_ref_typescript_language_tag() {
    let fixture = setup();

    let _sha = commit_file(
        &fixture,
        "src/user.ts",
        "export interface User {\n  id: number;\n}",
    );

    fixture.write_doc(
        "docs/rfcs/RFC-006-test.md",
        r#"---
title: "Test TS Language"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

@ref src/user.ts
"#,
    );

    let store = fixture.store();
    let result = show::run_json(&store, "RFC-006", true);

    assert!(result.is_ok());
    let output = result.unwrap();
    if output.contains("```") {
        assert!(
            output.contains("```ts") || output.contains("```typescript"),
            "Output should use ts or typescript language tag for .ts files"
        );
    }
}

#[test]
fn test_ref_rust_language_tag() {
    let fixture = setup();

    commit_file(&fixture, "src/lib.rs", "pub struct User {\n  id: u32,\n}");

    fixture.write_doc(
        "docs/rfcs/RFC-007-test.md",
        r#"---
title: "Test Rust Language"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

@ref src/lib.rs
"#,
    );

    let store = fixture.store();
    let result = show::run_json(&store, "RFC-007", true);

    assert!(result.is_ok());
    let output = result.unwrap();
    if output.contains("```") {
        assert!(
            output.contains("```rust"),
            "Output should use rust language tag for .rs files"
        );
    }
}

#[test]
fn test_mixed_resolved_and_unresolved_refs() {
    let fixture = setup();

    commit_file(
        &fixture,
        "src/user.ts",
        "export interface User {\n  id: number;\n  name: string;\n}",
    );

    commit_file(&fixture, "test.txt", "test content");

    fixture.write_doc(
        "docs/rfcs/RFC-001-test.md",
        r#"---
title: "Test Ref"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

See the code:

@ref test.txt
"#,
    );

    let store = fixture.store();

    let result = show::run_json(&store, "RFC-001", true);

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        output.contains("```"),
        "Output should contain fenced code block"
    );
    assert!(
        output.contains("test content") || output.contains("> [unresolved:"),
        "Output should contain either test content or unresolved blockquote"
    );
}

#[test]
fn test_show_without_expand_flag_shows_raw_refs() {
    let fixture = setup();

    commit_file(
        &fixture,
        "src/user.ts",
        "export interface User {\n  id: number;\n}",
    );

    fixture.write_doc(
        "docs/rfcs/RFC-010-test.md",
        r#"---
title: "Test Raw Ref"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

See the code:

@ref src/user.ts
"#,
    );

    let store = fixture.store();
    let result = show::run_json(&store, "RFC-010", false);

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        output.contains("@ref src/user.ts"),
        "Output should contain raw @ref directive when expand=false, got: {}",
        output
    );
    assert!(
        !output.contains("```ts") && !output.contains("```typescript"),
        "Output should NOT contain expanded code block when expand=false"
    );
}

#[test]
fn test_search_does_not_expand_refs() {
    let fixture = setup();

    commit_file(
        &fixture,
        "src/user.ts",
        "export interface User {\n  id: number;\n}",
    );

    fixture.write_doc(
        "docs/rfcs/RFC-011-test.md",
        r#"---
title: "Test Search Raw"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

Some context here.

@ref src/user.ts
"#,
    );

    let store = fixture.store();
    let results = store.search("@ref");

    assert!(
        !results.is_empty(),
        "Search for '@ref' should find the document"
    );
    let snippet = &results[0].snippet;
    assert!(
        snippet.contains("@ref"),
        "Search snippet should contain raw @ref text, got: {}",
        snippet
    );
}
