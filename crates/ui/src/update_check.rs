//! Background startup update-check watcher for the desktop UI (#2428 Part B).
//!
//! Runs the HTTP check off the UI thread at most once per day on native builds
//! (`run_windowed` only) and wakes the event loop once on completion so idle
//! frames never poll. Disabled in `wasm32` and in unit/headless test runs.
//!
//! Provenance: our own thread + mpsc watcher matching `ai_chat::worker`;
//! no vendor material consulted.

use std::sync::{Arc, mpsc};

use camcraft_app::update_check::ReleaseInfo;

/// Event-loop wake callback invoked when the startup update check finishes.
pub(crate) type WakeFn = Arc<dyn Fn() + Send + Sync>;

/// Result sent back from the one-shot startup check thread to the UI thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CheckOutcome {
    pub checked_at_utc: Option<u64>,
    pub update: Option<ReleaseInfo>,
}

/// Non-blocking receiver for the optional startup update check.
#[derive(Default)]
pub(crate) struct UpdateWatcher {
    rx: Option<mpsc::Receiver<CheckOutcome>>,
}

impl UpdateWatcher {
    /// Spawn a one-shot background thread at native desktop startup when enabled
    /// and at least 24 hours have passed since `last_check_utc`.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn spawn_if_due(
        &mut self,
        check_for_updates: bool,
        last_check_utc: Option<u64>,
        current_version: &'static str,
        wake: WakeFn,
    ) {
        #[cfg(test)]
        {
            let _ = (check_for_updates, last_check_utc, current_version, wake);
        }
        #[cfg(not(test))]
        {
            use camcraft_app::update_check::{
                current_unix_secs, fetch_latest_release, should_check_now,
            };

            if self.rx.is_some() {
                return;
            }

            let Some(now_utc) = current_unix_secs() else {
                return;
            };
            if !should_check_now(check_for_updates, last_check_utc, now_utc) {
                return;
            }
            let (tx, rx) = mpsc::channel();
            match std::thread::Builder::new()
                .name("camcraft-update-check".into())
                .spawn(move || {
                    let update = match fetch_latest_release(current_version) {
                        Ok(info) => info,
                        Err(err) => {
                            tracing::debug!("update check skipped silently: {err}");
                            None
                        }
                    };
                    let _ = tx.send(CheckOutcome {
                        checked_at_utc: Some(now_utc),
                        update,
                    });
                    wake();
                }) {
                Ok(_) => self.rx = Some(rx),
                Err(err) => tracing::debug!("update check thread not started: {err}"),
            }
        }
    }

    /// Poll once without blocking; drops the channel as soon as it finishes.
    pub(crate) fn poll(&mut self) -> Option<CheckOutcome> {
        let rx = self.rx.as_ref()?;
        match rx.try_recv() {
            Ok(outcome) => {
                self.rx = None;
                Some(outcome)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.rx = None;
                None
            }
        }
    }

    /// Drop any pending receiver at window shutdown.
    pub(crate) fn clear(&mut self) {
        self.rx = None;
    }

    #[cfg(test)]
    pub(crate) fn install_rx_for_test(&mut self, rx: mpsc::Receiver<CheckOutcome>) {
        self.rx = Some(rx);
    }
}
