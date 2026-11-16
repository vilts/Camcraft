//! Fixture modeling: the workholding hardware a part rides on (M3.2
//! stage B). A fixture is an ordinary model - a STEP the user loaded -
//! wearing a [`FixtureDef`] marker; the machine's own table is derived
//! from the loaded `.mch` and is never an entity.
//!
//! Provenance: our own domain model, specced from general machining
//! knowledge and machine-tool handbooks - vises and chucks are the
//! shop's parts, so the program imports them rather than inventing
//! parametric stand-ins. The table is the machine's stated travel box.
//! No vendor geometry, model, or implementation was inspected.
//!
//! ## The laws here
//!
//! - **A fixture is a model.** Its geometry is the `Facets` the importer
//!   put there, its pose is the entity `Location` - the same two
//!   components every part uses. The marker carries only the setup
//!   binding. There is no fixture-shaped geometry builder: whatever
//!   `boxes_for` reports for a tagged model is the world AABB of its
//!   loaded mesh, exactly what the part itself would report.
//! - **The machine table is derived, never stored.** It exists only
//!   while a machine file states its XY travel and Z bottom: footprint =
//!   the X/Y spans, top = the Z-travel minimum (the lowest the quill can
//!   reach is the surface work stands on), extending downward by
//!   [`MAX_FIXTURE_DIM`] - its underside can never be approached from
//!   below, so its depth is law-irrelevant and stated as the module's
//!   own absurd-dimension cap. No machine, or unstated limits, or a
//!   table with them: the unstated-limit law, exactly as the limit
//!   checker reads it.
//! - **World boxes are corner-AABBs: conservative by construction.**
//!   The AABB of transformed geometry is never smaller than the geometry,
//!   and for a rotated fixture it is a slightly LARGER box than the tilted
//!   reality. Fixtures are never gouge-relevant (they are not cut
//!   material), so a false hit merely annoys while a false negative is a
//!   silent crash: every tolerance here errs toward HIT.
//! - **The mesh acquits, the box convicts (#339).** The box laws stay
//!   the broad phase; a tagged model's closed world mesh
//!   ([`meshes_for`]) then clears the moves its box flagged but its
//!   real steel does not meet - a path down a vise's gap, upright, posed
//!   or under a rotary (slice 3, `verify::mesh_posed`). The mesh
//!   never adds a hit, and an open or refused mesh keeps its box
//!   verdict (`camcraft_sim::verify::mesh_refined`).
//! - **Steel is not kerf.** A retract through its own hole in STOCK is
//!   safe (the material was removed); a retract or rapid dipping through
//!   a fixture box is a crash, so the rapid law's vertical exemption
//!   does not carry over - see [`rapid_hits_box`].
//! - **The strict-dip z law.** A rapid whose span only TOUCHES a box top
//!   within tolerance is not reported: stock seated exactly on a table
//!   top would otherwise fire on every retract, and mating surfaces
//!   contact by design. The dip must exceed [`FIXTURE_HIT_TOL`] to
//!   count - stated because it is the one place this module errs toward
//!   SILENCE, and the margin is a float-dust tolerance, not clearance.
//! - **Workholding never rides the stamp.** Fixtures, the table box and
//!   the spindle band are LIVE gate inputs, exactly like the M2.3 machine
//!   limits: generation reports what the steel says right then, the post
//!   re-checks here and now ([`crate::posting::verification_gate`]), and
//!   no fixture or machine action can stale an otherwise fresh toolpath.
//!   "Load the roomy machine, post without re-generating" is the law this
//!   keeps; a moved vise is caught at the gate, never smuggled in as a
//!   stamp difference.

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::{Entity, World};

pub use camcraft_sim::fixture::*;
pub use camcraft_sim::fixture_mesh::MeshedFixture;

use crate::geometry::Point3;
use crate::schema::FixtureDef;
use crate::{machine, model, setup};

/// The marker component of a fixture entity, if it wears one.
#[must_use]
pub fn def_of(world: &World, entity: &Entity) -> Option<FixtureDef> {
    entity
        .entity_view(world)
        .try_get::<&FixtureDef>(|d| d.clone())
}

