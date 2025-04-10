use std::collections::HashMap;
use std::time::{Duration, Instant};
use crate::kanata::{KeyEvent, KeyValue, OsCode};
use std::sync::mpsc::SyncSender as Sender;
use std::thread;
use crate::kanata::debounce::debounce::try_send_panic;

/// Implementation of the asym_eager_defer_pk algorithm
/// See: https://github.com/qmk/qmk_firmware/blob/6ef97172889ccd5db376b2a9f8825489e24fdac4/docs/feature_debounce_type.md
pub struct AsymEagerDeferPk {
  debounce_duration: Duration,
  last_key_event_time: HashMap<OsCode, Instant>,
  release_timers: HashMap<OsCode, Option<thread::JoinHandle<()>>>,
}

impl AsymEagerDeferPk {
  pub fn new(debounce_duration_ms: u16) -> Self {
      Self {
          debounce_duration: Duration::from_millis(debounce_duration_ms.into()),
          last_key_event_time: HashMap::new(),
          release_timers: HashMap::new(),
      }
  }

  /// Cancels an existing timer for the given key (if any).
  /// THIS DOESN'T ACTUALLY TERMINATE THE THREAD, IT JUST REMOVES THE HANDLE
  fn cancel_timer(&mut self, oscode: &OsCode) {
      if let Some(timer) = self.release_timers.remove(oscode) {
          log::debug!("Cancelling timer for key release of {:?}", oscode);
          if let Some(handle) = timer {
              // Join the thread to ensure it has finished
              if let Err(e) = handle.join() {
                  log::error!("Failed to join timer thread for {:?}: {:?}", oscode, e);
              }
          }
      }
  }
}

impl crate::kanata::debounce::debounce::Debounce for AsymEagerDeferPk {
  fn process_event(&mut self, event: KeyEvent, process_tx: &Sender<KeyEvent>) {
      let now = Instant::now();
      let oscode = event.code;

      match event.value {
          KeyValue::Press => {
              // Cancel any existing release timer for this key
              self.cancel_timer(&oscode);

              // Check if the key press is within the debounce duration
              if let Some(&last_time) = self.last_key_event_time.get(&oscode) {
                  if now.duration_since(last_time) < self.debounce_duration {
                      log::info!("Debouncing key press for {:?}", oscode);
                      return; // Skip processing this event
                  }
              }

              // Eagerly process key-down events
              self.last_key_event_time.insert(oscode, now);
              try_send_panic(process_tx, event);
          }
          KeyValue::Release => {
              // Reset existing timer for release events
              self.cancel_timer(&oscode);

              let debounce_duration = self.debounce_duration;
              let process_tx = process_tx.clone();
              let release_event = event.clone();

              // Start a new timer for the key release
              let handle = thread::spawn(move || {
                  thread::sleep(debounce_duration);
                  log::info!("Emiting key release for {:?}", oscode);
                  try_send_panic(&process_tx, release_event);
              });

              self.release_timers.insert(oscode, Some(handle));
          }
          KeyValue::Repeat => {
              // Forward repeat events immediately
              log::info!("Forwarding repeat event for {:?}", oscode);
              try_send_panic(process_tx, event);
          }
          _ => {
              // Forward other key events without debouncing
              log::debug!("Forwarding other event for {:?}", oscode);
              try_send_panic(&process_tx, event);
          }
      }
  }
}
