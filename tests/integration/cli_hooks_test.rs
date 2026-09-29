use std::process::{Command, Output};

const HOOK_TOML: &str = r#"
[[hooks]]
name = "required-sections"
event = "validate"
types = ["rfc"]
run = ["sh", ".lazyspec/hooks/lint.sh"]
"#;

struct HookProject {
    fixture: crate::common::TestFixture,
    state: tempfile::TempDir,
}

impl HookProject {
    fn new() -> Self {
        let fixture = crate::common::TestFixture::new();
        let root = fixture.root();
        let toml = std::fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
        std::fs::write(root.join(".lazyspec.toml"), format!("{toml}\n{HOOK_TOML}")).unwrap();
        std::fs::create_dir_all(root.join(".lazyspec/hooks")).unwrap();
        std::fs::write(
            root.join(".lazyspec/hooks/lint.sh"),
            "cat >/dev/null\necho '{\"findings\":[{\"id\":\"RFC-001\",\"line\":3,\"severity\":\"error\",\"message\":\"missing ## Goals\"}]}'\n",
        )
        .unwrap();
        fixture.write_rfc("RFC-001-good.md", "Good", "draft");
        Self {
            fixture,
            state: tempfile::tempdir().unwrap(),
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lazyspec"))
            .args(args)
            .current_dir(self.fixture.root())
            .env("LAZYSPEC_STATE_DIR", self.state.path())
            .output()
            .unwrap()
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let output = self.run(args);
        serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
            panic!(
                "{e}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })
    }
}

fn rules(findings: &serde_json::Value) -> Vec<&str> {
    findings
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["rule"].as_str().unwrap())
        .collect()
}

#[test]
fn untrusted_hooks_do_not_run_and_one_warning_names_hook_trust() {
    let project = HookProject::new();

    let out = project.json(&["validate", "--json"]);

    assert!(!rules(&out["errors"]).contains(&"hook"));
    assert_eq!(
        rules(&out["warnings"])
            .iter()
            .filter(|r| **r == "hooks-untrusted")
            .count(),
        1
    );
    assert!(out["warnings"].to_string().contains("hook trust"));
}

#[test]
fn trusted_hook_findings_reach_validate_and_status_naming_the_hook() {
    let project = HookProject::new();
    assert!(project.run(&["hook", "trust"]).status.success());

    let validate = project.json(&["validate", "--json"]);
    let finding = validate["errors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["rule"] == "hook")
        .expect("the hook's finding is in validate");
    assert_eq!(finding["hook"], "required-sections");
    assert_eq!(finding["id"], "RFC-001");
    assert_eq!(finding["line"], 3);
    assert!(finding["message"]
        .as_str()
        .unwrap()
        .contains("required-sections"));
    assert!(!project.run(&["validate"]).status.success());

    let status = project.json(&["status", "--json"]);
    assert!(rules(&status["validation"]["errors"]).contains(&"hook"));
}

#[test]
fn no_hooks_skips_every_hook_and_the_trust_warning() {
    let project = HookProject::new();
    project.run(&["hook", "trust"]);

    let out = project.json(&["validate", "--json", "--no-hooks"]);

    assert!(!rules(&out["errors"]).contains(&"hook"));
}

#[test]
fn editing_the_hook_script_makes_it_untrusted_again() {
    let project = HookProject::new();
    project.run(&["hook", "trust"]);
    let script = project.fixture.root().join(".lazyspec/hooks/lint.sh");
    std::fs::write(&script, "echo '{\"findings\":[]}'\n").unwrap();

    let out = project.json(&["validate", "--json"]);

    assert!(rules(&out["warnings"]).contains(&"hooks-untrusted"));
}

#[test]
fn hook_list_shows_name_event_scope_and_trust_state() {
    let project = HookProject::new();

    let before = project.json(&["hook", "list", "--json"]);
    assert_eq!(before[0]["name"], "required-sections");
    assert_eq!(before[0]["event"], "validate");
    assert_eq!(before[0]["scope"]["types"], serde_json::json!(["rfc"]));
    assert_eq!(before[0]["trust"], "untrusted");

    project.run(&["hook", "trust"]);

    assert_eq!(
        project.json(&["hook", "list", "--json"])[0]["trust"],
        "trusted"
    );
}
