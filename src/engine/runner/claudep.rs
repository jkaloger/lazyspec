use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::thread;

use anyhow::{Context, Result};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};

use super::stream::parse_record;
use super::{AgentContext, AgentEvent, AgentHandle, AgentRunner};

pub struct ClaudeP {
    pub binary: String,
    pub allowed_tools: String,
    // stored for v1; enforcement is a later iteration
    pub turn_timeout_ms: u64,
}

impl ClaudeP {
    fn build_command(&self, ctx: &AgentContext) -> Command {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("-p")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--allowedTools")
            .arg(&self.allowed_tools)
            .current_dir(&ctx.workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }
}

impl AgentRunner for ClaudeP {
    fn spawn(&self, ctx: AgentContext) -> Result<AgentHandle> {
        let mut child = self
            .build_command(&ctx)
            .spawn()
            .with_context(|| format!("spawn {}", self.binary))?;

        let pid = child.id();

        if let Some(mut stdin) = child.stdin.take() {
            let prompt = ctx.prompt.clone();
            thread::spawn(move || {
                let _ = write_prompt(&mut stdin, &prompt);
            });
        }

        let stdout = child.stdout.take().context("child stdout missing")?;

        let (events_tx, events_rx): (Sender<AgentEvent>, Receiver<AgentEvent>) = unbounded();
        let (cancel_tx, cancel_rx) = bounded::<()>(1);

        let reader_tx = events_tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            'lines: for line in reader.lines().map_while(Result::ok) {
                for ev in parse_record(&line) {
                    if reader_tx.send(ev).is_err() {
                        break 'lines;
                    }
                }
            }
        });

        thread::spawn(move || {
            // exits cleanly when the handle drops the cancel sender (Disconnected)
            if cancel_rx.recv().is_ok() {
                send_sigterm(pid);
            }
        });

        thread::spawn(move || {
            let code = child.wait().ok().and_then(|s| s.code());
            let _ = events_tx.send(AgentEvent::SubprocessExited { code });
        });

        Ok(AgentHandle {
            pid,
            events: events_rx,
            cancel: cancel_tx,
        })
    }
}

fn send_sigterm(pid: u32) {
    // SAFETY: `kill` with SIGTERM is a stable POSIX syscall; pid is a valid
    // process id captured from Child::id(). Worst case the target has already
    // exited and kill returns ESRCH, which we ignore.
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
}

fn write_prompt(w: &mut impl std::io::Write, prompt: &str) -> std::io::Result<()> {
    w.write_all(prompt.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_prompt_writes_bytes_to_writer() {
        let mut buf: Vec<u8> = Vec::new();
        write_prompt(&mut buf, "hello").unwrap();
        assert_eq!(buf, b"hello");
    }

    #[test]
    fn write_prompt_handles_empty_string() {
        let mut buf: Vec<u8> = Vec::new();
        write_prompt(&mut buf, "").unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn build_command_sets_current_dir_to_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let runner = ClaudeP {
            binary: "sh".into(),
            allowed_tools: "Read".into(),
            turn_timeout_ms: 0,
        };
        let ctx = AgentContext {
            workspace: tmp.path().to_path_buf(),
            doc_id: "STORY-1".into(),
            agent_id: "claude-bot".into(),
            branch: "feat/x".into(),
            prompt: "noop".into(),
        };

        let cmd = runner.build_command(&ctx);

        assert_eq!(cmd.get_current_dir(), Some(tmp.path()));
    }

    #[test]
    fn write_prompt_propagates_writer_error() {
        struct FailingWriter;
        impl std::io::Write for FailingWriter {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("nope"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let mut w = FailingWriter;
        let err = write_prompt(&mut w, "data").unwrap_err();
        assert_eq!(err.to_string(), "nope");
    }
}
