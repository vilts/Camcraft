//! A test seam for the import's landing step (#917): it hands a built
//! import command to the same `finish_import` that a worker result goes
//! through, so a page test can land a model that no file can make (since
//! #917 no STEP text reaches an incomplete model at the pinned solidcore
//! commit). Provenance: our own test seam; no HSMWorks material consulted.

use std::path::PathBuf;

use camcraft_app::clock::Instant;
use camcraft_app::commands::ImportModel;

use super::State;

impl State {
    /// Land `command` as if the import worker of `file` returned it.
    pub(crate) fn land_import_for_test(&mut self, file: PathBuf, command: ImportModel) {
        self.import_file = Some(file);
        self.import_started = Some(Instant::now());
        self.finish_import(Ok(command));
    }
}
