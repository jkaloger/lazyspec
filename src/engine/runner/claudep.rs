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

impl AgentRunner for ClaudeP {
    fn spawn(&self, _ctx: AgentContext) -> Result<AgentHandle> {
        let mut child = Command::new(&self.binary)
            .arg("-p")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--allowedTools")
            .arg(&self.allowed_tools)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawn {}", self.binary))?;

        let pid = child.id();
        let stdout = child.stdout.take().context("child stdout missing")?;

        let (events_tx, events_rx): (Sender<AgentEvent>, Receiver<AgentEvent>) = unbounded();
        let (cancel_tx, cancel_rx) = bounded::<()>(1);

        let reader_tx = events_tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if let Some(ev) = parse_record(&line) {
                    if reader_tx.send(ev).is_err() {
                        break;
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
