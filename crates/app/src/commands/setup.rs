//! Setup commands: create, activate, and move operations
//! between setups (M3.2 stage A).
//!
//! Provenance: our own design; one clamping = one datum system is
//! standard machine-shop practice, specced from general machining
//! knowledge - no vendor implementation inspected. Like every sibling
//! here, construction reads the world, validates, and snapshots; `apply`
//! and `invert` are pure writes, so undo/redo replay bytes and never
//! re-derive.
//!
//! ## The laws these commands keep
//!
//! - **Materialization, never renaming.** Creating the first setup on a
//!   document that has none points it at the existing `Stock`/`G54`
//!   entities ([`crate::setup`]'s implicit-setup law). Nothing is
//!   renamed: every cached stamp, path and saved file predating setups
//!   keeps resolving, byte for byte.
//! - **Datum transfer = clone, not alias.** Transferring a datum copies
//!   the source setup's stock (parameters, placement, facets) and its
//!   WCS placement into the new setup's OWN entities (#342: one WCS). A shop then
//!   re-places the copy to describe the re-clamp - placement IS the
//!   flip (M3.3 adds the rotated form). `datum_from` records where the
//!   clone came from; it is sheet provenance, inert for every stamp.
//! - **A setup with operations is not deletable.** [`DeleteSetup`]
//!   refuses while any operation belongs to it ([`crate::setup::belongs`],
//!   by declaration or by the first/implicit position), naming the
//!   operation. Furniture the setup alone owns goes with it; names the
//!   legacy world or another setup also points at stay. A setup another
//!   is placed relative to (#316) is refused the same way, naming it.
//! - **Activation is inert data** (AGENTS.md, M1.6): which setup is
//!   ACTIVE changes no path, no stamp, no staleness - it only decides
//!   where NEW things join. Commands write the singleton; readers
//!   (`setup::active`) fall back to the first setup when it names
//!   nothing that exists.
//! - **Moving an operation between setups keeps its stamp.** The
//!   digest covers the RESOLVED heights and the stock's identity, so
//!   the machinery that already exists says Ok when the two setups
//!   place their stock identically, and Stale when they do not - no
//!   forced invalidation either way.

use flecs_ecs::prelude::World;

pub use super::setup_delete::DeleteSetup;

use crate::commands::{Command, CommandError};
use crate::geometry::Triangle;
use crate::model;
use crate::schema::{
    Location, Operation, OperationMeta, SetupDef, StockDef, Wcs, STOCK_NAME, WCS_SLOTS,
};
use crate::setup;
use crate::stock::StockShape;

/// A setup's stock as the transfer copies it: entity name, parameters,
/// placement, and the local facets a `FromModel` stock mirrors.
pub(super) struct StockSnap {
    pub(super) name: String,
    pub(super) shape: StockShape,
    pub(super) location: Location,
    pub(super) facets: std::sync::Arc<[Triangle]>,
}

/// Create a setup, optionally transferring a datum: cloning the named
/// source setup's stock and WCS placement into the new setup's own
/// entities. With no source the new setup starts with empty
/// furniture (lazily spawned by `SetStock`/`SetWcsOrigin` when used).
pub struct CreateSetup {
    name: String,
    def: SetupDef,
    /// Furniture the datum transfer clones in (owned names only - the
    /// materialization case clones nothing; it points at what exists).
    stock: Option<StockSnap>,
    wcs: Option<(String, Location)>,
    /// The active setup before this command, restored by its inverse.
    prev_active: Option<String>,
}

