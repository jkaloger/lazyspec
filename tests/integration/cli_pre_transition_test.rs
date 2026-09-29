use std::process::{Command, Output};

const BLOCK: &str =
    "cat >/dev/null\necho '{\"findings\":[{\"severity\":\"error\",\"message\":\"nope\"}]}'\n";
const WARN: &str =
    "cat >/dev/null\necho '{\"findings\":[{\"severity\":\"warning\",\"message\":\"heads up\"}]}'\n";
const REWRITE: &str = r#"input=$(cat)
hash=$(printf '%s' "$input" | sed -n 's/.*"content_hash":"\([0-9a-f]*\)".*/\1/p')
printf '{"updates":[{"id":"RFC-001","hash":"%s","body":"rewritten by hook"}]}' "$hash"
"#;

struct Project {
    fixture: crate::common::TestFixture,
    state: tempfile::TempDir,
}

impl Project {
    fn new(script: &str) -> Self {
        let fixture = crate::common::TestFixture::new();
        let root = fixture.root();
        let toml = std::fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
        let hook = "[[hooks]]\nname = \"gate\"\nevent = \"pre-transition\"\ntypes = [\"rfc\"]\nrun = [\"sh\", \".lazyspec/hooks/gate.sh\"]\n";
        std::fs::write(root.join(".lazyspec.toml"), format!("{toml}\n{hook}")).unwrap();
        std::fs::create_dir_all(root.join(".lazyspec/hooks")).unwrap();
        std::fs::write(root.join(".lazyspec/hooks/gate.sh"), script).unwrap();
        fixture.write_rfc("RFC-001-good.md", "Good", "draft");
        Self {
            fixture,
            state: tempfile::tempdir().unwrap(),
        }
    }

    fn trusted(script: &str) -> Self {
        let project = Self::new(script);
        assert!(project.run(&["hook", "trust"]).status.success());
        project
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lazyspec"))
            .args(args)
            .current_dir(self.fixture.root())
            .env("LAZYSPEC_STATE_DIR", self.state.path())
            .output()
            .unwrap()
    }

    fn doc(&self) -> serde_json::Value {
        let output = self.run(&["show", "RFC-001", "--json"]);
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

#[test]
fn an_error_finding_blocks_the_move_and_reports_in_json() {
    let project = Project::trusted(BLOCK);

    let output = project.run(&["update", "RFC-001", "--status", "review", "--json"]);

    assert!(!output.status.success());
    let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["findings"][0]["hook"], "gate");
    assert_eq!(body["findings"][0]["message"], "nope");
    assert_eq!(project.doc()["status"], "draft");
}

#[test]
fn an_error_finding_blocks_the_move_and_prints_findings_in_text() {
    let project = Project::trusted(BLOCK);

    let output = project.run(&["update", "RFC-001", "--status", "review"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("error [gate]: nope"));
}

#[test]
fn a_warning_is_reported_and_the_move_happens() {
    let project = Project::trusted(WARN);

    let output = project.run(&["update", "RFC-001", "--status", "review", "--json"]);

    assert!(output.status.success());
    let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["status"], "review");
    assert_eq!(body["hook_findings"][0]["message"], "heads up");
}

#[test]
fn updates_are_saved_together_with_the_status() {
    let project = Project::trusted(REWRITE);

    let output = project.run(&["update", "RFC-001", "--status", "review", "--json"]);

    assert!(output.status.success(), "{output:?}");
    let doc = project.doc();
    assert_eq!(doc["status"], "review");
    assert!(doc["body"].as_str().unwrap().contains("rewritten by hook"));
}

#[test]
fn an_update_with_a_stale_hash_saves_nothing() {
    let project = Project::trusted(
        "cat >/dev/null\necho '{\"updates\":[{\"id\":\"RFC-001\",\"hash\":\"stale\",\"body\":\"x\"}]}'\n",
    );

    let output = project.run(&["update", "RFC-001", "--status", "review"]);

    assert!(!output.status.success());
    assert_eq!(project.doc()["status"], "draft");
    assert!(!project.doc()["body"].as_str().unwrap().contains('x'));
}

#[test]
fn untrusted_hooks_are_skipped_with_a_warning_and_the_move_happens() {
    let project = Project::new(BLOCK);

    let output = project.run(&["update", "RFC-001", "--status", "review", "--json"]);

    assert!(output.status.success());
    let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(body["hook_findings"][0]["message"]
        .as_str()
        .unwrap()
        .contains("hook trust"));
    assert_eq!(body["status"], "review");
}

#[test]
fn no_hooks_skips_transition_hooks() {
    let project = Project::trusted(BLOCK);

    let output = project.run(&["update", "RFC-001", "--status", "review", "--no-hooks"]);

    assert!(output.status.success());
    assert_eq!(project.doc()["status"], "review");
}

#[test]
fn hook_run_dry_run_reports_updates_and_saves_nothing() {
    let project = Project::trusted(REWRITE);

    let output = project.run(&[
        "hook",
        "run",
        "pre-transition",
        "RFC-001",
        "--dry-run",
        "--json",
    ]);

    assert!(output.status.success(), "{output:?}");
    let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["dry_run"], true);
    assert_eq!(body["updates"][0]["body"], "rewritten by hook");
    assert!(!project.doc()["body"]
        .as_str()
        .unwrap()
        .contains("rewritten"));
}

#[test]
fn hook_run_saves_updates_without_changing_status() {
    let project = Project::trusted(REWRITE);

    let output = project.run(&["hook", "run", "pre-transition", "RFC-001"]);

    assert!(output.status.success(), "{output:?}");
    let doc = project.doc();
    assert_eq!(doc["status"], "draft");
    assert!(doc["body"].as_str().unwrap().contains("rewritten by hook"));
}
