//! `AlignPartToSetup` (#326): set how the part sits in one setup - the
//! rigid transform from the document frame into the setup frame - or
//! clear it back to "as modelled". The work offset is not touched: it is
//! a plain translation placed in the setup frame (`SetSetupWorkOffset`,
//! `SetWcsOrigin`).
//!
//! Provenance: our own design; re-clamping a part flipped or turned on
//! the table is general shop practice, and the transform math is
//! standard rigid-body algebra. No vendor implementation inspected.
//!
//! ## The laws this command keeps
//!
//! - **Validated and snapshotted at construction.** A finite position
//!   and a quaternion that normalizes (`check_location`), a live setup,
//!   a change from what is stored. `apply`/`invert` are pure writes.
//! - **Identity is absence.** An alignment that does nothing is stored
//!   as `None`, so "As modelled" restores the pre-#326 bytes.
//! - **The stock follows like it follows `MoveModel`.** A stock made
//!   from a part, or the stock of a one-part document, is re-placed by
//!   the change and rebuilt square to the setup frame (a box or a
//!   cylinder re-fits to the turned block's bounds). A cylinder whose
//!   axis would leave the vertical, or a following stock another setup
//!   shares, is refused by name - never silently left behind.
//! - **The presets pivot on the parts' centre.** A preset turns the
//!   parts about the centre of their bounds, so a flip leaves them on the
//!   same spot of the table. Typed angles ([`AlignPartToSetup::turn`],
//!   #606) pivot on the setup's WCS origin by default - the part keeps
//!   its relation to the origin - or on the centre, by choice.
//! - **Picks and the page's turns compose (#738).** A face, three
//!   points, the in-plane axes and [`AlignPartToSetup::turn_by`] each move
//!   the part from the pose the setup holds now. [`AlignPartToSetup::new`]
//!   takes any rigid transform - the door the face pick (#330) uses,
//!   through [`AlignPartToSetup::lay_on_plane`], three points (#555),
//!   through [`AlignPartToSetup::lay_on_points`], and the in-plane axis
//!   picks (#557), through [`AlignPartToSetup::turn_in_plane`].
//! - **Linked setups follow (#316).** Every setup placed relative to
//!   this one (directly or through others, [`crate::setup_relative`])
//!   keeps its transform to it: the command re-places each of them, with
//!   its following stock, in the same undo unit - or refuses the whole
//!   change, naming the setup that cannot follow.

use flecs_ecs::prelude::World;

use crate::commands::fixture::check_location;
use crate::commands::{Command, CommandError};
use crate::geometry::{Point3, Triangle, transform_point};
use crate::model;
use crate::schema::{Location, SetupDef, StockDef};
use crate::setup_align::{self, AlignPreset};
use crate::setup_turn::TurnPivot;

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)

use super::align_stock::{StockState, following_stock};

/// The setup's (held, new) alignment for `alignment`, normalised (the
/// identity is "as modelled"); refused when nothing would change.
fn change_of(
    world: &World,
    setup: &str,
    alignment: Option<Location>,
) -> Result<(Option<Location>, Option<Location>), CommandError> {
    let entity = model::find_by_name(world, setup).ok_or_else(|| CommandError::NotFound(setup.to_string()))?;
    let prev = entity
        .entity_view(world)
        .try_get::<&SetupDef>(|d| d.part_alignment)
        .ok_or_else(|| CommandError::Invalid(format!("'{setup}' is not a setup")))?;
    let next = match alignment {
        None => None,
        Some(a) => {
            let a = check_location(a, "the part alignment")?;
            let still = a.pos == Point3::ZERO && setup_align::is_identity_rot(a.rot);
            (!still).then_some(a)
        }
    };
    if next == prev {
        return Err(CommandError::Invalid(format!("setup '{setup}' already holds the part that way")));
    }
    Ok((prev, next))
}

/// One setup's alignment change, with its following stock.
struct Step {
    setup: String,
    prev: Option<Location>,
    next: Option<Location>,
    /// The following stock: entity name, before, after.
    stock: Option<(String, StockState, StockState)>,
}