impl CreateSetup {
    /// `name` defaults to the next free `SetupN`. `datum_from` names an
    /// existing setup to clone; `None` means a fresh, empty context -
    /// except on a document with no setups at all, where creation is by
    /// law the MATERIALIZATION and points at the legacy entities.
    pub fn new(
        world: &World,
        name: Option<&str>,
        datum_from: Option<&str>,
    ) -> Result<Self, CommandError> {
        let name = match name {
            Some(n) => {
                if n.trim().is_empty() {
                    return Err(CommandError::Invalid("setup name must not be empty".into()));
                }
                if n.contains('/') {
                    return Err(CommandError::Invalid(
                        "a setup name must not contain '/' - names with '/' are reserved for a setup's own stock and work offset".into(),
                    ));
                }
                n.to_string()
            }
            None => setup::next_setup_name(world).ok_or_else(|| {
                CommandError::Invalid("no free SetupN name - this document is out of setups".into())
            })?,
        };
        if model::find_by_name(world, &name).is_some() {
            return Err(CommandError::NameConflict(name.clone()));
        }
        if STOCK_NAME == name || WCS_SLOTS.contains(&name.as_str()) {
            return Err(CommandError::Invalid(format!(
                "'{name}' is a name reserved for a setup's stock and work offsets"
            )));
        }
        let materializing = setup::names(world).is_empty();
        if materializing && datum_from.is_some() {
            return Err(CommandError::Invalid(
                "there is nothing to transfer a datum from yet".into(),
            ));
        }
        let def = if materializing {
            SetupDef {
                stock: STOCK_NAME.to_string(),
                wcs: WCS_SLOTS[0].to_string(),
                datum_from: String::new(),
                work_offset: None,
                part_alignment: None,
                machine: None,
                post: None,
                relative_to: None,
                stock_source: None,
                post_options: None,
                workholding_max_rpm: None,
            }
        } else {
            SetupDef {
                stock: format!("{name}/Stock"),
                wcs: format!("{name}/{}", setup::WCS_SUFFIX),
                datum_from: datum_from.unwrap_or("").to_string(),
                // The datum transfer carries the source's work-offset
                // label with its WCS (#196): the word is part of the
                // datum system being cloned.
                work_offset: datum_from.and_then(|f| setup::work_offset(world, f)),
                // #326: the cloned stock and offsets sit in the source's
                // part frame, so the part alignment rides the transfer.
                part_alignment: datum_from.and_then(|f| crate::setup_align::stored(world, f)),
                // #343: a datum transfer is the same clamping system moved
                // to a new setup - it keeps the source's machine and post.
                machine: datum_from.and_then(|f| setup::def_of(world, f)).and_then(|d| d.machine),
                post: datum_from.and_then(|f| setup::def_of(world, f)).and_then(|d| d.post),
                // #316: a datum transfer starts in the source's frame (the
                // transform between them is the identity) but links nothing:
                // the clone does not follow the source's later edits.
                relative_to: None,
                // #320: a new setup takes the default stock source.
                stock_source: None,
                // #389: the post's options ride the post (#343's transfer).
                post_options: datum_from.and_then(|f| setup::def_of(world, f)).and_then(|d| d.post_options),
                // #1031: the same clamping system keeps its chuck's rating -
                // dropping a limit would silently lift a safety ceiling.
                workholding_max_rpm: datum_from.and_then(|f| setup::def_of(world, f)).and_then(|d| d.workholding_max_rpm),
            }
        };
        let mut stock = None;
        let mut wcs = None;
        if let Some(from) = datum_from {
            if !setup::exists(world, from) {
                return Err(CommandError::NotFound(from.to_string()));
            }
            if let Some((entity, shape)) = setup::stock_entity(world, Some(from))
                .and_then(|e| model::stock_def_of(world, &e).map(|s| (e, s)))
            {
                stock = Some(StockSnap {
                    name: format!("{name}/Stock"),
                    shape,
                    location: model::transform_of(world, &entity).unwrap_or_default(),
                    facets: model::facets_of(world, &entity).unwrap_or_default(),
                });
            }
            if let Some(loc) = setup::wcs_entity(world, Some(from))
                .and_then(|entity| model::transform_of(world, &entity))
            {
                wcs = Some((def.wcs.clone(), loc));
            }
        }
        Ok(CreateSetup {
            name,
            def,
            stock,
            wcs,
            prev_active: setup::active(world),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Command for CreateSetup {
    fn apply(&self, world: &World) {
        let entity = match model::find_by_name(world, &self.name) {
            Some(e) => e,
            None => model::spawn(world, &self.name),
        };
        entity.entity_view(world).set(self.def.clone());
        if let Some(s) = &self.stock {
            let e = match model::find_by_name(world, &s.name) {
                Some(e) => e,
                None => model::spawn(world, &s.name),
            };
            model::set_transform(world, &e, s.location);
            model::set_facets(world, &e, s.facets.clone());
            e.entity_view(world).set(StockDef {
                shape: s.shape.clone(),
            });
        }
        if let Some((wname, loc)) = &self.wcs {
            let e = match model::find_by_name(world, wname) {
                Some(e) => e,
                None => model::spawn(world, wname),
            };
            e.entity_view(world).add(Wcs);
            model::set_transform(world, &e, *loc);
        }
        // The new setup is the one you work in (module header: creation
        // activates; undo restores what was active).
        setup::set_active(world, Some(self.name.clone()));
    }

    fn invert(&self, world: &World) {
        setup::set_active(world, self.prev_active.clone());
        if let Some(s) = &self.stock
            && let Some(e) = model::find_by_name(world, &s.name)
        {
            model::remove(world, &e);
        }
        if let Some((wname, _)) = &self.wcs
            && let Some(e) = model::find_by_name(world, wname)
        {
            model::remove(world, &e);
        }
        if let Some(e) = model::find_by_name(world, &self.name) {
            model::remove(world, &e);
        }
    }
}

/// Choose the ACTIVE setup: where new operations and new furniture
/// join. Inert for every stamp (module header) - this command moves a
/// cursor, not geometry.
pub struct ActivateSetup {
    name: Option<String>,
    prev: Option<String>,
}

impl ActivateSetup {
    pub fn new(world: &World, name: Option<&str>) -> Result<Self, CommandError> {
        if let Some(n) = name
            && !setup::exists(world, n)
        {
            return Err(CommandError::NotFound(n.to_string()));
        }
        Ok(ActivateSetup {
            name: name.map(str::to_string),
            prev: setup::active(world),
        })
    }
}

impl Command for ActivateSetup {
    fn apply(&self, world: &World) {
        setup::set_active(world, self.name.clone());
    }

    fn invert(&self, world: &World) {
        setup::set_active(world, self.prev.clone());
    }
}

/// Move an operation into another setup (or into the implicit first
/// position with `None`). The stamp is KEPT: resolved heights and the
/// stock's identity re-digest on the next question, so the operation
/// reads Ok when the placements match and Stale when they do not - the
/// staleness law answering, not this command assuming.
pub struct SetOperationSetup {
    name: String,
    prev: Operation,
    next: Operation,
    prev_meta: OperationMeta,
    next_meta: OperationMeta,
}

impl SetOperationSetup {
    pub fn new(
        world: &World,
        op_name: &str,
        target: Option<&str>,
    ) -> Result<Self, CommandError> {
        let entity = model::find_by_name(world, op_name)
            .ok_or_else(|| CommandError::NotFound(op_name.to_string()))?;
        let prev = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{op_name}' is not an operation")))?;
        if let Some(t) = target
            && !setup::exists(world, t)
        {
            return Err(CommandError::NotFound(t.to_string()));
        }
        if target == prev.setup.as_deref() {
            return Err(CommandError::Invalid(format!(
                "'{op_name}' already machines in {}",
                target.map_or_else(|| "the first setup".to_string(), str::to_string)
            )));
        }
        let next = Operation {
            setup: target.map(str::to_string),
            ..prev.clone()
        };
        let prev_meta = model::operation_meta_of(world, &entity);
        let mut next_meta = prev_meta.clone();
        // A rank belongs to its old setup. A moved operation joins the
        // target's unranked tail until the user places it there.
        next_meta.order = None;
        Ok(SetOperationSetup {
            name: op_name.to_string(),
            prev,
            next,
            prev_meta,
            next_meta,
        })
    }
}

impl Command for SetOperationSetup {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
            entity.entity_view(world).set(self.next_meta.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
            entity.entity_view(world).set(self.prev_meta.clone());
        }
    }
}
