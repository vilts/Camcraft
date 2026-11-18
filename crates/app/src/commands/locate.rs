//! `LocatePart` / `LocateFixture` (#561): put the part in a fixture, or
//! the fixture on the part, by mates (a part face or point against a
//! fixture face or point, up to the shop's 3-2-1) and a typed nudge -
//! one undoable command each. The solve is `fixture_locate::plan`; the
//! write is the move the part or fixture already knows, so there is one
//! transform model and one stock-follows law.
//!
//! Provenance: our own design over 3-2-1 fixturing practice (machine-tool
//! handbooks). No vendor implementation inspected.
//!
//! ## The laws these commands keep
//!
//! - **Validated at construction, refused by name.** Everything
//!   `fixture_locate::plan` refuses, plus "nothing would move". The
//!   interpenetration check runs here once and is carried as a warning -
//!   the part against the fixture, and the setup's stock against it.
//! - **A square stock stays square (#561 review).** The part write is
//!   `MoveModel::locating` / `AlignPartToSetup::locating`: a box or
//!   cylinder stock square to the setup keeps its orientation and its
//!   allowance on each side round the located part (`crate::stock_follow`;
//!   since #571 every `MoveModel` turn keeps the same rule) - the vise
//!   holds the blank square, it does not turn it with the part.
//! - **The part moves as part alignment moves it.** In a setup that holds
//!   the part as modelled the locate is a `MoveModel` (the document
//!   placement; the stock follows the part); in a setup that holds the
//!   part turned (#326) it composes into that setup's alignment through
//!   `AlignPartToSetup` (its stock and linked setups follow). The work
//!   offset is never touched.
//! - **The fixture moves as the Fixtures page moves it** (`MoveFixture`):
//!   the inverse transform, so the relative placement is the same
//!   whichever body moves.
//! - **Staleness is the stamp's.** A moved part changes what every op of
//!   that setup read, so they read Stale until regenerated, and undo
//!   puts the stamp's inputs back (Ok again). A moved fixture never
//!   stales a toolpath (the live-gate law, `crate::fixture`): the
//!   collision check re-judges at the new placement.
//! - **The stock can be the held body (#2173).** Its faces mate the
//!   fixture; the part moves and the stock follows it, so the part keeps
//!   its place in the blank. A follow that leaves a picked stock face off
//!   its mate plane (a cylinder re-fitted upright round a turned part) or
//!   a picked stock corner off its mate is refused by name.

use flecs_ecs::prelude::World;

use crate::commands::{AlignPartToSetup, Command, CommandError, MoveFixture, MoveModel};
use crate::fixture_locate::{self, LocateSpec};
use crate::model;
use crate::setup_align;

/// The write a locate of the part lands as.
enum PartWrite {
    Move(MoveModel),
    Align(Box<AlignPartToSetup>),
}

fn invalid(why: String) -> CommandError {
    CommandError::Invalid(why)
}

/// Solve, refusing the identity.
fn plan_moving(world: &World, spec: &LocateSpec) -> Result<fixture_locate::Plan, CommandError> {
    let plan = fixture_locate::plan(world, spec).map_err(invalid)?;
    if fixture_locate::is_still(plan.delta) {
        return Err(invalid(format!("'{}' already sits that way in '{}' - nothing would move", spec.part, spec.fixture)));
    }
    Ok(plan)
}

/// Move the part onto the fixture by mates.
pub struct LocatePart {
    write: PartWrite,
    plan: fixture_locate::Plan,
    warning: Option<String>,
}

