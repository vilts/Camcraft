//! MoveModel: place a part in the document - position plus any rigid
//! orientation (M5.2 stage G, "part orientation"). The full rotation the
//! ROADMAP deferred from M3.2 ("placement flip; full rotation M5.2") is
//! exactly this command's extra two angles: the data model always
//! carried a quaternion and every geometry read applies it, so the
//! rotation became an editor change, not a schema change.
//!
//! Provenance: our own design; a workpiece sitting in the machine at an
//! arbitrary orientation (trunnied, tilted on a sine plate) is plain
//! shop practice, and R = Rz ⊗ Ry ⊗ Rx is the standard yaw-pitch-roll
//! composition `SetWcsOrigin` already speaks.
//!
//! ## The laws this command keeps
//!
//! - **Placement is a Location, validated at construction.** Same
//!   chokepoint as every pose write (`check_location`, shared from the
//!   fixture module so there is one law): a finite position and a
//!   quaternion that normalizes, or the command never enters history.
//! - **The model's `Location` is the single truth for its geometry.**
//!   The same rigid placement change carries stock made from that model,
//!   or the stock of a document with one part. Existing operation stamps
//!   read the changed placements and become stale.
//! - **Pages own their verbs.** A fixture is placed on the Fixtures
//!   page (`MoveFixture`), a work offset on the Work Offsets page
//!   (`SetWcsOrigin`), and the machine table is derived prose - all
//!   refused by name, because silently accepting them here would put
//!   two commands on one piece of furniture and split the undo story.
//! - **A turn keeps a square stock square (#561, #571).** Any move that
//!   turns the part - a locate or a Model-page turn alike, quarter turns
//!   included - re-fits a box or cylinder stock that stands square to the
//!   setup round the part's new place with the allowance it had on each
//!   side (`crate::stock_follow`) instead of turning it with the part. A
//!   move that does not turn the part carries the stock rigidly, exactly
//!   as before; a stock copied from the model or already off the axes
//!   follows rigidly on every move.
//! - **A move into the steel warns (#582).** Where the move leaves the
//!   part, or a stock that follows it, inside a fixture of any setup, the
//!   command carries the locate's by-name warning (`crate::fixture_clash`):
//!   the body, the fixture, a point. A warning, never a refusal; no
//!   fixtures, no check.
//! - **`apply` and `invert` are pure writes**, so redo replays bytes
//!   and never re-derives.

use std::sync::Arc;

use flecs_ecs::prelude::World;

use crate::commands::fixture::check_location;
use crate::commands::{Command, CommandError};
use crate::fixture::MACHINE_TABLE;
use crate::geometry::{Point3, Triangle, quat_conjugate, quat_mul, transform_inverse_point, transform_point};
use crate::model;
use crate::schema::Location;
use crate::setup_align;
use crate::schema::StockDef;
use crate::stock::{self, StockShape};
use crate::stock_follow;

/// The part alignment of the setup that names `stock` as its stock, when
/// that setup holds the part turned (#326).
fn aligned_owner(world: &World, stock: &str) -> Option<Location> {
    crate::setup::names(world)
        .into_iter()
        .filter(|s| crate::setup::def_of(world, s).is_some_and(|d| d.stock == stock))
        .find_map(|s| setup_align::stored(world, &s))
}

/// A following stock re-sized by a locate: (shape, mesh) before and after.
type Reshape = ((StockShape, Arc<[Triangle]>), (StockShape, Arc<[Triangle]>));

/// One following stock: name, placement before and after, and its new
/// size when a locate re-fitted it.
struct StockMove {
    name: String,
    before: Location,
    after: Location,
    reshape: Option<Reshape>,
}

/// Does going from `prev` to `next` turn the part? (q and -q are one turn.)
fn turns(prev: [f64; 4], next: [f64; 4]) -> bool {
    let d = quat_mul(next, quat_conjugate(prev));
    d[..3].iter().map(|v| v * v).sum::<f64>().sqrt() > 1e-12
}

