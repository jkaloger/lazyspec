mod common;

use std::ffi::OsStr;
use std::process::Command;

use common::TestFixture;
use lazyspec::cli::completions;

fn cargo_bin() -> Command {
    Command::new(env!("CARGO"))
}

fn run_completions(shell: &str) -> std::process::Output {
    cargo_bin()
        .args(["run", "--", "completions", shell])
        .output()
        .expect("failed to run lazyspec")
}

#[test]
fn completions_zsh_produces_valid_output() {
    let output = run_completions("zsh");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty());
    assert!(stdout.contains("lazyspec"));
}

#[test]
fn completions_bash_produces_valid_output() {
    let output = run_completions("bash");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty());
    assert!(stdout.contains("lazyspec"));
}

#[test]
fn completions_fish_produces_valid_output() {
    let output = run_completions("fish");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty());
    assert!(stdout.contains("lazyspec"));
}

#[test]
fn completions_unsupported_shell_gives_error() {
    let output = run_completions("nushell");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty());
}

#[test]
fn doc_id_completer_returns_matching_ids() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth", "accepted");
    fixture.write_story("STORY-001-login.md", "Login", "draft", None);

    let candidates = completions::complete_doc_id_in(fixture.root(), OsStr::new(""));
    let values: Vec<String> = candidates
        .iter()
        .map(|c| c.get_value().to_string_lossy().into_owned())
        .collect();

    assert!(values.contains(&"RFC-001".to_string()));
    assert!(values.contains(&"STORY-001".to_string()));
}

#[test]
fn doc_id_completer_filters_by_prefix() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth", "accepted");
    fixture.write_story("STORY-001-login.md", "Login", "draft", None);

    let candidates = completions::complete_doc_id_in(fixture.root(), OsStr::new("RFC"));
    let values: Vec<String> = candidates
        .iter()
        .map(|c| c.get_value().to_string_lossy().into_owned())
        .collect();

    assert!(values.contains(&"RFC-001".to_string()));
    assert!(!values.contains(&"STORY-001".to_string()));
}

#[test]
fn rel_type_completer_returns_all_types() {
    let candidates = completions::complete_rel_type(OsStr::new(""));
    let values: Vec<String> = candidates
        .iter()
        .map(|c| c.get_value().to_string_lossy().into_owned())
        .collect();

    assert_eq!(
        values,
        vec!["implements", "supersedes", "blocks", "related-to"]
    );
}

#[test]
fn rel_type_completer_filters_by_prefix() {
    let candidates = completions::complete_rel_type(OsStr::new("b"));
    let values: Vec<String> = candidates
        .iter()
        .map(|c| c.get_value().to_string_lossy().into_owned())
        .collect();

    assert_eq!(values, vec!["blocks"]);
}

#[test]
fn doc_id_completer_returns_empty_on_broken_store() {
    let dir = tempfile::TempDir::new().unwrap();
    let candidates = completions::complete_doc_id_in(dir.path(), OsStr::new(""));
    assert!(candidates.is_empty());
}
