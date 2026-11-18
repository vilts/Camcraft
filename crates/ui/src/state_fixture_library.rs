//! The fixture library dialogs' state (#339 slice 6): "Save fixture to
//! library…" and "Insert from library…" on a setup's Fixtures node.
//! Saving and deleting entries are file edits of the user library
//! (`app::fixture_library`), confirmed in the dialog and never undo
//! entries - the #383 machine-file law. Inserting is a document change,
//! so it is the undoable `InsertFixture` command, queued like any other.
//! Provenance: our own dialog state over our own library format; no
//! vendor fixture material consulted.

use std::path::PathBuf;

use camcraft_app::commands::InsertFixture;
use camcraft_app::fixture;
use camcraft_app::fixture_library::{self, DatumChoice, LibraryError};
use camcraft_app::geometry::{Point3, quat_from_euler};
use camcraft_app::model;
use camcraft_app::schema::Location;

use super::{ErrorScope, State};

/// One library entry as the insert dialog lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryRow {
    pub name: String,
    pub note: Option<String>,
    pub facets: usize,
}

/// "Save fixture to library…": which of the setup's fixtures, by which
/// datum, with what note. `confirm_replace` holds the names already in
/// the library while the dialog asks before replacing them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SaveFixtureDraft {
    /// The Fixtures node's setup (`""` - the implicit setup).
    pub setup: String,
    pub fixtures: Vec<(String, bool)>,
    pub datum: DatumChoice,
    pub note: String,
    pub confirm_replace: Option<Vec<String>>,
    pub error: Option<String>,
}

/// "Insert from library…": the listed entries, the picked one, the
/// datum's placement (mm and degrees, R = Rz ⊗ Ry ⊗ Rx as the Model
/// page) and a pending delete confirmation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InsertFixtureDraft {
    pub setup: String,
    pub rows: Vec<LibraryRow>,
    pub selected: Option<String>,
    pub pos: [f64; 3],
    pub rpy_deg: [f64; 3],
    pub confirm_delete: Option<String>,
    pub error: Option<String>,
}

/// The open fixture library dialog, if any (view state, never saved).
#[derive(Clone, Debug, PartialEq)]
pub enum FixtureLibDialog {
    Save(SaveFixtureDraft),
    Insert(InsertFixtureDraft),
    /// #788: "Add built-in vise…".
    Vise(super::ViseDraft),
    /// #819: "Add fixture plate…".
    Plate(super::PlateDraft),
    /// #789: "Add accessory…".
    Accessory(super::AccessoryDraft),
    /// #744: "Add profile / DXF fixture…".
    Profile(super::FixtureProfileDraft),
}

fn declared(setup: &str) -> Option<&str> {
    (!setup.is_empty()).then_some(setup)
}

impl State {
    /// The library file this window edits: the test override, else the
    /// user fixtures folder (`CAMCRAFT_FIXTURES`, else the data dir).
    pub fn fixture_library_path(&self) -> Result<PathBuf, String> {
        self.fixture_lib_dir
            .clone()
            .or_else(fixture_library::user_dir)
            .map(|d| fixture_library::library_path(&d))
            .ok_or_else(|| format!("no user fixtures folder (set {} or a home folder)", fixture_library::ENV_VAR))
    }

    /// Open the save dialog over `setup`'s fixtures; the selected ones
    /// start checked (the only one, when a setup has just one).
    pub fn open_fixture_library_save(&mut self, setup: String) {
        let w = &self.world;
        let tagged = fixture::tagged(w, declared(&setup));
        let lone = tagged.len() == 1;
        let fixtures = tagged.iter().map(|(n, e)| (n.clone(), lone || model::is_selected(w, e))).collect();
        self.fixture_lib = Some(FixtureLibDialog::Save(SaveFixtureDraft { setup, fixtures, ..Default::default() }));
    }

    /// Open the insert dialog over `setup`, listing the library afresh.
    pub fn open_fixture_library_insert(&mut self, setup: String) {
        let mut draft = InsertFixtureDraft { setup, ..Default::default() };
        self.reload_library_rows(&mut draft);
        self.fixture_lib = Some(FixtureLibDialog::Insert(draft));
    }

    fn reload_library_rows(&self, draft: &mut InsertFixtureDraft) {
        let read = self.fixture_library_path().and_then(|p| fixture_library::load(&p).map_err(|e| e.to_string()));
        match read {
            Ok(lib) => {
                draft.rows = lib
                    .into_iter()
                    .map(|e| LibraryRow { name: e.name, note: e.note, facets: e.facets.len() })
                    .collect();
                draft.error = None;
            }
            Err(err) => {
                draft.rows.clear();
                draft.error = Some(format!("library not read: {err}"));
            }
        }
        if !draft.selected.as_ref().is_some_and(|s| draft.rows.iter().any(|r| &r.name == s)) {
            draft.selected = draft.rows.first().map(|r| r.name.clone());
        }
    }