/// Write (or clear) a setup's part alignment.
pub struct AlignPartToSetup {
    main: Step,
    /// #316: the setups linked below this one, re-placed with it (parents
    /// before children).
    carried: Vec<Step>,
}

impl AlignPartToSetup {
    /// Any rigid transform document -> setup frame; `None` clears.
    pub fn new(world: &World, setup: &str, alignment: Option<Location>) -> Result<Self, CommandError> {
        Self::build(world, setup, alignment, false)
    }

    /// The alignment a locate writes (#561): this setup's square box or
    /// cylinder stock keeps its allowance on each side round the part
    /// (`crate::stock_follow`) instead of re-fitting the turned block.
    pub fn locating(world: &World, setup: &str, alignment: Option<Location>) -> Result<Self, CommandError> {
        Self::build(world, setup, alignment, true)
    }

    fn build(world: &World, setup: &str, alignment: Option<Location>, square: bool) -> Result<Self, CommandError> {
        let (prev, next) = change_of(world, setup, alignment)?;
        let stock = following_stock(world, setup, prev, next, square)?;
        let mut carried = Vec::new();
        for (child, c_prev, c_next) in crate::setup_relative::carried(world, setup, prev, next) {
            let follow = |e: CommandError| {
                let why = match e {
                    CommandError::Invalid(m) => m,
                    other => other.to_string(),
                };
                CommandError::Invalid(format!(
                    "setup '{child}' is placed relative to '{setup}' and must follow the change, but {why}"
                ))
            };
            let c_next = c_next.map(|a| check_location(a, "the carried part alignment")).transpose().map_err(follow)?;
            let stock = following_stock(world, &child, c_prev, c_next, false).map_err(follow)?;
            carried.push(Step { setup: child, prev: c_prev, next: c_next, stock });
        }
        Ok(AlignPartToSetup { main: Step { setup: setup.to_string(), prev, next, stock }, carried })
    }

    /// The main setup's following stock where the change leaves it
    /// (setup frame) - what a locate's interpenetration check reads.
    #[must_use]
    pub fn moved_stock(&self) -> Vec<(String, Vec<Triangle>)> {
        self.main
            .stock
            .iter()
            .map(|(name, _, (_, at, mesh))| {
                let tris = mesh
                    .iter()
                    .map(|t| Triangle::new(
                        transform_point(t.a, at.pos, at.rot),
                        transform_point(t.b, at.pos, at.rot),
                        transform_point(t.c, at.pos, at.rot),
                    ))
                    .collect();
                (name.clone(), tris)
            })
            .collect()
    }

    /// How many linked setups this change re-places with it (#316).
    #[must_use]
    pub fn carried_count(&self) -> usize {
        self.carried.len()
    }

    /// Would [`Self::lay_on_plane`] be accepted? The same refusals, by
    /// name ("already holds the part that way" included), without
    /// building the command: the Setups page greys Apply out with it.
    pub fn check_lay(
        world: &World,
        setup: &str,
        picks: &[crate::operation::GeoSel],
        lay: crate::pick::PlaneLay,
    ) -> Result<(), CommandError> {
        let at = crate::pick::lay_on_plane_in(world, setup, picks, lay).map_err(CommandError::Invalid)?;
        change_of(world, setup, Some(at)).map(|_| ())
    }

    /// Lay the part on a G17 / G18 / G19 plane from a pick (#330): one
    /// flat face (an edge is refused by name, #554), laid from the pose
    /// the setup holds NOW (#738, [`crate::pick::lay_on_plane_in`]).
    /// Refused by name before anything is written.
    pub fn lay_on_plane(
        world: &World,
        setup: &str,
        picks: &[crate::operation::GeoSel],
        lay: crate::pick::PlaneLay,
    ) -> Result<Self, CommandError> {
        let at = crate::pick::lay_on_plane_in(world, setup, picks, lay).map_err(CommandError::Invalid)?;
        Self::new(world, setup, Some(at))
    }

