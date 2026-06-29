//! Small unbounded event stream used by provider implementations.

use tokio::sync::mpsc;

/// Receiver side for streamed provider events.
pub struct EventStream<T> {
    rx: mpsc::UnboundedReceiver<T>,
}

/// Sender side used by provider tasks to emit stream events.
#[derive(Clone)]
pub struct EventStreamSender<T> {
    tx: mpsc::UnboundedSender<T>,
}

/// Create a sender/receiver pair for provider stream events.
pub fn create_event_stream<T>() -> (EventStreamSender<T>, EventStream<T>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (EventStreamSender { tx }, EventStream { rx })
}

impl<T> EventStreamSender<T> {
    /// Push one event to the stream and ignore closed receivers.
    pub fn push(&self, event: T) {
        let _ = self.tx.send(event);
    }
}

impl<T> EventStream<T> {
    /// Wait for the next provider event, returning `None` after the sender closes.
    pub async fn next(&mut self) -> Option<T> {
        self.rx.recv().await
    }
}
