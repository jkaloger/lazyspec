use anyhow::Result;
use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// How long a killed child's pipes are waited on for what it printed. A
/// grandchild holding them open must not turn a timeout into a hang.
const DRAIN_GRACE: Duration = Duration::from_millis(500);

/// The child outlived its timeout and was killed. Carries what it printed first,
/// so a caller can show why it was stuck.
#[derive(Debug)]
pub struct TimedOut {
    pub timeout: Duration,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl std::fmt::Display for TimedOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "process timed out after {:?}", self.timeout)
    }
}

impl std::error::Error for TimedOut {}

// Run `command` to completion, killing it if it outlives `timeout`. stdin is
// closed and stdout/stderr are drained on their own threads so a chatty child
// can't deadlock on a full pipe while we wait. Used for the network-facing
// git/gh calls in the sync path so a slow or auth-prompting remote can't wedge
// the poll thread indefinitely (BUG-001).
pub fn output_with_timeout(command: Command, timeout: Duration) -> Result<Output> {
    run_with_timeout(command, None, timeout)
}

/// [`output_with_timeout`] with `input` written to the child's stdin, then
/// closed. The write is on its own thread for the same reason the reads are: a
/// child that prints before it has read everything would otherwise deadlock
/// against a large payload (RFC-075 hooks read the whole document set).
pub fn output_with_timeout_and_input(
    command: Command,
    input: Vec<u8>,
    timeout: Duration,
) -> Result<Output> {
    run_with_timeout(command, Some(input), timeout)
}

/// A pipe read on its own thread into a buffer the owner can also read before the
/// pipe closes, which a timed-out child's may never do.
struct Drain {
    buf: Arc<Mutex<Vec<u8>>>,
    done: mpsc::Receiver<()>,
}

impl Drain {
    fn start(mut pipe: impl Read + Send + 'static) -> Self {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let (tx, done) = mpsc::channel();
        let sink = Arc::clone(&buf);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            while let Ok(n) = pipe.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                sink.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .extend_from_slice(&chunk[..n]);
            }
            let _ = tx.send(());
        });
        Self { buf, done }
    }

    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.buf.lock().unwrap_or_else(PoisonError::into_inner))
    }

    fn finish(self) -> Vec<u8> {
        let _ = self.done.recv();
        self.take()
    }

    fn partial(self) -> Vec<u8> {
        let _ = self.done.recv_timeout(DRAIN_GRACE);
        self.take()
    }
}

fn run_with_timeout(
    mut command: Command,
    input: Option<Vec<u8>>,
    timeout: Duration,
) -> Result<Output> {
    let stdin = if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    };
    let mut child = command
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let (Some(input), Some(mut in_pipe)) = (input, child.stdin.take()) {
        std::thread::spawn(move || {
            // A child that exits without reading closes the pipe; that is its
            // answer to give, not a write error to raise.
            let _ = in_pipe.write_all(&input);
        });
    }

    let out = Drain::start(child.stdout.take().expect("stdout piped"));
    let err = Drain::start(child.stderr.take().expect("stderr piped"));

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(TimedOut {
                timeout,
                stdout: out.partial(),
                stderr: err.partial(),
            }
            .into());
        }
        std::thread::sleep(POLL_INTERVAL);
    };

    let stdout = out.finish();
    let stderr = err.finish();
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_output_for_fast_command() {
        let mut cmd = Command::new("echo");
        cmd.arg("hello");
        let out = output_with_timeout(cmd, Duration::from_secs(5)).unwrap();
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello");
    }

    #[test]
    fn kills_command_that_exceeds_timeout() {
        let mut cmd = Command::new("sleep");
        cmd.arg("10");
        let start = Instant::now();
        let result = output_with_timeout(cmd, Duration::from_millis(200));
        assert!(result.is_err(), "a command past its timeout must error");
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "must return promptly after the timeout, not wait out the child"
        );
    }

    #[test]
    fn a_timeout_carries_what_the_child_printed_to_stderr() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo stuck-here >&2; sleep 1"]);
        let err = output_with_timeout(cmd, Duration::from_millis(150)).unwrap_err();
        let timed_out = err.downcast_ref::<TimedOut>().expect("a typed timeout");
        assert!(String::from_utf8_lossy(&timed_out.stderr).contains("stuck-here"));
    }

    #[test]
    fn feeds_input_to_the_child_on_stdin() {
        let out = output_with_timeout_and_input(
            Command::new("cat"),
            b"payload".to_vec(),
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "payload");
    }
}
