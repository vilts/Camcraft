//! Locate the part in a fixture, or the fixture on the part (#561): a
//! mate list - a part face or point against a fixture face or point -
//! read in the frame of the setup the fixture belongs to, solved as the
//! shop's 3-2-1 ([`solve`]), then a typed nudge. The result is one rigid
//! transform in that setup frame that carries the part onto the fixture;
//! `LocatePart` applies it to the part, `LocateFixture` its inverse to
//! the fixture. Pure: nothing here writes the world.
//!
//! Provenance: the 3-2-1 locating principle and face/point datums from
//! machine-tool and fixture-design handbooks; the interpenetration
//! warning is the shared sampled check (`crate::fixture_clash`, #582).
//! Our own code, no vendor material.
//!
//! ## The laws here
//!
//! - **The fixture's setup is the frame.** Fixtures are setup furniture
//!   and read their own `Location`; a part reads the setup's alignment
//!   (#326) composed on its document placement - the picks are read
//!   exactly where that setup shows them. The work offset is untouched.
//! - **Refused by name, before anything is built.** No such part or
//!   fixture, a pick on the wrong body, an edge or curved face, a face
//!   with no area, two mates on one point, faces that cannot oppose, a
//!   mate the earlier ones contradict, a non-finite nudge.
//! - **Interpenetration warns, it does not refuse.** A modelled vise is
//!   often drawn closed on a nominal part; the move is the user's call
//!   and the gate still judges every toolpath against the steel.

#[path = "fixture_locate_solve.rs"]
mod solve_impl;

pub use solve_impl::{LEN_TOL_MM, LocateOffset, MAX_MATES, NORMAL_TOL, solve};

#[path = "fixture_locate_land.rs"]
mod land_impl;

pub use land_impl::{FACE_AREA_REL_TOL, stock_lands};

use flecs_ecs::prelude::{Entity, World};

pub use crate::fixture_clash::PENETRATION_MM;

use crate::fixture::{self, MACHINE_TABLE};
use crate::fixture_clash;
use crate::geometry::Triangle;
use crate::model;
use crate::operation::GeoSel;
use crate::pick::mate_datum;
use crate::schema::Location;
use crate::setup_align;

/// One mate: a pick on the part against a pick on the fixture.
#[derive(Clone, Debug, PartialEq)]
pub struct Mate {
    pub part: GeoSel,
    pub fixture: GeoSel,
}

/// Which body the locate moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mover {
    /// The part goes into the fixture (the stock follows it).
    #[default]
    Part,
    /// The fixture comes to the part (the part keeps its place).
    Fixture,
}

/// Which body the mates' first picks are on (#2173): the part itself, or
/// the setup's stock round it. A vise holds the blank, so the stock is
/// the Fixtures page's default; the API's default stays the part.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LocateBody {
    /// The part's own faces and points meet the fixture.
    #[default]
    Part,
    /// The faces and points of the stock of the fixture's setup meet the
    /// fixture. The part moves and its stock follows it (the one
    /// stock-follows law), so the part keeps its place in the blank.
    Stock,
}

/// Everything a locate is asked for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LocateSpec {
    /// The part that moves (or that the fixture comes to).
    pub part: String,
    pub fixture: String,
    /// The body the mates' first picks are on.
    pub body: LocateBody,
    pub mates: Vec<Mate>,
    pub offset: LocateOffset,
}

/// A solved locate: the fixture's declared setup binding and the rigid
/// transform (setup frame) that carries the part onto the fixture.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub setup: Option<String>,
    pub delta: Location,
}

/// The fixture by name, refused by name when it is not one.
fn require_fixture(world: &World, name: &str) -> Result<Entity, String> {
    if name == MACHINE_TABLE {
        return Err("the machine table has no faces to mate - import the vise or fixture and mark it".into());
    }
    if name.trim().is_empty() {
        return Err("no fixture chosen - import one and mark it on the Fixtures page".into());
    }
    let e = model::find_by_name(world, name).ok_or_else(|| format!("no fixture named '{name}'"))?;
    if !model::is_fixture(world, &e) {
        return Err(format!("'{name}' is not a fixture - mark it on the Fixtures page first"));
    }
    Ok(e)
}

