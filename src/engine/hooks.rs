use std::collections::HashMap;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct HookSpec {
    pub script: String,
    // stored for v1; Task 3 enforces the timeout
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct HookEnv {
    pub doc_id: String,
    pub doc_type: String,
    pub agent_id: String,
    pub branch: String,
    pub workspace: PathBuf,
}

impl HookEnv {
    fn env_map(&self) -> HashMap<&'static str, String> {
        let mut m = HashMap::with_capacity(5);
        m.insert("LAZYSPEC_DOC_ID", self.doc_id.clone());
        m.insert("LAZYSPEC_DOC_TYPE", self.doc_type.clone());
        m.insert("LAZYSPEC_AGENT_ID", self.agent_id.clone());
        m.insert("LAZYSPEC_BRANCH", self.branch.clone());
        m.insert(
            "LAZYSPEC_WORKSPACE",
            self.workspace.to_string_lossy().into_owned(),
        );
        m
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    Ok,
    NonZero(i32),
    Timeout,
    SpawnFailed(String),
}

pub trait HookRunner {
    fn run(&self, spec: &HookSpec, env: &HookEnv) -> HookOutcome;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookPoint {
    AfterCreate,
    BeforeRun,
    AfterRun,
    BeforeRemove,
}

impl HookPoint {
    pub fn is_fatal(self) -> bool {
        matches!(self, Self::AfterCreate | Self::BeforeRun)
    }
}

#[derive(Debug)]
pub enum HookError {
    NonZero { point: HookPoint, code: i32 },
    Timeout { point: HookPoint },
    SpawnFailed { point: HookPoint, msg: String },
}

impl std::fmt::Display for HookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HookError::NonZero { point, code } => {
                write!(f, "hook {point:?} exited with code {code}")
            }
            HookError::Timeout { point } => write!(f, "hook {point:?} timed out"),
            HookError::SpawnFailed { point, msg } => {
                write!(f, "hook {point:?} failed to spawn: {msg}")
            }
        }
    }
}

impl std::error::Error for HookError {}