    /// Would [`Self::lay_on_points`] be accepted? The same refusals, by
    /// name, without building the command (the Setups page's Apply).
    pub fn check_points(
        world: &World,
        setup: &str,
        picks: &[crate::operation::GeoSel],
        pl: crate::pick::PointsLay,
    ) -> Result<(), CommandError> {
        let at = crate::pick::lay_on_points_in(world, setup, picks, pl).map_err(CommandError::Invalid)?;
        change_of(world, setup, Some(at)).map(|_| ())
    }

    /// Lay the part on a G17 / G18 / G19 plane through three picked
    /// points (#555) from the pose the setup holds NOW (#738,
    /// [`crate::pick::lay_on_points_in`]); with `frame`, 3-2-1. Refused
    /// by name before any write.
    pub fn lay_on_points(
        world: &World,
        setup: &str,
        picks: &[crate::operation::GeoSel],
        pl: crate::pick::PointsLay,
    ) -> Result<Self, CommandError> {
        let at = crate::pick::lay_on_points_in(world, setup, picks, pl).map_err(CommandError::Invalid)?;
        Self::new(world, setup, Some(at))
    }

    /// Would [`Self::turn_in_plane`] be accepted? The same refusals, by
    /// name, without building the command (the Setups page's Apply).
    pub fn check_turn(world: &World, setup: &str, axes: &crate::pick::InPlaneAxes) -> Result<(), CommandError> {
        let at = crate::pick::turn_in_plane(world, setup, axes).map_err(CommandError::Invalid)?;
        change_of(world, setup, Some(at)).map(|_| ())
    }

    /// Turn the part in the work plane from picks (#557): a straight edge
    /// or two points for +Y and/or +X, read by
    /// [`crate::pick::turn_in_plane`] in the setup's current frame and
    /// composed onto its alignment. Refused by name before any write.
    pub fn turn_in_plane(world: &World, setup: &str, axes: &crate::pick::InPlaneAxes) -> Result<Self, CommandError> {
        let at = crate::pick::turn_in_plane(world, setup, axes).map_err(CommandError::Invalid)?;
        Self::new(world, setup, Some(at))
    }

    /// Would [`Self::align_from_axes`] be accepted? The same refusals,
    /// by name, without building the command (the Setups page's Apply).
    pub fn check_align(world: &World, setup: &str, axes: &crate::pick::InPlaneAxes) -> Result<(), CommandError> {
        let at = crate::pick::align_from_axes(world, setup, axes).map_err(CommandError::Invalid)?;
        change_of(world, setup, Some(at)).map(|_| ())
    }

    /// Align the part from its two picked axes (#769): with both +Y and
    /// +X picked their directions define the full 3D frame - the
    /// primary kept exactly, the other made square to it, +Z where the
    /// right-hand rule puts it - read by
    /// [`crate::pick::align_from_axes`] through the setup's current
    /// alignment. One picked axis keeps the in-plane turn (#557).
    /// Refused by name before any write.
    pub fn align_from_axes(world: &World, setup: &str, axes: &crate::pick::InPlaneAxes) -> Result<Self, CommandError> {
        let at = crate::pick::align_from_axes(world, setup, axes).map_err(CommandError::Invalid)?;
        Self::new(world, setup, Some(at))
    }

    /// Align the part from a picked cylinder (#771): one curved-wall
    /// pick names a bore's or cylinder's axis; it lands on `target`,
    /// turned `angle_deg` about it, read by
    /// [`crate::pick::align_from_cylinder`] through the setup's
    /// current alignment. Refused by name before any write.
    pub fn align_from_cylinder(
        world: &World,
        setup: &str,
        pick: &crate::operation::GeoSel,
        target: crate::pick::TargetAxis,
        angle_deg: f64,
        flip: bool,
    ) -> Result<Self, CommandError> {
        let at = crate::pick::align_from_cylinder(world, setup, pick, target, angle_deg, flip)
            .map_err(CommandError::Invalid)?;
        Self::new(world, setup, Some(at))
    }