/// The part by name, refused by name when it is not one.
fn require_part(world: &World, name: &str) -> Result<Entity, String> {
    if name.trim().is_empty() {
        return Err("no part to locate - import a model first".into());
    }
    let e = model::find_by_name(world, name).ok_or_else(|| format!("no model named '{name}'"))?;
    if !setup_align::is_part(world, &e) {
        return Err(format!("'{name}' is not a part - pick the model the fixture holds"));
    }
    Ok(e)
}

/// The stock entity of the setup `fixture` belongs to, when that stock
/// exists (#2173): the body a stock locate mates.
#[must_use]
pub fn stock_of(world: &World, fixture: &str) -> Option<String> {
    let setup = model::find_by_name(world, fixture).and_then(|fe| fixture::def_of(world, &fe)).and_then(|d| d.setup);
    let name = crate::setup::stock_entity_name(world, setup.as_deref());
    model::find_by_name(world, &name).filter(|e| model::is_stock(world, e)).map(|_| name)
}

/// The held body's name and pose in the setup frame, refused by name.
fn held(world: &World, spec: &LocateSpec, part: &Entity, setup: Option<&str>) -> Result<(String, Location), String> {
    match spec.body {
        LocateBody::Part => Ok((spec.part.clone(), setup_align::pose_in(world, part, setup).unwrap_or_default())),
        LocateBody::Stock => {
            let name = stock_of(world, &spec.fixture).ok_or_else(|| {
                format!("the setup of '{}' has no stock to locate - set the stock first, or locate the part", spec.fixture)
            })?;
            let e = model::find_by_name(world, &name).ok_or_else(|| format!("no stock named '{name}'"))?;
            Ok((name, model::transform_of(world, &e).unwrap_or_default()))
        }
    }
}

/// Read and solve a locate. Every refusal names what to change.
pub fn plan(world: &World, spec: &LocateSpec) -> Result<Plan, String> {
    let fe = require_fixture(world, &spec.fixture)?;
    if spec.body == LocateBody::Stock && spec.part.trim().is_empty() {
        return Err("the stock follows its part - name the part (locate from its row on the Model page)".into());
    }
    let pe = require_part(world, &spec.part)?;
    let setup = fixture::def_of(world, &fe).and_then(|d| d.setup);
    let (held, part_pose) = held(world, spec, &pe, setup.as_deref())?;
    let noun = if spec.body == LocateBody::Stock { "stock" } else { "part" };
    let fixture_pose = model::transform_of(world, &fe).unwrap_or_default();
    let mut pairs = Vec::with_capacity(spec.mates.len());
    for (i, m) in spec.mates.iter().enumerate() {
        let k = i + 1;
        if m.part.entity != held {
            return Err(format!("mate {k}: {} is not on the {noun} '{held}'", crate::pick::geo_label(world, &m.part)));
        }
        if m.fixture.entity != spec.fixture {
            return Err(format!(
                "mate {k}: {} is not on the fixture '{}'",
                crate::pick::geo_label(world, &m.fixture), spec.fixture
            ));
        }
        let p = mate_datum(world, &m.part, part_pose).map_err(|e| format!("mate {k}: {e}"))?;
        let f = mate_datum(world, &m.fixture, fixture_pose).map_err(|e| format!("mate {k}: {e}"))?;
        pairs.push((p, f));
    }
    let delta = solve(&pairs, spec.offset)?;
    Ok(Plan { setup, delta })
}

