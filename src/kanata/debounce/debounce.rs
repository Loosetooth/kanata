use crate::kanata::KeyEvent;
use std::sync::mpsc::{self, SyncSender};

/// Trait for debounce algorithms
pub trait Debounce {
    fn event_preprocessor(&mut self, preprocess_rx: mpsc::Receiver<KeyEvent>);
}

/// Helper function to send events and panic on failure
pub fn try_send_panic(tx: &SyncSender<KeyEvent>, kev: KeyEvent) {
    if let Err(e) = tx.try_send(kev) {
        panic!("failed to send on channel: {e:?}");
    }
}