/// The part's vertices placed at `at`.
fn placed(tris: &[Triangle], at: Location) -> Vec<Point3> {
    tris.iter().flat_map(|t| [t.a, t.b, t.c]).map(|p| transform_point(p, at.pos, at.rot)).collect()
}

/// Place a model: the same rigid transform any page sets, applied to
/// the named part. Construction snapshots the previous `Location` for
/// the inverse.
pub struct MoveModel {
    name: String,
    prev: Location,
    next: Location,
    stock: Vec<StockMove>,
    warning: Option<String>,
}

impl MoveModel {
    /// The move a locate makes (#561) - since #571 the same move as
    /// [`MoveModel::new`]: every turn keeps a square stock square. The
    /// locate runs its own (mate-worded) interpenetration check, so this
    /// one runs none.
    pub fn locating(world: &World, name: &str, location: Location) -> Result<Self, CommandError> {
        Self::build(world, name, location, false)
    }

    /// A Model-page move or turn. Since #582 it runs the locate's sampled
    /// part-and-stock-versus-fixture check and carries a by-name warning
    /// ([`MoveModel::warning`]) - never a refusal.
    pub fn new(world: &World, name: &str, location: Location) -> Result<Self, CommandError> {
        Self::build(world, name, location, true)
    }

    fn build(world: &World, name: &str, location: Location, check: bool) -> Result<Self, CommandError> {
        if name == MACHINE_TABLE {
            return Err(CommandError::Invalid(
                "the machine table is derived from the machine file - it is not a part you can move".into(),
            ));
        }
        let entity =
            model::find_by_name(world, name).ok_or_else(|| CommandError::NotFound(name.into()))?;
        if model::is_fixture(world, &entity) {
            return Err(CommandError::Invalid(format!(
                "'{name}' is a fixture - the Fixtures page places it"
            )));
        }
        if model::is_wcs(world, &entity) {
            return Err(CommandError::Invalid(format!(
                "'{name}' is a work offset - the Work Offsets page places it"
            )));
        }
        // #1862: a DXF profile lies on its stock face, not in the world.
        if crate::sketch::is_sketch(world, &entity) {
            return Err(CommandError::Invalid(format!(
                "'{name}' is a DXF profile - the Stock page places it on its stock face"
            )));
        }
        let next = check_location(location, &format!("'{name}'"))?;
        let prev = model::transform_of(world, &entity).unwrap_or_default();
        // #571: only a move that turns the part re-fits a square stock; a
        // pure translation keeps the rigid follow, bytes for bytes.
        let part_tris = if turns(prev.rot, next.rot) { model::facets_of(world, &entity) } else { None };
        let single_part = model::model_entities(world)
            .iter()
            .filter(|(_, entity)| !model::is_fixture(world, entity))
            .count()
            == 1;
        let mut stock = Vec::new();
        for (stock_name, entity) in model::named_entities(world) {
            let Some(shape) = model::stock_def_of(world, &entity) else { continue };
            let follows = match &shape {
                StockShape::FromModel { source } => source == name,
                _ => single_part,
            };
            if !follows {
                continue;
            }
            let before = model::transform_of(world, &entity).unwrap_or_default();
            let owner = aligned_owner(world, &stock_name);
            // #561/#571: a turn keeps a square block square, re-fitted
            // round the part as the stock's setup sees it.
            if let Some(tris) = &part_tris {
                let seen = |l: Location| owner.map_or(l, |a| setup_align::compose(a, l));
                let fit = stock_follow::keep_square(
                    &stock_name,
                    &shape,
                    before,
                    &placed(tris, seen(prev)),
                    &placed(tris, seen(next)),
                )
                .map_err(CommandError::Invalid)?;
                if let Some((sized, at)) = fit {
                    let after = check_location(at, &format!("stock '{stock_name}'"))?;
                    let reshape = (sized != shape)
                        .then(|| {
                            let unbuilt = || {
                                CommandError::Invalid(format!("the stock '{stock_name}' cannot be rebuilt round the moved part"))
                            };
                            let old = model::facets_of(world, &entity)
                                .or_else(|| stock::stock_mesh(&shape).map(Arc::from))
                                .ok_or_else(unbuilt)?;
                            let mesh = stock::stock_mesh(&sized).filter(|_| sized.is_bounded()).ok_or_else(unbuilt)?;
                            Ok::<_, CommandError>(((shape.clone(), old), (sized, Arc::from(mesh))))
                        })
                        .transpose()?;
                    stock.push(StockMove { name: stock_name, before, after, reshape });
                    continue;
                }
            }
            // #326: a stock placed in a setup that holds the part turned
            // follows the move as that setup sees it - the document move
            // conjugated by the setup's alignment.
            let after = if let Some(a) = owner {
                let delta = setup_align::compose(next, setup_align::inverse(prev));
                let seen = setup_align::compose(a, setup_align::compose(delta, setup_align::inverse(a)));
                setup_align::compose(seen, before)
            } else {
                let local = transform_inverse_point(before.pos, prev.pos, prev.rot);
                Location {
                    pos: transform_point(local, next.pos, next.rot),
                    rot: quat_mul(quat_mul(next.rot, quat_conjugate(prev.rot)), before.rot),
                }
            };
            // #2177: a move that does not turn the part leaves the stock's
            // turn as it was - the composed quaternions' float noise (a
            // locate's solve, q * q^-1) never lands on a square stock.
            let after = if turns(prev.rot, next.rot) { after } else { Location { rot: before.rot, ..after } };
            let after = check_location(after, &format!("stock '{stock_name}'"))?;
            stock.push(StockMove { name: stock_name, before, after, reshape: None });
        }
        let mut command = MoveModel { name: name.to_string(), prev, next, stock, warning: None };
        if check {
            command.warning = crate::fixture_clash::after_part_move(world, name, next, &command.moved_stock(world));
        }
        Ok(command)
    }