/// Every tagged model sharing `declared`'s EFFECTIVE setup (the
/// implicit-setup law resolves both sides), name-sorted - THE entity
/// list of stage B: boxes, digests, lists and the fixtures page all
/// walk this, so they can never disagree about who is a fixture.
#[must_use]
pub fn tagged(world: &World, declared: Option<&str>) -> Vec<(String, Entity)> {
    let want = setup::effective(world, declared);
    model::named_entities(world)
        .into_iter()
        .filter(|(_, e)| {
            model::is_fixture(world, e)
                && setup::effective(
                    world,
                    def_of(world, e).and_then(|d| d.setup).as_deref(),
                ) == want
        })
        // A tagged entity that is also setup furniture (a hand-corrupted
        // document) is not workholding steel - it is someone's stock.
        .filter(|(_, e)| model::is_model(world, e))
        .collect()
}

/// The machine table's box: the stated X/Y travel spans, topped at the
/// stated Z-travel minimum. Any of the three unstated - or no machine
/// loaded - means no table (the unstated-limit law; a machine whose Z
/// axis does not reach down is not a machine we can collide-check).
/// The table of the machine the `declared` setup context mounts (#388):
/// an inactive setup's ops are checked against their own machine's.
#[must_use]
pub fn table_box(world: &World, declared: Option<&str>) -> Option<FixtureBox> {
    table_of(&machine::of_setup(world, declared).1?)
}

/// [`table_box`] of one machine, read straight off its axes - for a
/// caller that already holds the machine (the MCP machine tools name
/// the document's own, which no setup context resolves to).
#[must_use]
pub fn table_of(m: &machine::Machine) -> Option<FixtureBox> {
    let span = |n: &str| {
        m.axes
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(n))
            // #373: an inverted axis's stroke, mapped into the program frame.
            .and_then(machine::AxisLimits::program_travel)
    };
    let (x, y, z) = (span("X")?, span("Y")?, span("Z")?);
    if !(x.0.is_finite() && x.1.is_finite() && y.0.is_finite() && y.1.is_finite() && z.0.is_finite())
    {
        return None;
    }
    Some(FixtureBox {
        min: Point3::new(x.0.min(x.1), y.0.min(y.1), z.0 - MAX_FIXTURE_DIM),
        max: Point3::new(x.0.max(x.1), y.0.max(y.1), z.0),
    })
}

/// The collision truth for one setup context: every box standing in it -
/// the tagged models' transformed AABBs, name-order, then the derived
/// table. The verify pass hands this straight to
/// [`crate::verify::VerifyContext::fixtures`].
#[must_use]
pub fn boxes_for(world: &World, declared: Option<&str>) -> Vec<FixtureBox> {
    named_boxes_for(world, declared).into_iter().map(|(_, b)| b).collect()
}

/// [`boxes_for`] with each box's name beside it, in the same order: the
/// tagged model's name, then [`MACHINE_TABLE`] for the derived table.
/// The link planner (#339 slice 5) names the steel a link cannot clear.
#[must_use]
pub fn named_boxes_for(world: &World, declared: Option<&str>) -> Vec<(String, FixtureBox)> {
    let mut out: Vec<(String, FixtureBox)> = tagged(world, declared)
        .into_iter()
        .filter_map(|(name, e)| steel_box(world, &e).map(|b| (name, b)))
        .collect();
    out.extend(table_box(world, declared).map(|b| (MACHINE_TABLE.to_string(), b)));
    out
}

/// One tagged model's collision box: its world AABB - grown on every
/// side by its chord error when it was imported coarser than the STEP
/// importer promises (#864). The mesh's vertices lie on the true steel,
/// so a curved face can stand that far outside the box of its facets.
fn steel_box(world: &World, e: &Entity) -> Option<FixtureBox> {
    let b = model::entity_world_aabb(world, e)?;
    // Full fidelity: the box itself, bit for bit (no `+ 0.0`).
    // #1056: unknown fidelity has no honest growth; the gate refuses the
    // program by name (`model_fidelity::gate_problems`), so the plain box
    // stands in rather than one that collides with everything.
    let Some(g) = crate::model_fidelity::chord_of(world, e).filter(|g| !crate::model_fidelity::is_unknown(*g)) else {
        return Some(FixtureBox { min: b.min, max: b.max });
    };
    Some(FixtureBox {
        min: Point3::new(b.min.x - g, b.min.y - g, b.min.z - g),
        max: Point3::new(b.max.x + g, b.max.y + g, b.max.z + g),
    })
}

