//! Provenance: issue #3781; transient credential controls over our existing provider APIs.

use std::sync::{Arc, mpsc};
use std::time::Duration;

use camcraft_app::ai::ApiKey;
// The clock seam (#31): the std clock panics on wasm32.
use camcraft_app::clock::Instant;

use super::settings_text::NO_KEY;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SettingsRequest {
    pub preset_id: String,
    pub model: String,
    pub offline_only: bool,
}

const MODEL_CHANGED: &str = "Model changed. Click Test to test this model.";
/// A local check that failed runs again after this delay, so the status follows the server.
pub(crate) const LOCAL_RECHECK: Duration = Duration::from_secs(5);

pub(crate) enum SettingsAction {
    Check,
    Save(ApiKey),
    Remove,
    Test,
}

pub(crate) trait SettingsBackend: Send + Sync {
    fn run(&self, request: &SettingsRequest, action: SettingsAction) -> Result<String, String>;
}

pub(crate) struct Settings {
    pub backend: Arc<dyn SettingsBackend>,
    pub status: String,
    pub editing: bool,
    pub draft: String,
    request: Option<SettingsRequest>,
    pending: Option<mpsc::Receiver<Result<String, String>>>,
    needs_check: bool,
    check_after: Option<Instant>,
    tested_model: Option<String>,
    /// The status shows the result of a Test of `request.model`.
    shows_test_result: bool,
    /// The last check found no key. Test stays disabled until a key is found.
    pub no_key: bool,
    /// The queued check repeats a failed local check: keep the status while it runs.
    rechecking: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            status: "Checking provider...".into(),
            editing: false,
            draft: String::new(),
            request: None,
            pending: None,
            needs_check: false,
            check_after: None,
            tested_model: None,
            shows_test_result: false,
            no_key: false,
            rechecking: false,
        }
    }
}

impl Settings {
    /// Never retain a password draft when the editor closes.
    pub fn close_editor(&mut self) {
        self.draft.clear();
        self.editing = false;
    }

    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    #[cfg(test)]
    pub fn has_work(&self) -> bool {
        self.busy() || self.needs_check
    }

    /// The delay to the next frame that has settings work, or `None` when idle.
    pub fn next_wake(&self, now: Instant) -> Option<Duration> {
        if self.busy() {
            return Some(Duration::from_millis(50));
        }
        self.needs_check.then(|| {
            self.check_after
                .map_or(Duration::ZERO, |deadline| {
                    deadline.saturating_duration_since(now)
                })
                .max(Duration::from_millis(50))
        })
    }

    /// Keep one job in flight. Cloud readiness depends on credentials, not the model.
    pub fn sync(&mut self, request: SettingsRequest) {
        self.sync_at(request, Instant::now());
    }

