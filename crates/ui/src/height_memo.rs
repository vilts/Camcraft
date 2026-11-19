//! Display-only height reads, retained while their actual inputs stay equal.
//! Geometry allocations are held weakly so a deleted document cannot reuse a key.
//! Generation, verification and posting never read this module.
use std::cell::{Cell, RefCell};
use std::sync::{Arc, Weak};

use camcraft_app::geometry::{Aabb, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, SrcRef, Tilt};
use camcraft_app::schema::{Location, Operation, SetupDef};
use camcraft_app::units::Unit;
use flecs_ecs::prelude::{Entity, World};

struct Mesh(Weak<[Triangle]>);
impl PartialEq for Mesh {
    fn eq(&self, rhs: &Self) -> bool {
        self.0.ptr_eq(&rhs.0)
    }
}

#[derive(PartialEq)]
struct Body {
    name: String,
    entity: Entity,
    mesh: Option<Mesh>,
    pose: Option<Location>,
    stock: bool,
    fixture: bool,
    model: bool,
    wcs: bool,
}

#[derive(PartialEq)]
struct Key {
    setup: Option<String>,
    definition: Option<SetupDef>,
    default_definition: Option<(String, SetupDef)>,
    tilt: Option<Tilt>,
    bodies: Vec<Body>,
    excluded_part: Option<Entity>,
}
impl Key {
    fn of(world: &World, setup: Option<&str>, tilt: Option<Tilt>) -> Self {
        let effective = camcraft_app::setup::effective(world, setup);
        Self {
            setup: effective.clone(),
            definition: effective
                .as_deref()
                .and_then(|s| camcraft_app::setup::def_of(world, s)),
            // Untilting retains the shipped default-stock bounds read.
            default_definition: camcraft_app::setup::effective(world, None)
                .and_then(|name| camcraft_app::setup::def_of(world, &name).map(|def| (name, def))),
            tilt,
            excluded_part: camcraft_app::stock_source::stock_only_model(world),
            // Include furniture origins, geometry membership and fixture tags as well
            // as mesh/pose changes. No facet traversal occurs while taking this key.
            bodies: model::named_entities(world)
                .into_iter()
                .map(|(name, entity)| {
                    let mesh = model::facets_of(world, &entity);
                    Body {
                        name,
                        entity,
                        mesh: mesh.map(|m| Mesh(Arc::downgrade(&m))),
                        pose: model::transform_of(world, &entity),
                        stock: model::is_stock(world, &entity),
                        fixture: model::is_fixture(world, &entity),
                        model: model::is_model(world, &entity),
                        wcs: model::is_wcs(world, &entity),
                    }
                })
                .collect(),
        }
    }
}
type HeightRead = (Option<SrcRef>, Height, Result<f64, &'static str>);

#[derive(PartialEq)]
struct AdviceKey {
    geometry: Key,
    machine_kind: camcraft_app::machine::Kind,
    program: Vec<(String, Operation)>,
    unit: Unit,
    src: SrcRef,
    params: OpParams,
    this: Option<String>,
}

struct Entry {
    key: Key,
    bounds: Option<Option<Aabb>>,
    heights: Vec<HeightRead>,
}
thread_local! {
    static MEMO: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
    static BUILDS: Cell<u64> = const { Cell::new(0) };
    static ADVICE: RefCell<Option<(AdviceKey, Option<String>)>> = const { RefCell::new(None) };
    static ADVICE_BUILDS: Cell<u64> = const { Cell::new(0) };
}

/// The draft's display advice follows geometry, machine kind, units and program order,
/// including suppression changes. It never supplies a generation verdict.
pub(crate) fn clearance_advice(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    params: &OpParams,
    this: Option<&str>,
) -> Option<String> {
    let key = AdviceKey {
        geometry: Key::of(world, setup, params.tilt()),
        machine_kind: camcraft_app::machine::kind_of_setup(world, setup),
        program: camcraft_app::program_entry::sequence(world)
            .into_iter()
            .filter_map(|(name, entity)| model::operation_of(world, &entity).map(|op| (name, op)))
            .collect(),
        unit: camcraft_app::units::of(world),
        src: src.clone(),
        params: params.clone(),
        this: this.map(str::to_owned),
    };
    ADVICE.with(|memo| {
        let mut memo = memo.borrow_mut();
        if let Some((old, value)) = memo.as_ref()
            && *old == key
        {
            return value.clone();
        }
        let _span = crate::frame_cost::span("display/heights.clearance_advice");
        ADVICE_BUILDS.with(|n| n.set(n.get().saturating_add(1)));
        let value = operation::clearance_in_stock_advice(world, setup, src, params, this);
        *memo = Some((key, value.clone()));
        value
    })
}

#[cfg(test)]
pub(crate) fn advice_builds() -> u64 {
    ADVICE_BUILDS.with(Cell::get)
}
fn with_entry<R>(key: Key, f: impl FnOnce(&mut Entry) -> R) -> R {
    MEMO.with(|memo| {
        let mut memo = memo.borrow_mut();
        let mut entry = memo
            .iter()
            .position(|e| e.key == key)
            .map(|i| memo.remove(i))
            .unwrap_or(Entry {
                key,
                bounds: None,
                heights: Vec::new(),
            });
        if memo.len() >= 8 {
            memo.remove(0);
        }
        let result = f(&mut entry);
        memo.push(entry);
        result
    })
}
fn build<R>(f: impl FnOnce() -> R) -> R {
    let _span = crate::frame_cost::span("display/heights.resolve");
    BUILDS.with(|n| n.set(n.get() + 1));
    f()
}
pub(crate) fn bounds(
    world: &World,
    setup: Option<&str>,
    tilt: Option<Tilt>,
    fresh: impl FnOnce() -> Option<Aabb>,
) -> Option<Aabb> {
    with_entry(Key::of(world, setup, tilt), |entry| {
        *entry.bounds.get_or_insert_with(|| build(fresh))
    })
}
pub(crate) fn height(
    world: &World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    tilt: Option<Tilt>,
    h: Height,
) -> Result<f64, &'static str> {
    with_entry(Key::of(world, setup, tilt), |entry| {
        if let Some((_, _, value)) = entry
            .heights
            .iter()
            .find(|(s, height, _)| s.as_ref() == src && *height == h)
        {
            return *value;
        }
        let value = build(|| {
            let frame = operation::OpFrame::new(setup.map(str::to_owned), tilt)
                .checked_frame()
                .map_err(|_| "the 3+2 index commands no direction - its angles are not finite")?;
            operation::fold_height(world, setup, src, frame.as_ref(), h).ok_or_else(|| {
                operation::missing_source(world, setup, src, h.reference)
                    .unwrap_or("source missing")
            })
        });
        // Bound memory during long spinbox drags without losing a frame's rows.
        if entry.heights.len() >= 32 {
            let _ = entry.heights.remove(0);
        }
        entry.heights.push((src.cloned(), h, value));
        value
    })
}
#[cfg(test)]
pub(crate) fn builds() -> u64 {
    BUILDS.with(Cell::get)
}

#[cfg(test)]
#[path = "height_memo_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "height_memo_machine_tests.rs"]
mod machine_tests;
