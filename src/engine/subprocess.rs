use anyhow::{bail, Result};
use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(50);

// Run `command` to completion, killing it if it outlives `timeout`. stdin is
// closed and stdout/stderr are drained on their own threads so a chatty child
// can't deadlock on a full pipe while we wait. Used for the network-facing
// git/gh calls in the sync path so a slow or auth-prompting remote can't wedge
// the poll thread indefinitely (BUG-001).
pub fn output_with_timeout(mut command: Command, timeout: Duration) -> Result<Output> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut out_pipe = child.stdout.take().expect("stdout piped");
    let mut err_pipe = child.stderr.take().expect("stderr piped");
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut buf);
        buf
    });

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            bail!("process timed out after {:?}", timeout);
        }
        std::thread::sleep(POLL_INTERVAL);
    };

    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
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
}