/// One pick read where the locate reads it - the frame of the setup
/// `fixture` belongs to - for the mate list's plain words (#561 review):
/// a face's outward normal, a point's coordinates. Refused by name as
/// [`mate_datum`] refuses.
pub fn read_pick(world: &World, fixture: &str, g: &GeoSel) -> Result<crate::pick::MateDatum, String> {
    let setup = model::find_by_name(world, fixture).and_then(|fe| fixture::def_of(world, &fe)).and_then(|d| d.setup);
    let e = model::find_by_name(world, &g.entity).ok_or_else(|| format!("no model named '{}'", g.entity))?;
    // A stock reads where `plan` reads it: its own placement (#2173).
    let pose = if model::is_stock(world, &e) {
        model::transform_of(world, &e).unwrap_or_default()
    } else {
        setup_align::pose_in(world, &e, setup.as_deref()).unwrap_or_default()
    };
    mate_datum(world, g, pose)
}

/// Is this transform the identity (nothing would move)?
#[must_use]
pub fn is_still(delta: Location) -> bool {
    let p = delta.pos;
    p.x.abs() <= 1e-9 && p.y.abs() <= 1e-9 && p.z.abs() <= 1e-9 && setup_align::is_identity_rot(delta.rot)
}

/// The warning, by name, when the part would sit inside the fixture after
/// `delta` (relative placement - the same whichever body moves). A
/// sampled check: each facet's centre pushed [`PENETRATION_MM`] into its
/// own body, tested inside the other's closed mesh. `None` when clear, or
/// when a mesh is open and cannot say (logged).
#[must_use]
pub fn interpenetration(world: &World, spec: &LocateSpec, plan: &Plan) -> Option<String> {
    let fe = model::find_by_name(world, &spec.fixture)?;
    let pe = model::find_by_name(world, &spec.part)?;
    let part: Vec<Triangle> = setup_align::facets_in(world, &pe, plan.setup.as_deref())?
        .iter()
        .map(|t| fixture_clash::place(*t, plan.delta))
        .collect();
    let steel = model::world_facets(world, &fe)?;
    let hit = fixture_clash::clash(&part, &steel)?;
    Some(fixture_clash::part_message(&spec.part, &spec.fixture, hit, "check the mates and the offset"))
}

/// The warning, by name, when the setup's stock would sink into the
/// fixture where the locate leaves it (#561) - a blank with an allowance
/// under the part mated on the floor goes through the floor. `stocks`
/// are the moved stock bodies (setup frame) with the fixture where it
/// will stand; the same sampled check as [`interpenetration`].
#[must_use]
pub fn stock_interpenetration(world: &World, spec: &LocateSpec, plan: &Plan, stocks: &[(String, Vec<Triangle>)]) -> Option<String> {
    let fe = model::find_by_name(world, &spec.fixture)?;
    let steel = model::world_facets(world, &fe)?;
    let own = crate::setup::stock_entity_name(world, plan.setup.as_deref());
    stocks.iter().filter(|(n, _)| *n == own).find_map(|(name, body)| {
        let hit = fixture_clash::clash(body, &steel)?;
        Some(fixture_clash::stock_message(
            name,
            &spec.fixture,
            hit,
            "check its allowance under the mated faces, or set the stock again",
        ))
    })
}

/// The setup's stock as it stands, carried by `delta` - the relative
/// placement a `LocateFixture` gives it against the moved fixture.
#[must_use]
pub fn stock_carried(world: &World, plan: &Plan) -> Vec<(String, Vec<Triangle>)> {
    let name = crate::setup::stock_entity_name(world, plan.setup.as_deref());
    model::find_by_name(world, &name)
        .and_then(|e| model::world_facets(world, &e))
        .map(|t| vec![(name, t.into_iter().map(|t| fixture_clash::place(t, plan.delta)).collect())])
        .unwrap_or_default()
}

/// Both warnings, joined.
#[must_use]
pub fn join_warnings(a: Option<String>, b: Option<String>) -> Option<String> {
    fixture_clash::join(a.into_iter().chain(b).collect())
}
