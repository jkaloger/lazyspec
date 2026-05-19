use std::sync::{Arc, Mutex};

use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::engine::ipc::protocol::DaemonMessage;

#[derive(Clone, Default)]
pub struct Broadcaster {
    subs: Arc<Mutex<Vec<Sender<DaemonMessage>>>>,
}

impl Broadcaster {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&self) -> Receiver<DaemonMessage> {
        let (tx, rx) = unbounded();
        self.subs
            .lock()
            .expect("broadcaster mutex poisoned")
            .push(tx);
        rx
    }

    pub fn publish(&self, msg: DaemonMessage) {
        let mut subs = self.subs.lock().expect("broadcaster mutex poisoned");
        subs.retain(|tx| tx.send(msg.clone()).is_ok());
    }

    pub fn sub_count(&self) -> usize {
        self.subs.lock().expect("broadcaster mutex poisoned").len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::runner::AgentEvent;
    use std::time::Duration;

    fn sample_event(delta: &str) -> DaemonMessage {
        DaemonMessage::AgentEvent {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            doc_id: "STORY-1".into(),
            event: AgentEvent::Text {
                delta: delta.into(),
            },
        }
    }

    #[test]
    fn publish_with_no_subs_is_noop() {
        let bc = Broadcaster::new();
        bc.publish(sample_event("hi"));
        assert_eq!(bc.sub_count(), 0);
    }

    #[test]
    fn two_subs_each_receive_published_event() {
        let bc = Broadcaster::new();
        let rx1 = bc.subscribe();
        let rx2 = bc.subscribe();

        let msg = sample_event("hello");
        bc.publish(msg.clone());

        let got1 = rx1
            .recv_timeout(Duration::from_secs(2))
            .expect("rx1 should receive");
        let got2 = rx2
            .recv_timeout(Duration::from_secs(2))
            .expect("rx2 should receive");

        assert_eq!(got1, msg);
        assert_eq!(got2, msg);
    }

    #[test]
    fn dropped_sub_does_not_block_publish() {
        let bc = Broadcaster::new();
        let rx1 = bc.subscribe();
        let rx2 = bc.subscribe();
        assert_eq!(bc.sub_count(), 2);

        drop(rx1);

        let msg = sample_event("ping");
        bc.publish(msg.clone());

        let got = rx2
            .recv_timeout(Duration::from_secs(2))
            .expect("remaining rx should receive");
        assert_eq!(got, msg);
        assert_eq!(bc.sub_count(), 1);
    }

    #[test]
    fn unsubscribe_drops_sink() {
        let bc = Broadcaster::new();
        let rx = bc.subscribe();
        assert_eq!(bc.sub_count(), 1);

        drop(rx);
        bc.publish(sample_event("bye"));

        assert_eq!(bc.sub_count(), 0);
    }
}
