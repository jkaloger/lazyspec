use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crossbeam_channel::{bounded, Receiver, Sender};

use crate::engine::ipc::broadcaster::Broadcaster;
use crate::engine::ipc::protocol::AgentSnapshot;

pub trait SnapshotProvider: Send + Sync {
    fn snapshot(&self) -> Vec<AgentSnapshot>;
}

pub struct StaticSnapshotProvider(pub Vec<AgentSnapshot>);

impl SnapshotProvider for StaticSnapshotProvider {
    fn snapshot(&self) -> Vec<AgentSnapshot> {
        self.0.clone()
    }
}

pub struct DaemonState {
    pub cancel_map: Arc<Mutex<HashMap<String, Sender<()>>>>,
    pub snapshot_provider: Arc<dyn SnapshotProvider>,
    pub broadcaster: Broadcaster,
    pub wake: Sender<()>,
}

pub fn wake_channel() -> (Sender<()>, Receiver<()>) {
    bounded(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::TrySendError;
    use std::time::Duration;

    fn snap(agent_id: &str) -> AgentSnapshot {
        AgentSnapshot {
            agent_id: agent_id.into(),
            session_id: format!("{agent_id}-s"),
            doc_id: "STORY-1".into(),
            elapsed_ms: 0,
            tokens_in: 0,
            tokens_out: 0,
        }
    }

    #[test]
    fn static_snapshot_provider_returns_zero_for_empty() {
        let provider = StaticSnapshotProvider(vec![]);
        assert!(provider.snapshot().is_empty());
    }

    #[test]
    fn static_snapshot_provider_returns_one() {
        let provider = StaticSnapshotProvider(vec![snap("a1")]);
        let got = provider.snapshot();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].agent_id, "a1");
    }

    #[test]
    fn static_snapshot_provider_returns_n() {
        let provider = StaticSnapshotProvider(vec![snap("a1"), snap("a2"), snap("a3")]);
        let got = provider.snapshot();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].agent_id, "a1");
        assert_eq!(got[1].agent_id, "a2");
        assert_eq!(got[2].agent_id, "a3");
    }

    #[test]
    fn wake_channel_collapses_multiple_sends() {
        let (tx, rx) = wake_channel();

        tx.try_send(()).expect("first send fits");
        match tx.try_send(()) {
            Err(TrySendError::Full(())) => {}
            other => panic!("expected Full, got {other:?}"),
        }

        rx.recv_timeout(Duration::from_secs(1))
            .expect("first wake drained");

        tx.try_send(()).expect("send after drain fits");
    }

    #[test]
    fn daemon_state_constructible() {
        let (wake_tx, _wake_rx) = wake_channel();
        let state = DaemonState {
            cancel_map: Arc::new(Mutex::new(HashMap::new())),
            snapshot_provider: Arc::new(StaticSnapshotProvider(vec![snap("a1")])),
            broadcaster: Broadcaster::new(),
            wake: wake_tx,
        };

        assert_eq!(state.snapshot_provider.snapshot().len(), 1);
        assert_eq!(state.broadcaster.sub_count(), 0);
        assert!(state.cancel_map.lock().unwrap().is_empty());
        let _ = state.wake.try_send(());
    }
}
