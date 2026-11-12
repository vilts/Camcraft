//! The program sim across setups (#317): the machined body one setup
//! leaves is carried into the next through the transform between the
//! two setups (`setup_relative::transform_between`, #316), so a later
//! setup starts from what the earlier one cut instead of a fresh block.
//!
//! Provenance: our own design per #308's research decision (one body,
//! viewed through each setup's transform, never re-sampled onto each
//! setup's grid); the engine is camcraft-sim's `stock_sim::CarriedStock`
//! (#637: the column `Body`, or the field read back as one). Plain
//! re-clamp practice; no vendor material consulted.
//!
//! ## The laws here
//!
//! - **When the body is carried.** By the entered setup's stock source
//!   (#320, `setup_stock`): "what the setup before leaves behind"
//!   carries; "its own stock" keeps the M3.2 A CLIP in the same hold
//!   (the new block's top lowers what stands; nothing moves) and takes a
//!   NEW BLOCK in a different hold - the gouges of the block left behind
//!   fold into the verdict, never dropped. Unpicked, the #317 rule: a
//!   setup holding the part differently, or placed relative to the one
//!   before it (#316's link, "Same as" included), carries.
//! - **The carried body is the incoming stock.** A carried setup's own
//!   stock entity is not read by the sim: the material the machine
//!   holds is what the previous setup left.
//! - **Refuse what is not verified.** A turn the column lattice cannot
//!   carry exactly (side-on, or off the quarter turns) and a lathe or
//!   mill-turn machine refuse by name, naming both setups. Lathe,
//!   rotary and posed programs are refused before the walk, as before.
//! - **One machine (#404).** A carry into a setup whose machine (its own
//!   or the document's, #343) differs from the setup before it refuses
//!   by name ([`CARRY_MACHINE_CHANGE_REFUSAL`]) in [`entry`], so the rest
//!   reading (#318) refuses it too. The program walk refuses every setup
//!   change onto another machine, the clip included: the clip keeps
//!   what the earlier setup left standing. (A rest chain that meets a
//!   clip starts from the setup's own block and reads nothing across, so
//!   it does not refuse.) The machine kind gate reads the two setups'
//!   machine, never the active setup's.

pub(crate) use super::held::{Held, first_carried, first_held};
use crate::stock_sim::{Body, Deviation, LatticeTurn, Planned, SimOp, stock_grid};

/// The refusal for a program that moves the stock between setups on
/// different machines (#404). The program sim judges one machine's
/// program, the post writes one (#343's one-mount law), so the material
/// is never handed across a machine change on a guess.
pub const CARRY_MACHINE_CHANGE_REFUSAL: &str = "the setups are on different machines - the program sim carries the stock only between setups on one machine; simulate each machine's setups on their own";

/// The refusal for a carry on a lathe or mill-turn machine (#321 decided:
/// the part flip / sub-spindle hand-off is not implemented - the column
/// lattice carries milled bodies only, and a turned body lives in the
/// meridian field). Every reader of the carry - the program sim, the rest
/// and judged stock of generation, and so the post - says this by name.
pub const CARRY_MACHINE_REFUSAL: &str = "a part flip or sub-spindle hand-off is not supported yet on a lathe or mill-turn machine";

/// How the program enters a setup from the setup before it.
pub(crate) enum Entry {
    /// Its own stock in the same hold: the conservative clip to the new
    /// block.
    Clip,
    /// The body moves through the transform.
    Carry(LatticeTurn),
    /// Its own stock in a different hold (#320): a new block.
    NewBlock,
}

/// The rotation matrix of a unit quaternion, row-major (`p' = r p`).
fn matrix_of(rot: [f64; 4]) -> [[f64; 3]; 3] {
    use crate::geometry::{Point3, transform_point};
    let col = |e: Point3| transform_point(e, Point3::ZERO, rot);
    let (x, y, z) = (col(Point3::new(1.0, 0.0, 0.0)), col(Point3::new(0.0, 1.0, 0.0)), col(Point3::new(0.0, 0.0, 1.0)));
    [[x.x, y.x, z.x], [x.y, y.y, z.y], [x.z, y.z, z.z]]
}

fn carry_head(from: &str, to: &str) -> String {
    format!("carrying the machined stock from setup '{from}' into setup '{to}'")
}