    /// The save dialog's Save (and, after the replace prompt, Replace):
    /// writes the checked fixtures into the library file. A clash asks
    /// first (`confirm_replace`); a refusal stays in the dialog by name.
    pub fn save_fixtures_to_library(&mut self, replace: bool) {
        let Some(FixtureLibDialog::Save(draft)) = self.fixture_lib.as_ref() else {
            return;
        };
        let names: Vec<String> = draft.fixtures.iter().filter(|(_, on)| *on).map(|(n, _)| n.clone()).collect();
        let entries = if names.is_empty() {
            Err("check a fixture to save".to_owned())
        } else {
            names
                .iter()
                .map(|n| fixture_library::entry_from_model(&self.world, n, draft.datum, Some(draft.note.as_str())))
                .collect::<Result<Vec<_>, _>>()
        };
        let outcome = entries.and_then(|entries| {
            let path = self.fixture_library_path()?;
            Ok(fixture_library::add_entries(&path, entries, replace).map(|()| path))
        });
        let Some(FixtureLibDialog::Save(draft)) = self.fixture_lib.as_mut() else {
            return;
        };
        match outcome {
            Ok(Ok(path)) => {
                self.status = format!("saved {} fixture(s) to {}", names.len(), path.display());
                self.fixture_lib = None;
            }
            Ok(Err(LibraryError::Exists(taken))) => {
                draft.confirm_replace = Some(taken);
                draft.error = None;
            }
            Ok(Err(err)) => {
                draft.confirm_replace = None;
                draft.error = Some(format!("not saved: {err}"));
            }
            Err(err) => {
                draft.confirm_replace = None;
                draft.error = Some(format!("not saved: {err}"));
            }
        }
    }

    /// The insert dialog's confirmed Delete: removes the entry from the
    /// library file (no undo - a file edit, asked first) and re-lists.
    pub fn delete_library_entry(&mut self, name: &str) {
        let done = self
            .fixture_library_path()
            .and_then(|p| fixture_library::delete_entry(&p, name).map_err(|e| e.to_string()));
        let Some(FixtureLibDialog::Insert(mut draft)) = self.fixture_lib.take() else {
            return;
        };
        draft.confirm_delete = None;
        self.reload_library_rows(&mut draft);
        match done {
            Ok(()) => self.status = format!("deleted '{name}' from the fixture library"),
            Err(err) => draft.error = Some(format!("not deleted: {err}")),
        }
        self.fixture_lib = Some(FixtureLibDialog::Insert(draft));
    }

    /// `CommandKind::InsertFixtureFromLibrary`: read the entry afresh and
    /// execute `InsertFixture` - one undo unit. Closes the dialog on
    /// success; a refusal names the entry and leaves the dialog open.
    pub(super) fn insert_fixture_from_library(
        &mut self,
        entry: &str,
        setup: Option<String>,
        pos: [f64; 3],
        rpy_deg: [f64; 3],
    ) {
        if rpy_deg.iter().any(|a| !a.is_finite()) {
            self.refuse(ErrorScope::Setup, "fixture not inserted: rotation angles must be finite".into());
            return;
        }
        let pick = Location {
            pos: Point3::new(pos[0], pos[1], pos[2]),
            rot: quat_from_euler(rpy_deg[0].to_radians(), rpy_deg[1].to_radians(), rpy_deg[2].to_radians()),
        };
        let planned = self
            .fixture_library_path()
            .and_then(|p| fixture_library::load(&p).map_err(|e| e.to_string()))
            .and_then(|lib| {
                lib.into_iter().find(|e| e.name == entry).ok_or_else(|| format!("no fixture '{entry}' in the library"))
            })
            .and_then(|e| InsertFixture::new(&self.world, &e, setup.as_deref(), pick).map_err(|e| e.to_string()));
        match planned {
            Ok(command) => {
                let name = command.name().to_owned();
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = format!("inserted fixture {name} from the library");
                self.fixture_lib = None;
            }
            Err(err) => {
                let msg = format!("fixture '{entry}' not inserted: {err}");
                if let Some(FixtureLibDialog::Insert(d)) = self.fixture_lib.as_mut() {
                    d.error = Some(msg.clone());
                }
                self.refuse(ErrorScope::Setup, msg);
            }
        }
    }
}