impl LocatePart {
    pub fn new(world: &World, spec: &LocateSpec) -> Result<Self, CommandError> {
        let plan = plan_moving(world, spec)?;
        let setup = crate::setup::effective(world, plan.setup.as_deref());
        let aligned = setup.as_deref().and_then(|s| setup_align::stored(world, s).map(|a| (s, a)));
        let write = match aligned {
            Some((s, a)) => PartWrite::Align(Box::new(AlignPartToSetup::locating(world, s, Some(setup_align::compose(plan.delta, a)))?)),
            None => {
                let e = model::find_by_name(world, &spec.part).ok_or_else(|| CommandError::NotFound(spec.part.clone()))?;
                let loc = model::transform_of(world, &e).unwrap_or_default();
                PartWrite::Move(MoveModel::locating(world, &spec.part, setup_align::compose(plan.delta, loc))?)
            }
        };
        let stocks = match &write {
            PartWrite::Move(c) => c.moved_stock(world),
            PartWrite::Align(c) => c.moved_stock(),
        };
        // #2173: a stock locate lands only where its stock meets the mates.
        if spec.body == fixture_locate::LocateBody::Stock {
            fixture_locate::stock_lands(world, spec, &plan, &stocks).map_err(invalid)?;
        }
        let warning = fixture_locate::join_warnings(
            fixture_locate::interpenetration(world, spec, &plan),
            fixture_locate::stock_interpenetration(world, spec, &plan, &stocks),
        );
        Ok(LocatePart { write, plan, warning })
    }

    /// The transform (setup frame) the part takes.
    #[must_use]
    pub fn delta(&self) -> crate::schema::Location {
        self.plan.delta
    }

    /// The interpenetration warning, by name, if the placement leaves the
    /// part inside the fixture.
    #[must_use]
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }
}

impl Command for LocatePart {
    fn apply(&self, world: &World) {
        match &self.write {
            PartWrite::Move(c) => c.apply(world),
            PartWrite::Align(c) => c.apply(world),
        }
    }

    fn invert(&self, world: &World) {
        match &self.write {
            PartWrite::Move(c) => c.invert(world),
            PartWrite::Align(c) => c.invert(world),
        }
    }
}

/// Move the fixture onto the part by the same mates (the part stays).
pub struct LocateFixture {
    write: MoveFixture,
    plan: fixture_locate::Plan,
    warning: Option<String>,
}

impl LocateFixture {
    pub fn new(world: &World, spec: &LocateSpec) -> Result<Self, CommandError> {
        let plan = plan_moving(world, spec)?;
        let e = model::find_by_name(world, &spec.fixture).ok_or_else(|| CommandError::NotFound(spec.fixture.clone()))?;
        let loc = model::transform_of(world, &e).unwrap_or_default();
        let next = setup_align::compose(setup_align::inverse(plan.delta), loc);
        let write = MoveFixture::locating(world, &spec.fixture, next)?;
        let warning = fixture_locate::join_warnings(
            fixture_locate::interpenetration(world, spec, &plan),
            fixture_locate::stock_interpenetration(world, spec, &plan, &fixture_locate::stock_carried(world, &plan)),
        );
        // #826: the riders a located plate carries are judged where they land.
        let warning = crate::fixture_clash::join(warning.into_iter().chain(write.rider_warnings(world)).collect());
        Ok(LocateFixture { write, plan, warning })
    }

    /// The transform (setup frame) that carries the part onto the
    /// fixture; the fixture takes its inverse.
    #[must_use]
    pub fn delta(&self) -> crate::schema::Location {
        self.plan.delta
    }

    #[must_use]
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }
}

impl Command for LocateFixture {
    fn apply(&self, world: &World) {
        self.write.apply(world);
    }

    fn invert(&self, world: &World) {
        self.write.invert(world);
    }
}

/// Either locate, picked by which body moves - one command type for
/// callers (the UI, MCP) that offer both.
pub enum Locate {
    Part(Box<LocatePart>),
    Fixture(Box<LocateFixture>),
}

impl Locate {
    pub fn new(world: &World, spec: &LocateSpec, mover: fixture_locate::Mover) -> Result<Self, CommandError> {
        Ok(match mover {
            fixture_locate::Mover::Part => Locate::Part(Box::new(LocatePart::new(world, spec)?)),
            fixture_locate::Mover::Fixture => Locate::Fixture(Box::new(LocateFixture::new(world, spec)?)),
        })
    }

    #[must_use]
    pub fn warning(&self) -> Option<&str> {
        match self {
            Locate::Part(c) => c.warning(),
            Locate::Fixture(c) => c.warning(),
        }
    }
}

impl Command for Locate {
    fn apply(&self, world: &World) {
        match self {
            Locate::Part(c) => c.apply(world),
            Locate::Fixture(c) => c.apply(world),
        }
    }

    fn invert(&self, world: &World) {
        match self {
            Locate::Part(c) => c.invert(world),
            Locate::Fixture(c) => c.invert(world),
        }
    }
}