    /// The interpenetration warning (#582), by name: the part or its
    /// stock would sit inside a fixture where this move leaves it.
    #[must_use]
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    /// The stock this move carries with the part (stock made from it, or
    /// the stock of a one-part document), by name. The MCP `move_model`
    /// tool (#716) reports them, because a part move that also moves the
    /// stock is easy to miss from the part's own pose.
    pub fn following_stock(&self) -> impl Iterator<Item = &str> {
        self.stock.iter().map(|m| m.name.as_str())
    }

    /// Each following stock's facets where the move leaves it (setup
    /// frame) - what the locate's interpenetration check reads.
    #[must_use]
    pub fn moved_stock(&self, world: &World) -> Vec<(String, Vec<Triangle>)> {
        self.stock
            .iter()
            .filter_map(|m| {
                let mesh = match &m.reshape {
                    Some((_, (_, mesh))) => mesh.clone(),
                    None => model::facets_of(world, &model::find_by_name(world, &m.name)?)?,
                };
                let at = m.after;
                let tris = mesh
                    .iter()
                    .map(|t| Triangle::new(
                        transform_point(t.a, at.pos, at.rot),
                        transform_point(t.b, at.pos, at.rot),
                        transform_point(t.c, at.pos, at.rot),
                    ))
                    .collect();
                Some((m.name.clone(), tris))
            })
            .collect()
    }
}

/// Write one stock's placement and, when it was re-sized, its block.
fn write_stock(world: &World, name: &str, at: Location, block: Option<&(StockShape, Arc<[Triangle]>)>) {
    if let Some(entity) = model::find_by_name(world, name) {
        model::set_transform(world, &entity, at);
        if let Some((shape, mesh)) = block {
            model::set_facets(world, &entity, mesh.clone());
            entity.entity_view(world).set(StockDef { shape: shape.clone() });
        }
    }
}

impl Command for MoveModel {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::set_transform(world, &entity, self.next);
        }
        for m in &self.stock {
            write_stock(world, &m.name, m.after, m.reshape.as_ref().map(|(_, after)| after));
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::set_transform(world, &entity, self.prev);
        }
        for m in &self.stock {
            write_stock(world, &m.name, m.before, m.reshape.as_ref().map(|(before, _)| before));
        }
    }
}
