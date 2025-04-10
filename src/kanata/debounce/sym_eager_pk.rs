use std::collections::HashMap;
use std::time::{Duration, Instant};
use crate::kanata::{KeyEvent, OsCode};
use std::sync::mpsc::SyncSender as Sender;
use crate::kanata::debounce::debounce::try_send_panic;

/// Implementation of the sym_eager_pk algorithm
/// See: https://github.com/qmk/qmk_firmware/blob/6ef97172889ccd5db376b2a9f8825489e24fdac4/docs/feature_debounce_type.md
/// Debouncing per key. On any state change, response is immediate,
/// followed by debounce_duration milliseconds of no further input for that key.
pub struct SymEagerPk {
    debounce_duration: Duration,
    last_event_time: HashMap<OsCode, Instant>, // Tracks the last event time for each key
}

impl SymEagerPk {
    pub fn new(debounce_duration_ms: u16) -> Self {
        Self {
            debounce_duration: Duration::from_millis(debounce_duration_ms.into()),
            last_event_time: HashMap::new(),
        }
    }
}

impl crate::kanata::debounce::debounce::Debounce for SymEagerPk {
    fn process_event(&mut self, event: KeyEvent, process_tx: &Sender<KeyEvent>) {
        let now = Instant::now();
        let oscode = event.code;

        // Check if the key is within the debounce duration
        if let Some(&last_time) = self.last_event_time.get(&oscode) {
            if now.duration_since(last_time) < self.debounce_duration {
                log::info!(
                    "Debouncing event for {:?} (elapsed: {:?}, required: {:?})",
                    oscode,
                    now.duration_since(last_time),
                    self.debounce_duration
                );
                return; // Skip processing this event
            }
        }

        // Process the event immediately
        log::info!("Processing event for {:?}: {:?}", oscode, event.value);
        try_send_panic(process_tx, event);

        // Update the last event time for the key
        self.last_event_time.insert(oscode, now);
    }
}