pub fn run_hook(
    point: HookPoint,
    runner: &dyn HookRunner,
    spec: &HookSpec,
    env: &HookEnv,
) -> Result<(), HookError> {
    let outcome = runner.run(spec, env);
    match outcome {
        HookOutcome::Ok => Ok(()),
        other if point.is_fatal() => Err(match other {
            HookOutcome::NonZero(code) => HookError::NonZero { point, code },
            HookOutcome::Timeout => HookError::Timeout { point },
            HookOutcome::SpawnFailed(msg) => HookError::SpawnFailed { point, msg },
            HookOutcome::Ok => unreachable!(),
        }),
        other => {
            eprintln!("hook {point:?} failed: {other:?}");
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BashHookRunner;

impl HookRunner for BashHookRunner {
    fn run(&self, spec: &HookSpec, env: &HookEnv) -> HookOutcome {
        let spawn = Command::new("bash")
            .arg("-lc")
            .arg(&spec.script)
            .current_dir(&env.workspace)
            .envs(env.env_map())
            .process_group(0)
            .spawn();

        let mut child = match spawn {
            Ok(c) => c,
            Err(e) => return HookOutcome::SpawnFailed(e.to_string()),
        };

        let pid = child.id() as i32;
        let deadline = Instant::now() + spec.timeout;

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return match status.code() {
                        Some(0) => HookOutcome::Ok,
                        Some(c) => HookOutcome::NonZero(c),
                        None => HookOutcome::NonZero(-1),
                    };
                }
                Ok(None) => {
                    if Instant::now() >= deadline {
                        // SAFETY: pid was just obtained from Child::id() and refers to a
                        // process we spawned into its own group; signalling -pid targets
                        // the whole group so subshells (e.g. `sleep` under `bash -lc`)
                        // also receive the signal. ESRCH is harmless and ignored.
                        unsafe {
                            libc::kill(-pid, libc::SIGTERM);
                        }

                        let grace_deadline = Instant::now() + Duration::from_millis(200);
                        let mut exited = false;
                        while Instant::now() < grace_deadline {
                            if let Ok(Some(_)) = child.try_wait() {
                                exited = true;
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        }

                        if !exited {
                            // SAFETY: same conditions as the SIGTERM above; SIGKILL is
                            // delivered to the process group to reap stubborn subshells.
                            unsafe {
                                libc::kill(-pid, libc::SIGKILL);
                            }
                        }

                        let _ = child.wait();
                        return HookOutcome::Timeout;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return HookOutcome::SpawnFailed(format!("wait: {e}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn env_for(ws: &TempDir) -> HookEnv {
        HookEnv {
            doc_id: "STORY-127".into(),
            doc_type: "story".into(),
            agent_id: "claude-bot".into(),
            branch: "feat/orchestration".into(),
            workspace: ws.path().to_path_buf(),
        }
    }

    fn spec(script: &str) -> HookSpec {
        HookSpec {
            script: script.into(),
            timeout: Duration::from_secs(30),
        }
    }

    #[test]
    fn run_script_exit_zero_returns_ok() {
        let ws = TempDir::new().unwrap();
        let runner = BashHookRunner;
        assert_eq!(runner.run(&spec("exit 0"), &env_for(&ws)), HookOutcome::Ok);
    }

    #[test]
    fn run_script_exit_nonzero_returns_nonzero() {
        let ws = TempDir::new().unwrap();
        let runner = BashHookRunner;
        assert_eq!(
            runner.run(&spec("exit 42"), &env_for(&ws)),
            HookOutcome::NonZero(42)
        );
    }

    // SpawnFailed is unreachable when bash is on PATH and the impl hard-codes "bash".
    // Task spec says skip; documented here for the reader.

    #[test]
    fn run_injects_env_vars() {
        let ws = TempDir::new().unwrap();
        let runner = BashHookRunner;
        let script = r#"printenv | grep ^LAZYSPEC_ > "$LAZYSPEC_WORKSPACE/out""#;
        let env = env_for(&ws);
        assert_eq!(runner.run(&spec(script), &env), HookOutcome::Ok);

        let out = fs::read_to_string(ws.path().join("out")).unwrap();
        assert!(out.contains("LAZYSPEC_DOC_ID=STORY-127"), "got: {out}");
        assert!(out.contains("LAZYSPEC_DOC_TYPE=story"), "got: {out}");
        assert!(out.contains("LAZYSPEC_AGENT_ID=claude-bot"), "got: {out}");
        assert!(
            out.contains("LAZYSPEC_BRANCH=feat/orchestration"),
            "got: {out}"
        );
        assert!(
            out.contains(&format!(
                "LAZYSPEC_WORKSPACE={}",
                ws.path().to_string_lossy()
            )),
            "got: {out}"
        );
    }

    #[test]
    fn run_cwd_is_workspace() {
        let ws = TempDir::new().unwrap();
        let runner = BashHookRunner;
        assert_eq!(
            runner.run(&spec("pwd > out"), &env_for(&ws)),
            HookOutcome::Ok
        );

        let pwd = fs::read_to_string(ws.path().join("out")).unwrap();
        let canon_ws = fs::canonicalize(ws.path()).unwrap();
        let canon_pwd = fs::canonicalize(pwd.trim()).unwrap();
        assert_eq!(canon_pwd, canon_ws);
    }

    struct FakeRunner {
        outcome: HookOutcome,
    }

    impl HookRunner for FakeRunner {
        fn run(&self, _: &HookSpec, _: &HookEnv) -> HookOutcome {
            self.outcome.clone()
        }
    }

    const ALL_POINTS: [HookPoint; 4] = [
        HookPoint::AfterCreate,
        HookPoint::BeforeRun,
        HookPoint::AfterRun,
        HookPoint::BeforeRemove,
    ];

    fn dummy_env() -> HookEnv {
        HookEnv {
            doc_id: "x".into(),
            doc_type: "story".into(),
            agent_id: "a".into(),
            branch: "b".into(),
            workspace: PathBuf::from("/tmp"),
        }
    }

    #[test]
    fn is_fatal_classification() {
        assert!(HookPoint::AfterCreate.is_fatal());
        assert!(HookPoint::BeforeRun.is_fatal());
        assert!(!HookPoint::AfterRun.is_fatal());
        assert!(!HookPoint::BeforeRemove.is_fatal());
    }

    #[test]
    fn ok_outcome_always_returns_ok() {
        let runner = FakeRunner {
            outcome: HookOutcome::Ok,
        };
        let s = spec("noop");
        let env = dummy_env();
        for p in ALL_POINTS {
            assert!(run_hook(p, &runner, &s, &env).is_ok());
        }
    }

    #[test]
    fn fatal_point_nonzero_returns_err() {
        let runner = FakeRunner {
            outcome: HookOutcome::NonZero(1),
        };
        let err = run_hook(HookPoint::AfterCreate, &runner, &spec("x"), &dummy_env()).unwrap_err();
        assert!(matches!(
            err,
            HookError::NonZero {
                point: HookPoint::AfterCreate,
                code: 1
            }
        ));
    }

    #[test]
    fn fatal_point_timeout_returns_err() {
        let runner = FakeRunner {
            outcome: HookOutcome::Timeout,
        };
        let err = run_hook(HookPoint::BeforeRun, &runner, &spec("x"), &dummy_env()).unwrap_err();
        assert!(matches!(
            err,
            HookError::Timeout {
                point: HookPoint::BeforeRun
            }
        ));
    }

    #[test]
    fn fatal_point_spawn_failed_returns_err() {
        let runner = FakeRunner {
            outcome: HookOutcome::SpawnFailed("boom".into()),
        };
        let err = run_hook(HookPoint::AfterCreate, &runner, &spec("x"), &dummy_env()).unwrap_err();
        match err {
            HookError::SpawnFailed { point, msg } => {
                assert_eq!(point, HookPoint::AfterCreate);
                assert_eq!(msg, "boom");
            }
            other => panic!("expected SpawnFailed, got {other:?}"),
        }
    }

    #[test]
    fn nonfatal_point_nonzero_returns_ok() {
        let runner = FakeRunner {
            outcome: HookOutcome::NonZero(1),
        };
        assert!(run_hook(HookPoint::AfterRun, &runner, &spec("x"), &dummy_env()).is_ok());
    }

    #[test]
    fn nonfatal_point_timeout_returns_ok() {
        let runner = FakeRunner {
            outcome: HookOutcome::Timeout,
        };
        assert!(run_hook(HookPoint::BeforeRemove, &runner, &spec("x"), &dummy_env()).is_ok());
    }

    #[test]
    fn nonfatal_point_spawn_failed_returns_ok() {
        let runner = FakeRunner {
            outcome: HookOutcome::SpawnFailed("boom".into()),
        };
        assert!(run_hook(HookPoint::AfterRun, &runner, &spec("x"), &dummy_env()).is_ok());
    }

    fn timed_spec(script: &str, timeout: Duration) -> HookSpec {
        HookSpec {
            script: script.into(),
            timeout,
        }
    }

    #[test]
    fn timeout_kills_long_script() {
        let ws = TempDir::new().unwrap();
        let runner = BashHookRunner;
        let start = Instant::now();
        let outcome = runner.run(
            &timed_spec("sleep 5", Duration::from_millis(100)),
            &env_for(&ws),
        );
        let elapsed = start.elapsed();
        assert_eq!(outcome, HookOutcome::Timeout);
        assert!(elapsed < Duration::from_millis(500), "elapsed={elapsed:?}");
    }

    #[test]
    fn non_timeout_path_still_works() {
        let ws = TempDir::new().unwrap();
        let runner = BashHookRunner;
        assert_eq!(
            runner.run(
                &timed_spec("exit 0", Duration::from_secs(10)),
                &env_for(&ws)
            ),
            HookOutcome::Ok
        );
    }

    #[test]
    fn nonzero_exit_path_still_works() {
        let ws = TempDir::new().unwrap();
        let runner = BashHookRunner;
        assert_eq!(
            runner.run(
                &timed_spec("exit 7", Duration::from_secs(10)),
                &env_for(&ws)
            ),
            HookOutcome::NonZero(7)
        );
    }

    #[test]
    fn timeout_with_sigterm_resistant_script() {
        let ws = TempDir::new().unwrap();
        let runner = BashHookRunner;
        let start = Instant::now();
        let outcome = runner.run(
            &timed_spec("trap '' TERM; sleep 5", Duration::from_millis(100)),
            &env_for(&ws),
        );
        let elapsed = start.elapsed();
        assert_eq!(outcome, HookOutcome::Timeout);
        assert!(elapsed < Duration::from_millis(500), "elapsed={elapsed:?}");
    }
}