/// The machine both setups mount (#343: each its own, else the
/// document's), or [`CARRY_MACHINE_CHANGE_REFUSAL`] naming both setups and both
/// machines when they differ (#404).
///
/// # Errors
/// The two setups mount different machines.
pub(crate) fn one_machine(
    world: &flecs_ecs::prelude::World,
    from: &str,
    to: &str,
) -> Result<(String, Option<crate::machine::Machine>), String> {
    use crate::setup_mount::{machine_label, machine_of};
    let (m_from, m_to) = (machine_of(world, Some(from)), machine_of(world, Some(to)));
    if m_from != m_to {
        return Err(format!(
            "{}: {CARRY_MACHINE_CHANGE_REFUSAL} ('{from}' on {}, '{to}' on {})",
            carry_head(from, to),
            machine_label(&m_from),
            machine_label(&m_to),
        ));
    }
    Ok(m_to)
}

/// The lattice turn that carries the body from setup `from` into setup
/// `to`.
///
/// # Errors
/// Setups on different machines (#404), a transform that cannot be
/// carried exactly, a machine kind the carry is not verified on, or a
/// setup `transform_between` refuses - each named with both setups.
pub(crate) fn carry_turn(world: &flecs_ecs::prelude::World, from: &str, to: &str) -> Result<LatticeTurn, String> {
    let head = carry_head(from, to);
    // #404: the machine is the two setups' own (#343), never the active
    // setup's - the body never crosses a machine change. #388: with both
    // ends on that one machine, its kind is both ends' kind - a carry off
    // or onto a machine that is not a mill is the same unverified turn.
    let m_to = one_machine(world, from, to)?;
    if crate::machine::kind(m_to.1.as_ref()) != crate::machine::Kind::Mill {
        return Err(format!("{head}: {CARRY_MACHINE_REFUSAL}"));
    }
    let t = crate::setup_relative::transform_between(world, from, to).map_err(|e| format!("{head}: {e}"))?;
    LatticeTurn::from_matrix(matrix_of(t.rot), [t.pos.x, t.pos.y, t.pos.z]).map_err(|e| {
        if e == crate::stock_sim::CARRY_TURN_REFUSAL {
            // #866: say WHICH turn, in the words the alignment row uses.
            let [x, y, z] = crate::setup_align::angles_of(Some(t));
            format!("{head}: '{to}' holds the part turned X {x}°, Y {y}°, Z {z}° from how '{from}' holds it, and {e}")
        } else {
            format!("{head}: {e}")
        }
    })
}

/// Decide how the program enters setup `to` from setup `from`, by the
/// setup's stock source (#320; `setup_stock::between` - unpicked, the
/// #317 rule).
///
/// # Errors
/// As [`carry_turn`], when the body is to be carried.
pub(crate) fn entry(world: &flecs_ecs::prelude::World, from: Option<&str>, to: Option<&str>) -> Result<Entry, String> {
    use crate::setup_stock::{self, StockSource};
    let (Some(from), Some(to)) = (from, to) else { return Ok(Entry::Clip) };
    match setup_stock::between(world, from, to) {
        StockSource::FromPrevious => carry_turn(world, from, to).map(Entry::Carry),
        StockSource::Own if setup_stock::same_hold(world, from, to) => Ok(Entry::Clip),
        StockSource::Own => Ok(Entry::NewBlock),
    }
}

/// Fold the gouges of a block the program left behind (#320: a later
/// setup took a new block) into the final verdict, so a gouge cut
/// before the new block is never dropped. The cells are on another
/// lattice, so only the counts and the worst depth fold; the worst
/// point stays in the frame of the setup that cut it.
pub(crate) fn fold_left(dev: &mut Deviation, left: &Deviation) {
    dev.gouge_cells += left.gouge_cells;
    dev.gouge_volume += left.gouge_volume;
    if left.max_gouge > dev.max_gouge {
        dev.max_gouge = left.max_gouge;
        dev.max_gouge_at = left.max_gouge_at;
    }
}

/// Where the program's walk stands: the stock, the setup frame it is
/// held in, and whether the program crossed into another setup.
pub(crate) struct Walked {
    pub held: Held,
    pub frame: Option<String>,
    pub spanned: bool,
    /// The setup whose stock the plain grid (or the body) was first
    /// built from: the program's first setup, or the last setup that
    /// took a new block (#320).
    pub origin: Option<String>,
    /// The gouges of blocks left behind for a new block (#320), folded.
    pub left: Option<Deviation>,
}

