//! `lazyspec daemon` — thin CLI wrapper around [`engine::daemon::Daemon`].
//!
//! Translates SIGTERM/SIGINT into a send on a bounded shutdown channel and
//! delegates the actual run loop to the engine. Foreground only; no fork, no
//! PID file. Run under a process supervisor.

use std::path::Path;
use std::thread;

use anyhow::Result;
use crossbeam_channel::bounded;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use crate::engine::config::Config;
use crate::engine::daemon::Daemon;

/// Build the daemon, install signal handlers for SIGTERM/SIGINT, and block on
/// `Daemon::run` until a signal arrives. The first signal triggers shutdown;
/// subsequent signals are drained (idempotent).
pub fn run(root: &Path, config: &Config) -> Result<()> {
    let mut daemon = Daemon::new(root, config)?;

    let (shutdown_tx, shutdown_rx) = bounded::<()>(1);

    let mut signals = Signals::new([SIGTERM, SIGINT])?;
    thread::spawn(move || {
        let mut fired = false;
        for _sig in signals.forever() {
            if !fired {
                let _ = shutdown_tx.send(());
                fired = true;
            }
        }
    });

    daemon.run(shutdown_rx)
}