/// [`boxes_for`] split by whether the steel turns with the part (M13.8
/// (2)): `.0` the tagged models riding the rotary (the default), `.1`
/// those declared fixed, then the machine table - a table never turns.
/// Both lists together are exactly [`boxes_for`]'s boxes.
#[must_use]
pub fn workholding_for(world: &World, declared: Option<&str>) -> (Vec<FixtureBox>, Vec<FixtureBox>) {
    let (mut turning, mut fixed) = (Vec::new(), Vec::new());
    for (_, e) in tagged(world, declared) {
        let Some(b) = steel_box(world, &e) else { continue };
        if def_of(world, &e).is_some_and(|d| d.fixed) {
            fixed.push(b);
        } else {
            turning.push(b);
        }
    }
    fixed.extend(table_box(world, declared));
    (turning, fixed)
}

/// The tagged models of the `declared` setup context no collision box
/// can be read for (no finite geometry), by name (#1037): steel every
/// box law passes over, so a law that must not pass it silently names it.
#[must_use]
pub fn unreadable_for(world: &World, declared: Option<&str>) -> Vec<String> {
    tagged(world, declared).into_iter().filter(|(_, e)| steel_box(world, e).is_none()).map(|(n, _)| n).collect()
}

/// Every tagged model's world mesh, paired with the very box
/// [`boxes_for`] reports for it (#339): the mesh law's input. The BVH is
/// built only when a box law first flags that fixture
/// ([`camcraft_sim::fixture_mesh::MeshedFixture::mesh`]).
#[must_use]
pub fn meshes_for(world: &World, declared: Option<&str>) -> Vec<MeshedFixture> {
    tagged(world, declared)
        .into_iter()
        // #864: a fixture imported coarser than promised is judged by its
        // (grown) box alone - its mesh is never trusted to acquit a hit.
        .filter(|(_, e)| crate::model_fidelity::chord_of(world, e).is_none())
        .filter_map(|(name, e)| {
            let b = model::entity_world_aabb(world, &e)?;
            let tris = model::world_facets(world, &e)?;
            Some(MeshedFixture::new(name, FixtureBox { min: b.min, max: b.max }, tris.into()))
        })
        .collect()
}

/// Log every fixture whose mesh was asked to judge and refused (#339):
/// it was judged by its box instead - conservative, but the user should
/// know why a path through its gap still reports.
pub fn log_mesh_refusals(fixtures: &[MeshedFixture]) {
    for f in fixtures {
        if let Some(why) = f.refusal() {
            tracing::info!(fixture = %f.name, "fixture judged by its bounding box: {why}");
        }
    }
}

/// The rotary axis line a rotary kind turns its part about, in the
/// world frame its cached path is written in: re-fitted from the source
/// model's world facets (the fit is a pure function of the facets, the
/// generator's own). None for a non-rotary kind, a stock or selection
/// source, or a refused fit: no machine frame is stated, and fixed
/// steel keeps the path-frame verdict - the conservative one.
#[must_use]
pub fn rotary_axis_of(
    world: &World,
    op: &crate::schema::Operation,
) -> Option<camcraft_sim::verify::RotaryAxis> {
    use crate::operation::{OpParams, SrcRef};
    if !matches!(
        op.params,
        OpParams::RotaryContour { .. } | OpParams::RotaryParallel { .. } | OpParams::RotaryOffset { .. }
    ) {
        return None;
    }
    let SrcRef::Entity(name) = &op.src else { return None };
    let e = model::find_by_name(world, name)?;
    let facets = model::world_facets(world, &e)?;
    let ax = crate::multiaxis::axis::fit(&facets).ok()?;
    Some(camcraft_sim::verify::RotaryAxis { origin: ax.origin, dir: [ax.dir.x, ax.dir.y, ax.dir.z] })
}

/// Every name that occupies the context: the tagged models, then the
/// table's virtual name when the machine states one. The fixtures page
/// and the setup sheet draw this list; the commands treat the table row
/// as read-only prose.
#[must_use]
pub fn list(world: &World, declared: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = tagged(world, declared).into_iter().map(|(n, _)| n).collect();
    if table_box(world, declared).is_some() {
        out.push(MACHINE_TABLE.to_string());
    }
    out
}

