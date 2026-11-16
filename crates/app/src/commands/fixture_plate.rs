//! Fixture plate commands (#819), each ONE undo unit:
//!
//! - [`InsertPlate`]: a built-in parametric plate, through the fixture
//!   library's insert path (`InsertFixture`, marked with its grid), so it
//!   is an ordinary meshed fixture to the collision laws and the file.
//! - [`SetPlate`]: change a built-in plate's figures; the mesh is
//!   regenerated through `UpdateModel` and every fixture standing on it
//!   is re-placed at its named location (refused by name if that
//!   location is gone, the fixture would hang off, or a mod vise jaw
//!   would reach into the stock; a re-placed fixture sitting in a part or
//!   the stock is a by-name warning, as for `MoveFixture`).
//! - [`PlaceOnPlate`]: put a fixture at a grid location or on a dowel
//!   pair of a plate, recording the place on its marker.
//! - [`SetPlateGrid`]: make an imported fixture a plate by detecting its
//!   hole grid (or stop it being one).
//!
//! Moving a plate (`MoveFixture`) carries the fixtures on it.
//!
//! Provenance: our own composition of the insert, update and marker
//! commands over `plate`'s geometry and placement rules.

use flecs_ecs::prelude::World;

use crate::commands::fixture::check_location;
use crate::commands::{Command, CommandError, InsertFixture, UpdateModel};
use crate::fixture::{self, MACHINE_TABLE};
use crate::fixture_library::FixtureEntry;
use crate::geometry::{IDENTITY_Q, Mesh, Point3};
use crate::model;
use crate::plate::mount::{self, MountAt, PlateMount, RiderMove};
use crate::plate::{PlateDef, PlateParams, detect};
use crate::schema::{FixtureDef, Location};
use crate::setup_align::compose;

/// Where a new plate goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlatePlacement {
    /// The plate frame (its origin corner on the top face) at this pose.
    At(Location),
    /// Centred on the setup machine's table, lying on its top.
    OnTable,
}

/// Place built-in plate `params` into setup context `setup`.
pub struct InsertPlate {
    inner: InsertFixture,
}

impl InsertPlate {
    /// Refused when the plate breaks the plate law (named), the setup is
    /// gone, the pose is not a finite rigid frame, or - on the table -
    /// the setup's machine states no table.
    pub fn new(world: &World, params: &PlateParams, setup: Option<&str>, placement: PlatePlacement) -> Result<Self, CommandError> {
        let def = params.def().map_err(CommandError::Invalid)?;
        let facets = params.mesh().map_err(CommandError::Invalid)?;
        let pick = match placement {
            PlatePlacement::At(loc) => loc,
            PlatePlacement::OnTable => {
                let t = fixture::table_box(world, setup).ok_or_else(|| {
                    CommandError::Invalid("the setup's machine states no table to lay the plate on".into())
                })?;
                let pos = Point3::new(
                    0.5 * (t.min.x + t.max.x) - 0.5 * params.length,
                    0.5 * (t.min.y + t.max.y) - 0.5 * params.width,
                    t.max.z + params.thickness,
                );
                Location { pos, rot: IDENTITY_Q }
            }
        };
        let entry = FixtureEntry {
            name: params.base_name(),
            note: None,
            datum: Location { pos: Point3::ZERO, rot: IDENTITY_Q },
            facets,
        };
        let inner = InsertFixture::new(world, &entry, setup, pick)?.with_plate(def);
        Ok(Self { inner })
    }

    /// The model name the plate lands as.
    #[must_use]
    pub fn name(&self) -> &str {
        self.inner.name()
    }

    /// The pose of the plate frame.
    #[must_use]
    pub fn location(&self) -> Location {
        self.inner.location()
    }

    /// The plate's grid.
    #[must_use]
    pub fn plate(&self) -> Option<&PlateDef> {
        self.inner.def().plate.plate.as_ref()
    }
}

impl Command for InsertPlate {
    fn apply(&self, world: &World) {
        self.inner.apply(world);
    }

    fn invert(&self, world: &World) {
        self.inner.invert(world);
    }
}

/// Fixture `name`'s marker, or why it is not a fixture.
fn def_named(world: &World, name: &str) -> Result<(FixtureDef, Location), CommandError> {
    if name == MACHINE_TABLE {
        return Err(CommandError::Invalid("the machine table is derived from the machine file - it is not a fixture you can place".into()));
    }
    let e = model::find_by_name(world, name).ok_or_else(|| CommandError::NotFound(name.to_owned()))?;
    let def = fixture::def_of(world, &e).ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a fixture")))?;
    Ok((def, model::transform_of(world, &e).unwrap_or_default()))
}