    pub(crate) fn sync_at(&mut self, request: SettingsRequest, now: Instant) {
        if self.request.as_ref() != Some(&request) {
            let same_provider = self.request.as_ref().is_some_and(|previous| {
                previous.preset_id == request.preset_id
                    && previous.offline_only == request.offline_only
            });
            let cloud = camcraft_app::ai::by_id(&request.preset_id)
                .is_some_and(|preset| preset.key.is_some());
            self.close_editor();
            if !same_provider {
                self.no_key = false;
            }
            self.rechecking = false;
            self.request = Some(request);
            if !same_provider || !cloud {
                self.status = "Checking provider...".into();
                self.needs_check = true;
                // Coalesce local Model keystrokes. Provider switches check immediately.
                self.check_after = same_provider.then(|| now + Duration::from_millis(500));
                self.shows_test_result = false;
            } else if self.shows_test_result {
                // The Test result is for the previous model. Do not show it for this one.
                self.status = MODEL_CHANGED.into();
                self.shows_test_result = false;
            }
        }
        if let Some(rx) = &self.pending {
            let result = match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                    "The provider check stopped. Click Test to try again.".into(),
                )),
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                let local = self.request.as_ref().is_some_and(|request| {
                    camcraft_app::ai::by_id(&request.preset_id).is_none_or(|p| p.key.is_none())
                });
                if local && result.is_err() && !self.needs_check {
                    // The user can start Ollama at any time: check again soon.
                    self.needs_check = true;
                    self.check_after = Some(now + LOCAL_RECHECK);
                    self.rechecking = true;
                    self.status = result.unwrap_or_else(|message| message);
                    self.shows_test_result = self.tested_model.is_some();
                    self.tested_model = None;
                    self.pending = None;
                    return;
                }
                self.no_key = !local && matches!(&result, Ok(status) if status == NO_KEY);
                if !self.needs_check {
                    let current_model = self.request.as_ref().map(|request| &request.model);
                    if self
                        .tested_model
                        .as_ref()
                        .is_none_or(|model| Some(model) == current_model)
                    {
                        self.status = result.unwrap_or_else(|message| message);
                        self.shows_test_result = self.tested_model.is_some();
                    } else {
                        self.status = MODEL_CHANGED.into();
                    }
                }
                self.tested_model = None;
                self.pending = None;
            }
        }
        if !self.busy()
            && self.needs_check
            && self.check_after.is_none_or(|deadline| now >= deadline)
        {
            // Starting the queued check replaces a stale status before the UI draws it.
            self.needs_check = false;
            self.start(SettingsAction::Check);
        }
    }

    pub fn save(&mut self) {
        match ApiKey::new(&self.draft) {
            Ok(key) => {
                self.close_editor();
                self.start(SettingsAction::Save(key));
            }
            Err(message) => self.status = message,
        }
    }

    pub fn start(&mut self, action: SettingsAction) {
        if self.busy() {
            return;
        }
        let Some(request) = self.request.clone() else {
            return;
        };
        if matches!(action, SettingsAction::Test)
            && request.offline_only
            && camcraft_app::ai::by_id(&request.preset_id).is_some_and(|p| p.key.is_some())
        {
            self.status =
                "Offline only blocks the cloud test. Clear Offline only to test this key.".into();
            return;
        }
        if matches!(action, SettingsAction::Test) && self.no_key {
            self.status = "Set an API key first. Test needs a key.".into();
            return;
        }
        let tested_model = matches!(&action, SettingsAction::Test).then(|| request.model.clone());
        let backend = Arc::clone(&self.backend);
        let (tx, rx) = mpsc::channel();
        let started = request.clone();
        match std::thread::Builder::new()
            .name("ai-provider-settings".into())
            .spawn(move || {
                let result = backend.run(&started, action);
                let _ = tx.send(result);
            }) {
            Ok(_) => {
                if tested_model.is_some() {
                    self.status = "Testing...".into();
                } else if !self.rechecking {
                    self.status = "Checking provider...".into();
                }
                self.rechecking = false;
                self.pending = Some(rx);
                self.tested_model = tested_model;
                self.shows_test_result = false;
                self.needs_check = false;
                self.check_after = None;
            }
            Err(_) => {
                self.status = "The provider check could not start. Click Test to retry.".into()
            }
        }
    }
}

fn default_backend() -> Arc<dyn SettingsBackend> {
    #[cfg(all(not(test), not(target_arch = "wasm32")))]
    {
        Arc::new(super::settings_native::NativeSettings)
    }
    #[cfg(any(test, target_arch = "wasm32"))]
    {
        Arc::new(UnavailableSettings)
    }
}

#[cfg(any(test, target_arch = "wasm32"))]
struct UnavailableSettings;

#[cfg(any(test, target_arch = "wasm32"))]
impl SettingsBackend for UnavailableSettings {
    fn run(&self, _: &SettingsRequest, _: SettingsAction) -> Result<String, String> {
        #[cfg(test)]
        {
            Ok(NO_KEY.into())
        }
        #[cfg(not(test))]
        {
            Err("Provider setup needs the desktop app.".into())
        }
    }
}
