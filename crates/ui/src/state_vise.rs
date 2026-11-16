//! Built-in vise state (#788): the "Add built-in vise…" dialog's draft
//! and the handlers its commands land through. Placing is the undoable
//! `InsertVise` (the fixture library's insert path); editing a placed
//! vise's jaw opening or dimensions is the undoable `SetVise`. The drafts
//! are view state, never history.
//! Provenance: our own dialog state over our own vise commands.

use camcraft_app::commands::{InsertVise, SetVise, VisePlacement};
use camcraft_app::fixture;
use camcraft_app::geometry::{Point3, quat_from_euler};
use camcraft_app::model;
use camcraft_app::schema::Location;
use camcraft_app::vise::{self, ViseParams};
use camcraft_app::vise_fit;

use super::{ErrorScope, FixtureLibDialog, State};

/// "Add built-in vise…": the picked size row, its (editable) figures,
/// and where it goes - fitted to the stock, or at a typed pose.
#[derive(Clone, Debug, PartialEq)]
pub struct ViseDraft {
    /// The Fixtures node's setup (`""` - the implicit setup).
    pub setup: String,
    /// Index into `vise::presets()` (standard rows, then self-centering).
    pub size: usize,
    pub params: ViseParams,
    /// Against the setup's stock, jaws opened to it (the default).
    pub fit_stock: bool,
    /// The vise frame's origin (fixed jaw face, centred, at the floor; a
    /// self-centering vise's centre between the jaws), mm.
    pub pos: [f64; 3],
    pub yaw_deg: f64,
    pub error: Option<String>,
    /// What fitting would do right now, refreshed each frame by the
    /// dialog from the app's `vise_fit::preview` (`None` before the first
    /// frame: Fit stands as ticked).
    pub fit: Option<vise_fit::VisePreview>,
}

impl ViseDraft {
    /// A fresh draft on size row `size`, half open.
    #[must_use]
    pub fn new(setup: String, size: usize) -> Self {
        let size = size.min(vise::presets().count().saturating_sub(1));
        let params = vise::preset(size).map_or_else(empty_params, vise::ViseSize::half_open);
        Self { setup, size, params, fit_stock: true, pos: [0.0; 3], yaw_deg: 0.0, error: None, fit: None }
    }

    /// Pick size row `size`: its figures replace the draft's.
    pub fn pick(&mut self, size: usize) {
        if let Some(row) = vise::preset(size) {
            self.size = size;
            self.params = row.half_open();
            self.error = None;
        }
    }
}

/// One vise row's jaw-opening draft (#788, per vise since the #802
/// review): `value` is what the field shows and Set sends; `live` is the
/// vise's opening the draft was taken from. When the live opening moves
/// (Set, "Open to stock", undo, redo) the draft follows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViseOpeningDraft {
    pub live: f64,
    pub value: f64,
}

/// A placeholder the table never reaches (it is never empty); every
/// figure zero, so the vise law refuses it by name if it ever is.
fn empty_params() -> ViseParams {
    ViseParams {
        size: String::new(),
        jaw_width: 0.0,
        jaw_height: 0.0,
        jaw_thickness: 0.0,
        jaw_body_depth: 0.0,
        max_opening: 0.0,
        length: 0.0,
        width: 0.0,
        base_height: 0.0,
        housing_length: 0.0,
        housing_height: 0.0,
        opening: 0.0,
        style: vise::ViseStyle::Standard,
    }
}

impl State {
    /// Vise `name`'s jaw-opening draft, `live` its opening in the world
    /// now: kept while the live opening stands still, refreshed from it
    /// once it moves. Each vise row has its own, so two rows never reset
    /// each other's typing.
    pub fn vise_opening_draft(&mut self, name: &str, live: f64) -> &mut f64 {
        let d = self
            .vise_opening
            .entry(name.to_owned())
            .or_insert(ViseOpeningDraft { live, value: live });
        if d.live.to_bits() != live.to_bits() {
            *d = ViseOpeningDraft { live, value: live };
        }
        &mut d.value
    }

    /// Drop the drafts of vises that are gone (deleted, renamed, or no
    /// longer built-in vises), so a later vise of that name starts fresh.
    pub fn retire_vise_opening_drafts(&mut self) {
        let world = &self.world;
        self.vise_opening.retain(|name, _| {
            model::find_by_name(world, name)
                .and_then(|e| fixture::def_of(world, &e))
                .is_some_and(|d| d.vise.is_some())
        });
    }

    /// Open the built-in vise dialog over `setup`'s Fixtures node, on the
    /// 6 in size for an inch document and 150 mm otherwise.
    pub fn open_vise_dialog(&mut self, setup: String) {
        let want = if self.units() == camcraft_app::units::Unit::Inches { "6 in" } else { "150 mm" };
        let size = vise::presets().position(|s| s.label == want).unwrap_or(0);
        self.fixture_lib = Some(FixtureLibDialog::Vise(ViseDraft::new(setup, size)));
    }

    /// `CommandKind::InsertBuiltinVise`: one `InsertVise`. Closes the
    /// dialog on success; a refusal is named in it and in the status.
    pub(super) fn insert_builtin_vise(
        &mut self,
        setup: Option<String>,
        params: &ViseParams,
        fit_stock: bool,
        pos: [f64; 3],
        yaw_deg: f64,
    ) {
        let placement = if fit_stock {
            VisePlacement::FitStock
        } else {
            VisePlacement::At(Location {
                pos: Point3::new(pos[0], pos[1], pos[2]),
                rot: quat_from_euler(0.0, 0.0, yaw_deg.to_radians()),
            })
        };
        match InsertVise::new(&self.world, params, setup.as_deref(), placement) {
            Ok(command) => {
                let name = command.name().to_owned();
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = format!("placed built-in vise {name}");
                self.fixture_lib = None;
            }
            Err(err) => {
                let msg = format!("vise not placed: {err}");
                if let Some(FixtureLibDialog::Vise(d)) = self.fixture_lib.as_mut() {
                    d.error = Some(msg.clone());
                }
                self.refuse(ErrorScope::Setup, msg);
            }
        }
    }

    /// `CommandKind::SetVise`: one `SetVise` - the opening (and any edited
    /// figure) from `params`, or the opening from the setup's stock.
    pub(super) fn set_builtin_vise(&mut self, name: &str, params: ViseParams, from_stock: bool) {
        let planned = if from_stock {
            SetVise::from_stock(&self.world, name, params)
        } else {
            SetVise::new(&self.world, name, params)
        };
        match planned {
            Ok(command) => {
                let opening = command.params().map_or(0.0, |p| p.opening);
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.vise_opening.remove(name);
                let u = self.units();
                self.status = format!("{name}: jaws open {}", camcraft_app::units::len(opening, u, 3));
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("vise not changed: {err}")),
        }
    }
}
