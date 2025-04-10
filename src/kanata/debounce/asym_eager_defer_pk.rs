use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::kanata::{KeyEvent, KeyValue, OsCode};
use std::sync::mpsc::{self, Receiver, SyncSender as Sender, TryRecvError};
use std::thread;
use crate::kanata::debounce::debounce::try_send_panic;

/// Implementation of the asym_eager_defer_pk algorithm
/// See: https://github.com/qmk/qmk_firmware/blob/6ef97172889ccd5db376b2a9f8825489e24fdac4/docs/feature_debounce_type.md
pub struct AsymEagerDeferPk {
  debounce_duration: Duration,
  last_key_event_time: HashMap<OsCode, Instant>,
  process_tx: Sender<KeyEvent>,
}

impl AsymEagerDeferPk {
  pub fn new(
      debounce_duration_ms: u16,
      process_tx: Sender<KeyEvent>,
  ) -> Self {
      Self {
          debounce_duration: Duration::from_millis(debounce_duration_ms.into()),
          last_key_event_time: HashMap::new(),
          process_tx
      }
  }
}

impl crate::kanata::debounce::debounce::Debounce for AsymEagerDeferPk {
    fn event_preprocessor(&mut self, preprocess_rx: Receiver<KeyEvent>) {
        let release_queue = Arc::new(std::sync::Mutex::new(ReleaseQueue { queue: Vec::new() }));
        let (release_tx, release_rx) = mpsc::sync_channel(100);

        process_with_timeout(release_queue.clone(), release_rx, self.process_tx.clone());
        loop {
            match preprocess_rx.try_recv() {
                Ok(kev) => {
                    log::info!("Received event: {:?}", kev);
                    self.process_event(kev, &release_queue, &release_tx);
                }
                Err(TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(TryRecvError::Disconnected) => {
                    panic!("channel disconnected");
                }
            }
        }
    }
}

impl AsymEagerDeferPk {
    fn process_event(
        &mut self,
        event: KeyEvent,
        release_queue: &std::sync::Mutex<ReleaseQueue>,
        release_tx: &Sender<KeySnapshot>
    ) {
        let now = Instant::now();
        let oscode = event.code;

        match event.value {
            KeyValue::Press => {
                // Remove the key press from the release queue
                if let Some((event, _)) = release_queue.lock().unwrap().remove_keypress(event.clone()) {
                    log::info!("Removed key press for {:?}", event.code);
                }

                // Check if the key press is within the debounce duration
                if let Some(&last_time) = self.last_key_event_time.get(&oscode) {
                    if now.duration_since(last_time) < self.debounce_duration {
                        log::info!("Debouncing key press for {:?}", oscode);
                        return; // Skip processing this event
                    }
                }

                // Eagerly process key-down events
                self.last_key_event_time.insert(oscode, now);
                try_send_panic(&self.process_tx, event);
            }
            KeyValue::Release => {
                // Reset existing timer for release events
                if let Some((event, _)) = release_queue.lock().unwrap().remove_keypress(event.clone()) {
                    log::info!("Removed key press for {:?}", event.code);
                }

                let timeout = now + self.debounce_duration;
                let release_event = event.clone();

                // Start a new timer for the key release
                release_tx.send((release_event, timeout));
            }
            KeyValue::Repeat => {
                // Forward repeat events immediately
                log::info!("Forwarding repeat event for {:?}", oscode);
                try_send_panic(&self.process_tx, event);
            }
            _ => {
                // Forward other key events without debouncing
                log::debug!("Forwarding other event for {:?}", oscode);
                try_send_panic(&self.process_tx, event);
            }
        }
    }
}

type KeySnapshot = (KeyEvent, std::time::Instant);
pub struct ReleaseQueue {
    queue: Vec<KeySnapshot>
}

impl ReleaseQueue {
    pub fn add_keypress(
        &mut self,
        keypress: KeySnapshot
    ) {
        self.queue.push(keypress);
    }

    pub fn remove_keypress(
        &mut self, event: KeyEvent
    ) -> Option<(KeyEvent, std::time::Instant)> {
        if let Some(pos) = self.queue.iter().position(|(e, _)| e.code == event.code && e.value == event.value) {
            Some(self.queue.remove(pos))
        } else {
            None
        }
    }
}

pub fn process_with_timeout(
    release_queue: Arc<std::sync::Mutex<ReleaseQueue>>,
    release_rx: mpsc::Receiver<KeySnapshot>,
    key_tx: mpsc::SyncSender<KeyEvent>,
) {
    thread::spawn(move || {
        loop {
            match release_rx.recv() {
                Ok(keypress) => {
                    {
                        let mut queue = release_queue.lock().unwrap();
                        queue.add_keypress(keypress);
                    }

                    // Sleep until the first timeout in the queue
                    if let Some((_, release_time)) = release_queue.lock().unwrap().queue.first() {
                        let now = Instant::now();
                        if *release_time > now {
                            thread::sleep(*release_time - now);
                        }
                    }

                    // Send out all events which are timed out
                    let mut queue = release_queue.lock().unwrap();
                    let now = Instant::now();
                    while let Some((event, release_time)) = queue.queue.first() {
                        if *release_time <= now {
                            try_send_panic(&key_tx, event.clone());
                            queue.queue.remove(0);
                        } else {
                            break;
                        }
                    }
                }
                Err(_) => {
                }
            }
        }
    });
}