/// Walk the program through the stock, carrying or clipping at every
/// setup change. `until`: stop on entering that setup (its first op not
/// cut), returning the stock it starts from.
pub(crate) fn walk(
    world: &flecs_ecs::prelude::World,
    planned: &[Planned],
    until: Option<&str>,
) -> Result<Walked, String> {
    let min_r = planned.iter().map(|p| p.radius).fold(f64::INFINITY, f64::min);
    // The block the program starts on is the FIRST setup's stock - the
    // implicit-setup law makes that the legacy `Stock` entity exactly,
    // so a single-setup document walks this identical to before.
    let first = planned.first().and_then(|p| p.setup.as_deref());
    let first_eff = crate::setup::effective(world, first);
    let mut held = first_held(world, first, min_r)?;
    let mut frame = first_eff.clone();
    let mut origin = first_eff;
    let mut left: Option<Deviation> = None;
    let mut spanned = false;
    for p in planned {
        let eff = crate::setup::effective(world, p.setup.as_deref());
        if eff != frame {
            spanned = true;
            match entry(world, frame.as_deref(), eff.as_deref())? {
                Entry::Clip => {
                    // #404: material never crosses a machine change. The
                    // carry refuses in `carry_turn`; the clip keeps what the
                    // earlier setup left standing, so it hands material
                    // across too and refuses here. A new block hands nothing
                    // across, so it may start on another machine.
                    if let (Some(from), Some(to)) = (frame.as_deref(), eff.as_deref()) {
                        one_machine(world, from, to)?;
                    }
                    clip_to_setup_stock(&mut held, world, eff.as_deref(), min_r)?;
                }
                Entry::Carry(turn) => {
                    held = Held::Carried(held.into_carried(world, origin.as_deref())?.carry(&turn)?);
                }
                Entry::NewBlock => {
                    let dev = judge(world, &held, frame.as_deref())?;
                    if dev.gouge_cells > 0 {
                        match left.as_mut() {
                            Some(l) => fold_left(l, &dev),
                            None => left = Some(dev),
                        }
                    }
                    held = first_held(world, eff.as_deref(), min_r)?;
                    origin = eff.clone();
                }
            }
            frame = eff;
        }
        if until.is_some() && frame.as_deref() == until {
            return Ok(Walked { held, frame, spanned, origin, left });
        }
        let sim_ops = [SimOp { moves: &p.moves, radius: p.radius, ball: p.ball, cone: p.cone, corner: p.corner }];
        held.simulate(&sim_ops)?;
    }
    if let Some(u) = until {
        return Err(format!("the program never enters setup '{u}'"));
    }
    Ok(Walked { held, frame, spanned, origin, left })
}

/// The verdict of the stock as held, against the parts in `frame`.
pub(crate) fn judge(world: &flecs_ecs::prelude::World, held: &Held, frame: Option<&str>) -> Result<Deviation, String> {
    let facets = crate::setup_align::parts_facets_in(world, frame);
    // #864: the same tightened tolerance the final verdict is read at.
    let tol = crate::model_fidelity::verdict_tol(world)?;
    held.deviate(&facets, tol)
}

/// The stock setup `setup` starts from when the program reaches it: the
/// body the setups before it leave, in `setup`'s frame (#317; #320's
/// incoming-stock display and #318's rest read it). The program's first
/// setup starts from its own stock.
///
/// # Errors
/// Every refusal of the program sim, and a setup the program never
/// enters.
pub fn incoming_body(world: &flecs_ecs::prelude::World, setup: &str) -> Result<Body, String> {
    let planned = crate::stock_sim::plan_program(world)?;
    crate::stock_sim::refuse_unjudged(&planned)?;
    let w = walk(world, &planned, Some(setup))?;
    w.held.into_carried(world, w.origin.as_deref())?.into_body()
}

/// Fold a newly-entered setup's stock into the standing grid: inside
/// the new block, no column may stand above that block's top; outside,
/// what the previous setup left stands untouched - the conservative
/// clip for two setups that hold the part alike, not the carry (#317). A setup with no stock entity yet clips
/// nothing; a rotated or from-model stock refuses, per the same law.
fn clip_to_setup_stock(
    held: &mut Held,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    tool_r: f64,
) -> Result<(), String> {
    use crate::model;
    use crate::stock::StockShape;
    let Some(entity) = crate::setup::stock_entity(world, setup) else {
        return Ok(());
    };
    if matches!(model::stock_def_of(world, &entity), Some(StockShape::FromModel { .. })) {
        return Err(
            "the program sim cannot clip to a later setup's from-model stock - choose what the setup before it leaves behind as its stock so the machined stock is carried instead"
                .into(),
        );
    }
    // The column law (`clip_columns_under`); on the field arm the same
    // law as a solid (`FieldBody::clip_under`, #637).
    held.clip_under(&stock_grid(world, setup, tool_r)?)
}
