use std::path::Path;
use std::process::Command;

/// Build the foreground interactive Command from the configured `[agents] interactive`
/// shell string. Run via `bash -lc`; the rendered prompt + doc path are passed by
/// environment (LAZYSPEC_PROMPT / LAZYSPEC_DOC_PATH) so the command references them
/// without the engine shell-quoting rendered markdown. The engine never touches
/// terminal state (convention 3); the caller (TUI) owns suspend/restore. Single
/// configured behaviour -> no trait (convention 6).
pub fn build_interactive_command(cmd: &str, prompt: &str, doc_path: &Path) -> Command {
    let mut command = Command::new("bash");
    command
        .arg("-lc")
        .arg(cmd)
        .env("LAZYSPEC_PROMPT", prompt)
        .env("LAZYSPEC_DOC_PATH", doc_path);
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::Path;

    fn args_of(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn env_value(command: &Command, key: &str) -> Option<String> {
        command
            .get_envs()
            .find(|(k, _)| *k == OsStr::new(key))
            .and_then(|(_, v)| v)
            .map(|v| v.to_string_lossy().into_owned())
    }

    // AC3: the built Command has program `bash`, args `["-lc", cmd]`, and the
    // rendered prompt + doc path exported by environment.
    #[test]
    fn interactive_command_program_args_env() {
        let command = build_interactive_command(
            r#"claude "$LAZYSPEC_PROMPT""#,
            "hello body",
            Path::new("/tmp/x.md"),
        );

        assert_eq!(command.get_program(), OsStr::new("bash"));
        assert_eq!(
            args_of(&command),
            vec![
                "-lc".to_string(),
                r#"claude "$LAZYSPEC_PROMPT""#.to_string()
            ]
        );
        assert_eq!(
            env_value(&command, "LAZYSPEC_PROMPT").as_deref(),
            Some("hello body")
        );
        assert_eq!(
            env_value(&command, "LAZYSPEC_DOC_PATH").as_deref(),
            Some("/tmp/x.md")
        );
    }

    // AC6: an arbitrary custom/tmux wrapper passes through verbatim as the `-lc`
    // arg, with both env vars still carried.
    #[test]
    fn interactive_command_custom_tmux() {
        let command = build_interactive_command(
            r#"tmux new-window claude "$LAZYSPEC_PROMPT""#,
            "render",
            Path::new("/tmp/y.md"),
        );

        assert_eq!(command.get_program(), OsStr::new("bash"));
        assert_eq!(
            args_of(&command),
            vec![
                "-lc".to_string(),
                r#"tmux new-window claude "$LAZYSPEC_PROMPT""#.to_string()
            ]
        );
        assert_eq!(
            env_value(&command, "LAZYSPEC_PROMPT").as_deref(),
            Some("render")
        );
        assert_eq!(
            env_value(&command, "LAZYSPEC_DOC_PATH").as_deref(),
            Some("/tmp/y.md")
        );
    }

    // AC7: interactive ignores allowed_tools -- the builder signature cannot pass it.
    // Even when a template carries `allowed_tools = Some(...)`, the only inputs the
    // builder accepts are the cmd, the prompt, and the doc path; the resulting
    // Command therefore carries no `--allowedTools` arg and no tools string anywhere.
    #[test]
    fn interactive_command_ignores_allowed_tools() {
        let template_allowed_tools: Option<Vec<String>> =
            Some(vec!["Read".to_string(), "Edit".to_string()]);
        // The builder takes no allowed_tools param; the template's policy is dropped.
        let _ = &template_allowed_tools;

        let command = build_interactive_command(
            r#"claude "$LAZYSPEC_PROMPT""#,
            "body",
            Path::new("/tmp/z.md"),
        );

        let args = args_of(&command);
        assert!(
            !args.iter().any(|a| a.contains("--allowedTools")),
            "interactive command must not carry an --allowedTools flag, got: {args:?}"
        );
        assert!(
            !args
                .iter()
                .any(|a| a.contains("Read") || a.contains("Edit")),
            "interactive command must not carry the template's tools, got: {args:?}"
        );

        for key in ["LAZYSPEC_PROMPT", "LAZYSPEC_DOC_PATH"] {
            let value = env_value(&command, key).unwrap_or_default();
            assert!(
                !value.contains("Read") && !value.contains("Edit"),
                "env {key} must not carry the template's tools, got: {value:?}"
            );
        }
    }
}