/// Plate `name`'s grid, or why it has none.
pub fn plate_of(world: &World, name: &str) -> Result<PlateDef, CommandError> {
    def_named(world, name)?
        .0
        .plate
        .plate
        .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a fixture plate (add one, or detect an imported plate's grid)")))
}

/// Fixture `name`'s own mesh.
fn facets(world: &World, name: &str) -> Vec<crate::geometry::Triangle> {
    model::find_by_name(world, name).and_then(|e| model::facets_of(world, &e)).map(|f| f.to_vec()).unwrap_or_default()
}

/// Fixture `rider` placed at `m` on plate `plate` (its grid `def`, its
/// mesh `plate_mesh`, its pose `at`): the rider's pose, or why not.
pub(crate) fn seat(def: &PlateDef, plate_mesh: &[crate::geometry::Triangle], at: Location, rider: &str, rider_mesh: &[crate::geometry::Triangle], m: &PlateMount) -> Result<Location, CommandError> {
    let local = def.local_pose(&m.at, m.yaw_deg).map_err(|e| CommandError::Invalid(format!("plate '{}': {e}", m.plate)))?;
    let local = mount::stand(local, rider_mesh);
    mount::on_the_plate(plate_mesh, rider_mesh, local)
        .map_err(|e| CommandError::Invalid(format!("'{rider}' at {} on '{}': {e}", m.at.label(), m.plate)))?;
    check_location(compose(at, local), &format!("'{rider}'"))
}

/// A placed accessory `rider` resized to `params` (its new mesh
/// `rider_mesh`) while it stands at `m`: refused by name when a subplate's
/// pins no longer fit the dowel pair or the new footprint hangs off the
/// plate (#821). Its pose is unchanged: every accessory frame has its
/// underside at z = 0.
pub(crate) fn still_seated(world: &World, rider: &str, m: &PlateMount, params: &crate::accessory::Accessory, rider_mesh: &[crate::geometry::Triangle]) -> Result<(), CommandError> {
    let def = plate_of(world, &m.plate)?;
    crate::rotary_stack::pins_fit(&def, m, rider, Some(params)).map_err(CommandError::Invalid)?;
    let (_, at) = def_named(world, &m.plate)?;
    seat(&def, &facets(world, &m.plate), at, rider, rider_mesh, m).map(|_| ())
}

/// A marker swap plus the poses it moves - the body of every plate
/// command but the insert.
struct Swap {
    name: String,
    prev: FixtureDef,
    next: FixtureDef,
    moves: Vec<RiderMove>,
}

impl Swap {
    fn write(&self, world: &World, forward: bool) {
        if let Some(e) = model::find_by_name(world, &self.name) {
            e.entity_view(world).set(if forward { self.next.clone() } else { self.prev.clone() });
        }
        mount::write(world, &self.moves, forward);
    }
}

/// The fixtures standing on plate `plate` re-seated on its new grid
/// `def` (and mesh): their moves, plus their own riders' moves.
fn reseat(world: &World, plate: &str, def: &PlateDef, mesh: &[crate::geometry::Triangle], at: Location) -> Result<Vec<RiderMove>, CommandError> {
    let mut moves = Vec::new();
    for rider in mount::riders(world, plate) {
        let (rdef, before) = def_named(world, &rider)?;
        let Some(m) = rdef.plate.mount else { continue };
        crate::rotary_stack::pins_fit(def, &m, &rider, rdef.accessory.as_ref()).map_err(CommandError::Invalid)?;
        let after = seat(def, mesh, at, &rider, &facets(world, &rider), &m)?;
        moves.extend(mount::carry(world, &rider, mount::delta(before, after), &[plate]));
        moves.push(RiderMove { name: rider, before, after });
    }
    Ok(moves)
}

/// Change a built-in plate's figures.
pub struct SetPlate {
    update: UpdateModel,
    swap: Swap,
    warning: Option<String>,
}

impl SetPlate {
    /// Refused when `name` is not a built-in plate, `params` break the
    /// plate law, a fixture on it loses its location or would hang off,
    /// or a mod vise jaw block on it would reach into the stock.
    pub fn new(world: &World, name: &str, params: PlateParams) -> Result<Self, CommandError> {
        let (prev, at) = def_named(world, name)?;
        if prev.plate.plate.as_ref().and_then(|p| p.params.as_ref()).is_none() {
            return Err(CommandError::Invalid(format!("'{name}' is not a built-in plate (an imported plate has a fixed shape)")));
        }
        let def = params.def().map_err(CommandError::Invalid)?;
        let mesh = params.mesh().map_err(CommandError::Invalid)?;
        let moves = reseat(world, name, &def, &mesh, at)?;
        // #820 review: the stock stays put while the riders re-seat - a
        // mod vise jaw pushed into it is refused, and every rider (and the
        // new slab) is judged where it lands, as `MoveFixture` judges.
        let mut warnings = crate::commands::mod_vise::jaws_after_reseat(world, &moves)?;
        warnings.extend(crate::fixture_clash::after_fixture_shape(world, name, &mesh, at));
        warnings.extend(moves.iter().filter_map(|m| crate::fixture_clash::after_fixture_move(world, &m.name, m.after)));
        let update = UpdateModel::new(world, name, Mesh::from_facets(mesh))?.keeping_plate();
        let mut next = prev.clone();
        next.plate.plate = Some(def);
        Ok(Self { update, swap: Swap { name: name.to_owned(), prev, next, moves }, warning: crate::fixture_clash::join(warnings) })
    }

    /// The by-name warnings of the edit (a rider or the new slab sitting in
    /// a part or the stock, a mod vise jaw lifted off the stock) - never a
    /// refusal; `None` when clear.
    #[must_use]
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    /// The parameters the edit lands.
    #[must_use]
    pub fn params(&self) -> Option<&PlateParams> {
        self.swap.next.plate.plate.as_ref().and_then(|p| p.params.as_ref())
    }

    /// The fixtures it re-places, by name.
    pub fn moved(&self) -> impl Iterator<Item = &str> {
        self.swap.moves.iter().map(|m| m.name.as_str())
    }
}

impl Command for SetPlate {
    fn apply(&self, world: &World) {
        self.update.apply(world);
        self.swap.write(world, true);
    }

    fn invert(&self, world: &World) {
        self.update.invert(world);
        self.swap.write(world, false);
    }
}

/// Put a fixture at a grid location or on a dowel pair of a plate.
pub struct PlaceOnPlate {
    swap: Swap,
    location: Location,
}

impl PlaceOnPlate {
    /// Refused by name: an unknown fixture or plate, a fixture placed on
    /// itself (or on a plate that rides on it), an unknown location, a
    /// fixture that would hang off the plate, a non-finite turn.
    pub fn new(world: &World, name: &str, plate: &str, at: MountAt, yaw_deg: f64) -> Result<Self, CommandError> {
        let (prev, before) = def_named(world, name)?;
        let def = plate_of(world, plate)?;
        if name == plate {
            return Err(CommandError::Invalid(format!("'{name}' cannot stand on itself")));
        }
        // The plate must not ride (transitively) on the fixture.
        let mut up = Some(plate.to_owned());
        let mut hops = 0usize;
        while let Some(p) = up {
            if p == name {
                return Err(CommandError::Invalid(format!("'{plate}' stands on '{name}' - it cannot carry it")));
            }
            hops += 1;
            if hops > 1024 {
                break;
            }
            up = def_named(world, &p).ok().and_then(|(d, _)| d.plate.mount).map(|m| m.plate);
        }
        let (_, plate_at) = def_named(world, plate)?;
        let m = PlateMount { plate: plate.to_owned(), at, yaw_deg };
        // #821: a rotary subplate goes on the dowel pair its pins fit.
        crate::rotary_stack::pins_fit(&def, &m, name, prev.accessory.as_ref()).map_err(CommandError::Invalid)?;
        let after = seat(&def, &facets(world, plate), plate_at, name, &facets(world, name), &m)?;
        let mut moves = mount::carry(world, name, mount::delta(before, after), &[]);
        moves.push(RiderMove { name: name.to_owned(), before, after });
        let mut next = prev.clone();
        next.plate.mount = Some(m);
        Ok(Self { swap: Swap { name: name.to_owned(), prev, next, moves }, location: after })
    }

    /// Where the fixture lands (world).
    #[must_use]
    pub fn location(&self) -> Location {
        self.location
    }
}

impl Command for PlaceOnPlate {
    fn apply(&self, world: &World) {
        self.swap.write(world, true);
    }

    fn invert(&self, world: &World) {
        self.swap.write(world, false);
    }
}

/// Make a fixture a plate from its detected hole grid, or stop it being one.
pub struct SetPlateGrid {
    swap: Swap,
}

impl SetPlateGrid {
    /// Detect fixture `name`'s grid from its holes (its own model frame).
    /// Refused when it is a built-in plate, or its holes form no grid.
    pub fn detect(world: &World, name: &str) -> Result<Self, CommandError> {
        let (prev, _) = def_named(world, name)?;
        if prev.plate.plate.as_ref().is_some_and(|p| p.params.is_some()) {
            return Err(CommandError::Invalid(format!("'{name}' is a built-in plate - its grid comes from its figures")));
        }
        let def = detect::grid_from_facets(&facets(world, name)).map_err(|e| CommandError::Invalid(format!("'{name}': {e}")))?;
        let mut next = prev.clone();
        next.plate.plate = Some(def);
        Ok(Self { swap: Swap { name: name.to_owned(), prev, next, moves: Vec::new() } })
    }

    /// Stop fixture `name` being a plate. Refused while fixtures stand on it.
    pub fn clear(world: &World, name: &str) -> Result<Self, CommandError> {
        let (prev, _) = def_named(world, name)?;
        if prev.plate.plate.is_none() {
            return Err(CommandError::Invalid(format!("'{name}' is not a fixture plate")));
        }
        let on = mount::riders(world, name);
        if !on.is_empty() {
            return Err(CommandError::Invalid(format!("'{name}' still carries {} - place them elsewhere first", on.join(", "))));
        }
        let mut next = prev.clone();
        next.plate.plate = None;
        Ok(Self { swap: Swap { name: name.to_owned(), prev, next, moves: Vec::new() } })
    }

    /// The grid it lands (none when clearing).
    #[must_use]
    pub fn plate(&self) -> Option<&PlateDef> {
        self.swap.next.plate.plate.as_ref()
    }
}

impl Command for SetPlateGrid {
    fn apply(&self, world: &World) {
        self.swap.write(world, true);
    }

    fn invert(&self, world: &World) {
        self.swap.write(world, false);
    }
}
