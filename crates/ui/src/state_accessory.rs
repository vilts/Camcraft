//! Built-in accessory state (#789): the "Add accessory…" dialog's draft,
//! each placed accessory row's size draft (one per accessory, by name),
//! and the handlers its commands land through. Placing is the undoable
//! `InsertAccessory`; resizing a placed one is the undoable
//! `SetAccessory`. The drafts are view state, never history.
//! Provenance: our own dialog state over our own accessory commands.

use camcraft_app::accessory::{self, Accessory};
use camcraft_app::commands::{AccessoryPlacement, InsertAccessory, SetAccessory};
use camcraft_app::fixture;
use camcraft_app::geometry::{Point3, quat_from_euler};
use camcraft_app::model;
use camcraft_app::schema::Location;

use super::{ErrorScope, FixtureLibDialog, State};

/// The stock's default height over the jaws for parallels: 1/4 in.
pub const DEFAULT_ABOVE: f64 = 6.35;

/// "Add accessory…": the picked kind, its (editable) figures, and where
/// it goes - at a typed pose, (parallels) under the stock in a vise, or
/// (a lathe chuck, #1468) fitted to the stock.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessoryDraft {
    /// The Fixtures node's setup (`""` - the implicit setup).
    pub setup: String,
    pub params: Accessory,
    /// The setup's built-in vises, for the parallels fit.
    pub vises: Vec<String>,
    /// Parallels: fit under the stock in this vise (`None` - by pose).
    pub in_vise: Option<String>,
    /// The stock's top over the jaws, mm (the parallels fit).
    pub above: f64,
    /// A lathe chuck: fit it to the setup's stock (`false` - by pose).
    /// Opens ticked when the setup's stock is round.
    pub on_stock: bool,
    /// The accessory frame's origin, mm.
    pub pos: [f64; 3],
    pub yaw_deg: f64,
    pub error: Option<String>,
}

impl AccessoryDraft {
    /// Pick kind `kind` (an `accessory::KINDS` key): its defaults replace
    /// the draft's figures.
    pub fn pick(&mut self, kind: &str) {
        if let Some(a) = Accessory::default_of(kind) {
            self.params = a;
            self.error = None;
        }
    }

    /// Is this a lathe chuck the Place button fits to the stock? An
    /// inside grip never is: the stock's box does not give its bore.
    #[must_use]
    pub fn fits_stock(&self) -> bool {
        self.on_stock && accessory::chuck::is_chuck(&self.params) && self.params.grip() != Some(accessory::chuck::Grip::Inside)
    }

    /// Where the Place button puts it.
    #[must_use]
    pub fn placement(&self) -> AccessoryPlacement {
        match (&self.params, &self.in_vise) {
            (Accessory::Parallels { .. }, Some(vise)) => {
                AccessoryPlacement::UnderStockInVise { vise: vise.clone(), above: self.above }
            }
            _ if self.fits_stock() => AccessoryPlacement::OnStock,
            _ => AccessoryPlacement::At(Location {
                pos: Point3::new(self.pos[0], self.pos[1], self.pos[2]),
                rot: quat_from_euler(0.0, 0.0, self.yaw_deg.to_radians()),
            }),
        }
    }
}

/// One accessory row's size draft: `value` is what the fields show and
/// Apply sends; `live` the parameters it was taken from. When the live
/// parameters move (Apply, undo, redo) the draft follows them.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessorySizeDraft {
    pub live: Accessory,
    pub value: Accessory,
}

impl State {
    /// Open the accessory dialog over `setup`'s Fixtures node, on
    /// parallels - fitted into the setup's first built-in vise, if any.
    pub fn open_accessory_dialog(&mut self, setup: String) {
        let declared = (!setup.is_empty()).then_some(setup.as_str());
        let vises: Vec<String> = fixture::tagged(&self.world, declared)
            .into_iter()
            .filter(|(_, e)| fixture::def_of(&self.world, e).is_some_and(|d| d.vise.is_some()))
            .map(|(n, _)| n)
            .collect();
        let params = Accessory::default_of(accessory::KINDS[0]).unwrap_or(Accessory::Block {
            length: 76.2,
            width: 50.8,
            height: 25.4,
        });
        let in_vise = vises.first().cloned();
        let on_stock = model::stock_world_aabb_for(&self.world, declared).is_some_and(|b| accessory::chuck::stock_is_round(&b));
        self.fixture_lib = Some(FixtureLibDialog::Accessory(AccessoryDraft {
            setup,
            params,
            vises,
            in_vise,
            above: DEFAULT_ABOVE,
            on_stock,
            pos: [0.0; 3],
            yaw_deg: 0.0,
            error: None,
        }));
    }

    /// `CommandKind::InsertAccessory`: one `InsertAccessory`. Closes the
    /// dialog on success; a refusal is named in it and in the status.
    pub(super) fn insert_accessory(&mut self, setup: Option<String>, params: &Accessory, placement: AccessoryPlacement) {
        match InsertAccessory::new(&self.world, params, setup.as_deref(), placement) {
            Ok(command) => {
                let name = command.name().to_owned();
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = format!("placed {name}");
                self.fixture_lib = None;
            }
            Err(err) => {
                let msg = format!("accessory not placed: {err}");
                if let Some(FixtureLibDialog::Accessory(d)) = self.fixture_lib.as_mut() {
                    d.error = Some(msg.clone());
                }
                self.refuse(ErrorScope::Setup, msg);
            }
        }
    }

    /// `CommandKind::SetAccessory`: one `SetAccessory`.
    pub(super) fn set_accessory(&mut self, name: &str, params: Accessory) {
        match SetAccessory::new(&self.world, name, params) {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.accessory_sizes.remove(name);
                self.status = format!("{name}: resized");
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("accessory not changed: {err}")),
        }
    }

    /// Accessory `name`'s size draft, `live` its parameters in the world
    /// now: kept while they stand still, refreshed once they move. Each
    /// row has its own, so two rows never reset each other's typing.
    pub fn accessory_size_draft(&mut self, name: &str, live: &Accessory) -> &mut Accessory {
        let d = self
            .accessory_sizes
            .entry(name.to_owned())
            .or_insert_with(|| AccessorySizeDraft { live: live.clone(), value: live.clone() });
        if &d.live != live {
            *d = AccessorySizeDraft { live: live.clone(), value: live.clone() };
        }
        &mut d.value
    }

    /// Drop the drafts of accessories that are gone (deleted, renamed or
    /// no longer built-in accessories).
    pub fn retire_accessory_size_drafts(&mut self) {
        let world = &self.world;
        self.accessory_sizes.retain(|name, _| {
            model::find_by_name(world, name)
                .and_then(|e| fixture::def_of(world, &e))
                .is_some_and(|d| d.accessory.is_some())
        });
    }
}