    /// Would [`Self::align_from_cylinder`] be accepted? The same
    /// refusals, by name, without building the command (the Setups
    /// page's Apply).
    pub fn check_cylinder(
        world: &World,
        setup: &str,
        pick: &crate::operation::GeoSel,
        target: crate::pick::TargetAxis,
        angle_deg: f64,
        flip: bool,
    ) -> Result<(), CommandError> {
        let at = crate::pick::align_from_cylinder(world, setup, pick, target, angle_deg, flip)
            .map_err(CommandError::Invalid)?;
        change_of(world, setup, Some(at)).map(|_| ())
    }

    /// One of the Setups page's quick choices.
    pub fn preset(world: &World, setup: &str, preset: AlignPreset) -> Result<Self, CommandError> {
        Self::angles(world, setup, preset.angles())
    }

    /// Turn the parts about X, then Y, then Z (degrees) about the centre
    /// of their bounds (the presets' pivot). All zero is "as modelled".
    pub fn angles(world: &World, setup: &str, deg: [f64; 3]) -> Result<Self, CommandError> {
        Self::turn(world, setup, deg, TurnPivot::PartCentre)
    }

    /// Turn the parts to X, then Y, then Z (degrees, from as modelled)
    /// about `pivot` (#606). All zero is "as modelled".
    pub fn turn(world: &World, setup: &str, deg: [f64; 3], pivot: TurnPivot) -> Result<Self, CommandError> {
        let at = crate::setup_turn::turned(world, setup, rotation(deg)?, pivot).map_err(CommandError::Invalid)?;
        Self::new(world, setup, at)
    }

    /// Turn the parts BY X, then Y, then Z (degrees) about `pivot` from
    /// where they sit now (#738, [`crate::setup_turn::turned_by`]): the
    /// Setups page's preset and "Turn by" rows, composed onto every move
    /// before them. One undo step.
    pub fn turn_by(world: &World, setup: &str, deg: [f64; 3], pivot: TurnPivot) -> Result<Self, CommandError> {
        let at = crate::setup_turn::turned_by(world, setup, rotation(deg)?, pivot).map_err(CommandError::Invalid)?;
        Self::new(world, setup, at)
    }
}

fn rotation(deg: [f64; 3]) -> Result<[f64; 4], CommandError> {
    setup_align::rotation_from_angles(deg)
        .ok_or_else(|| CommandError::Invalid("the alignment angles must be finite numbers".into()))
}

impl Step {
    fn write(&self, world: &World, alignment: Option<Location>, stock: Option<&StockState>) {
        if let Some(entity) = model::find_by_name(world, &self.setup) {
            let view = entity.entity_view(world);
            if let Some(mut def) = view.try_get::<&SetupDef>(|d| d.clone()) {
                def.part_alignment = alignment;
                view.set(def);
            }
        }
        if let (Some((name, before, after)), Some((shape, loc, facets))) = (&self.stock, stock)
            && let Some(entity) = model::find_by_name(world, name)
        {
            model::set_transform(world, &entity, *loc);
            // Re-mesh only a block that changed size: a flip that keeps
            // the box moves it, and its geometry revision stays put.
            if before.0 != after.0 {
                model::set_facets(world, &entity, facets.clone());
                entity.entity_view(world).set(StockDef { shape: shape.clone() });
            }
        }
    }
}

impl Command for AlignPartToSetup {
    fn apply(&self, world: &World) {
        for step in std::iter::once(&self.main).chain(&self.carried) {
            step.write(world, step.next, step.stock.as_ref().map(|(_, _, after)| after));
        }
    }

    fn invert(&self, world: &World) {
        for step in self.carried.iter().rev().chain(std::iter::once(&self.main)) {
            step.write(world, step.prev, step.stock.as_ref().map(|(_, before, _)| before));
        }
    }
}
