//! All Flecs component declarations live here (AGENTS.md layering rule).
//!
//! Provenance: our own domain model; no HSMWorks internals involved.
//!
//! Components are native Rust types owned by the ECS: `Vec`/`String` fields
//! are freed by flecs_ecs's registered drop hooks when an entity is deleted
//! or a component removed. Per-type arenas are a later (M4.3-era) perf
//! optimization, not current policy.

use flecs_ecs::prelude::*;

use crate::cl::Toolpath as ClPath;
use crate::geometry::{IDENTITY_Q, Point3, Triangle};
use crate::operation::types::{AfterOperation, AreaControls, CoolantMode, GenStamp, OpParams, SrcRef};
use crate::stock::StockShape;
use crate::tools::Tool;
use crate::verify::Warning;

/// Human-unique name of a persistent entity. The project serializer and
/// name-based command lookup both key on this.
#[derive(Component, Debug)]
pub struct Name(pub String);

/// Rigid placement of a model entity: world position plus orientation as a
/// unit quaternion (x, y, z, w). The UI exposes yaw only for now; the data
/// model carries full orientation so stock/fixture alignment later is an
/// editor change, not a schema change. Write it through `model::set_transform`,
/// which guarantees the quaternion is unit.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Location {
    pub pos: Point3,
    pub rot: [f64; 4],
}

impl Default for Location {
    /// Manual: a derived `Default` would store the zero quaternion, which is
    /// not a rotation.
    fn default() -> Self {
        Location {
            pos: Point3::ZERO,
            rot: IDENTITY_Q,
        }
    }
}

/// Triangulated geometry of a model. Rust-owned storage: freed on delete.
#[derive(Component, Debug)]
pub struct Facets {
    pub tris: std::sync::Arc<[Triangle]>,
}

/// How many times geometry has landed on this model. M3.1's shape was
/// `1` at import, `+1` per `UpdateModel`; M5.3 A moved the increment
/// into `model::set_facets`, so EVERY landing moves it - a write can
/// never leave a fresh-looking memo entry behind, because the memo key
/// moves with the write. Restore commands (`UpdateModel::invert`, the
/// project loader) pin the counter back to a past value while landing
/// the exact bytes that value carried - the pairing the memo trusts.
/// Staleness is still derived from the stamp against the live facets,
/// never from this counter (AGENTS.md: no stored stale flags); the
/// counter only says when a derived digest may be REUSED, not what is
/// true. Entities without the component read as revision 0, which is
/// exactly what pre-v20 project files meant.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Revision(pub u64);

/// The newest derived geometry signature of a model, paired with the
/// revision it describes (M5.3 A). Valid exactly while `rev` is the live
/// revision counter (which `set_facets` moves with every landing) and
/// gated on the facets still being mounted. Derived state, never a
/// stored stale flag: correctness is independent of hits (the
/// FEATURES_CACHE law), and as a component it dies with the entity it
/// describes - no cache outlives its subject. Not serialized: a fresh
/// world re-derives on first read.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeoSig {
    pub rev: u64,
    pub sig: u64,
}

/// A model whose mesh stands for its curved surfaces more coarsely than
/// the STEP importer promises (#864): `chord_mm` is the chord error the
/// import actually kept, always above `step::CHORD_CAP`. Present ONLY on
/// such a model - one that met the promise, an STL or OBJ mesh, and
/// every model of a project saved before this existed carry no component
/// and read as full fidelity. Written by the import and update commands
/// and the project loader, through `model_fidelity::set`.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ReducedFidelity {
    pub chord_mm: f64,
}

/// The STEP facet budget a model's mesh was imported under, when it is not
/// the default (#1054; `step_budget::DEFAULT`). Absent on every model
/// imported at the default, on STL/OBJ meshes and on every model of a
/// project saved before it existed. Written by the import and update
/// commands and the project loader, through `model_fidelity::set_budget`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepBudget {
    pub facets: u64,
}

/// Selection marker (tag). Only `SelectEntity` writes it (AGENTS.md: changes
/// go through commands); the tree and viewport read it to highlight.
#[derive(Component, Debug)]
pub struct Selected;

/// The setup's raw material, stored as parameters (`stock` module). Only
/// `SetStock` writes it, and it always sits on the entity named `STOCK_NAME`,
/// whose `Facets` mirror the meshed shape (render and pick need no special
/// stock paths, only the kind test).
#[derive(Component, Debug)]
pub struct StockDef {
    pub shape: StockShape,
}

/// #1094: the user said the from-model stock's source model is raw stock
/// only - a casting or blank, not the finished part - so the program
/// verdict does not judge it. Rides the stock entity; `source` is the
/// model it was said of, honoured only while the stock is still a copy of
/// that model AND while the model's geometry still carries `print`, the
/// fingerprint taken when the mark was set (third review: the mark is
/// self-validating where it takes effect). Absent (the default, every
/// older file) judges it. Only `SetStockSourceOnly` and the project
/// loader write it.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct StockSourceOnly {
    pub source: String,
    pub print: GeomPrint,
}

/// A model's geometry as the program verdict reads it (#1094,
/// `stock_print`): its facet count and two independent 64-bit hashes over
/// every vertex coordinate's bits (local frame) and its placement's bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeomPrint {
    pub facets: u64,
    pub hash: [u64; 2],
}

/// The memo of a model's facet hashes (`stock_print`), valid while `rev`,
/// `ptr` and `len` still describe the mounted facets - a 200k-facet model
/// is hashed once per revision, not once per verdict. Derived, never
/// serialized; dies with its entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeomPrintMemo {
    pub rev: u64,
    pub ptr: usize,
    pub len: usize,
    pub hash: [u64; 2],
}

/// Unit tag marking an entity as a work coordinate system origin: a
/// setup's one WCS (#342), named by `SetupDef::wcs` (`G54` for a document
/// without setups), its `Location` the transform; there is no payload to
/// drift. The NAME is not the G-word - that is the setup's label
/// (`SetupDef::work_offset`). Never has facets.
#[derive(Component, Debug)]
pub struct Wcs;

/// A cutting tool definition, on the entity named by the catalog or the
/// user. Tools never have facets; operations and the verifier read their
/// numbers through `model::tool_of`.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct ToolDef {
    pub tool: Tool,
    /// The controller's number for this tool (#221): what the post writes
    /// as `T..` / `H..`, matching the machine's tool table. `None` = not
    /// stated, and the tool posts as its 1-based position in the document's
    /// tool list (the numbering every earlier document had). Changed only
    /// through `SetToolNumber`; `ChangeTool` leaves it as it is.
    pub number: Option<u32>,
}

/// A tool's modelled body (#368): a holder or assembly mesh (a vendor
/// STEP, an STL) placed in the tool's frame, and what the judging laws
/// read off it, derived from those facets at landing and never stored
/// apart from them. A milling tool's frame is the spindle's (tip at the
/// origin, axis +Z toward the spindle; its revolved envelope); a lathe
/// steel's is the turning tool frame (#369: tip at the origin, +x away
/// from the spindle axis, +z along it; its clearance bands) - see
/// [`crate::tool_body::BodyFrame`]. A tool without one judges its
/// `holder_d`/`holder_len` cylinder, as always. Written only through
/// `SetToolBody` (`crate::tool_body`).
#[derive(Component, Debug, Clone, PartialEq)]
pub struct ToolBody {
    /// Where the mesh came from (a file name), for display only.
    pub source: String,
    /// The placed facets, in the frame `frame` names.
    pub facets: std::sync::Arc<[Triangle]>,
    /// The drawn outline when the body is a spun profile (#93): the
    /// facets are its revolve and the spindle frame's `profile` its exact
    /// bands. `None` for an imported mesh (and every lathe-frame body).
    pub spun: Option<camcraft_tools::spun::SpunProfile>,
    /// The frame, the placement it landed with, and the derived read.
    pub frame: crate::tool_body::BodyFrame,
    /// #828: the holder is ASSUMED - a machine's `tooling.default_holder`
    /// landed on a new tool, not a holder the user picked. Judged no
    /// smaller than the tool's stand-in holder cylinder anywhere
    /// (`HolderProfile::floored`, in `tool_body::judged`). `false` for
    /// every body the user lands, and for every older file's.
    pub machine_default: bool,
}

/// #93 slice 2: a milling tool's own drawn cutter, neck and shank
/// outline (`SetToolCutter`, `crate::tool_cutter`). It agrees with the
/// tool's numbers by law (`cutter_profile::check`); no generator reads
/// it, and the stack laws and simulation read only its neck where it is
/// narrower than the stated shank (#852, `tool_cutter::neck_of`).
/// Absent = drawn from the numbers.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct ToolCutter {
    pub spun: camcraft_tools::spun::SpunProfile,
}

impl ToolBody {
    /// The revolved envelope of a milling (spindle-frame) body.
    #[must_use]
    pub fn profile(&self) -> Option<&camcraft_tools::holder_profile::HolderProfile> {
        match &self.frame {
            crate::tool_body::BodyFrame::Spindle { profile, .. } => Some(profile),
            crate::tool_body::BodyFrame::Lathe { .. } => None,
        }
    }
}

/// One machine setup (M3.2 stage A): the named context that says WHICH
/// raw material and WHICH work coordinate system an operation is
/// machined against. The strings are entity NAMES, resolved through
/// [`crate::setup`] - the first setup of a pre-M3.2 document points at
/// the legacy global names (`STOCK_NAME`, `G54`) so nothing is renamed
/// when setups arrive.
/// `wcs` names the setup's ONE work coordinate system (#342): one origin
/// per setup, never a list of slots. An empty name, or one naming an
/// entity that does not exist yet, means "this setup's WCS is not placed"
/// (it sits at the origin). `datum_from` records the setup whose stock
/// and WCS a new setup was cloned from - provenance for the sheet, inert
/// for every stamp (the clones themselves carry the data).
/// `work_offset` is only the setup's post LABEL (#196, #342), an index
/// into [`WCS_SLOTS`]: the `G54`..`G59` word the post writes for this
/// setup's WCS. It selects no stored origin - changing it moves nothing.
/// `None` - the default - writes `G54` ([`crate::setup::wcs_slot`]).
/// Written only by `SetSetupWorkOffset` and the loader's migrations.
/// `part_alignment` (#326) is how the PART sits in this setup: the rigid
/// transform from the document frame into the setup frame (which face
/// is on the table, which edge runs along X). The stock, the WCS and
/// the fixtures of the setup are placed in that frame already; only the
/// parts read through it (`setup_align`). `None` - the default - is "as
/// modelled", exactly the pre-#326 behaviour. Never `Some(identity)`.
/// Written only by `AlignPartToSetup`, `CreateSetup` (a datum transfer
/// carries it) and the project loader.
/// `machine` and `post` (#343) are the setup's OWN machine and post:
/// `None` - the default, and every file before v55 - follows the
/// document's (`MachineDef` / `PostRef` singletons), so an untouched
/// document mounts and posts exactly as before. Read through
/// [`crate::setup_mount`]; written only by `SetSetupMachine` /
/// `SetSetupPost` and the project loader.
/// `relative_to` (#316) names the setup this one is placed RELATIVE to
/// (a re-clamp of it: flipped, turned). The relation itself is not
/// stored - it is `part_alignment` read against the other setup's
/// ([`crate::setup_relative::transform_between`]); the link only makes
/// a later change of that setup's alignment carry into this one. `None`,
/// the default and every file before v56, links nothing. Written only by
/// `SetSetupRelative` and the project loader.
/// `stock_source` (#320) is where the setup's stock comes from: its own,
/// or what the setup before it leaves. `None`, the default and every
/// file before v57, reads as #317's implicit rule
/// ([`crate::setup_stock::implicit`]). Written only by
/// `SetSetupStockSource` and the project loader.
/// `post_options` (#389) is the setup's own post options; `None`, the
/// default and every earlier file, follows the document's `PostOptions`.
/// Written only by `SetSetupPostOptions` and the project loader.
/// `workholding_max_rpm` (#1031) is the rated maximum rpm of the chuck
/// or collet the setup holds the work in; `None`, the default and every
/// earlier file, states none. Read through [`crate::setup_workholding`];
/// written only by `SetSetupWorkholdingRpm` and the project loader.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct SetupDef {
    pub stock: String,
    pub wcs: String,
    pub datum_from: String,
    pub work_offset: Option<usize>,
    pub part_alignment: Option<Location>,
    pub machine: Option<MachineDef>,
    pub post: Option<PostRef>,
    pub relative_to: Option<String>,
    pub stock_source: Option<crate::setup_stock::StockSource>,
    pub post_options: Option<PostOptions>,
    pub workholding_max_rpm: Option<f64>,
}

/// The M3.2 stage B marker turning a model entity into a FIXTURE: the
/// solid clamps, sits on, or stands beside the stock, and collision-
/// checks every path in its setup context (M3.2 stage B). There is no
/// synthetic fixture geometry in this program - a fixture is an
/// ordinary named model (its facets are whatever STEP the user loaded),
/// wearing this tag; `setup` follows the declared-vs-effective law of
/// `Operation::setup`. Moving a fixture IS moving a model (the same
/// `Location`), so only the binding rides here. The machine's table is
/// NOT one of these - it is derived from the `.mch` on demand
/// (`fixture::boxes_for`), which is why this component names no kind.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct FixtureDef {
    pub setup: Option<String>,
    /// The workholding stands still while a rotary turns the part (a
    /// table, a tailstock) and is judged in the machine frame; false -
    /// the default, every pre-M13.8 file - turns with it (a chuck) and
    /// is judged in the path's own frame.
    pub fixed: bool,
    /// A built-in vise's parameters (#788): its facets were generated
    /// from these, so the jaw opening can be edited and regenerated.
    /// `None` - every imported fixture - is plain steel.
    pub vise: Option<crate::vise::ViseParams>,
    /// Fixture plates (#819): this fixture IS a plate (its hole grid),
    /// and/or stands ON one at a named location. Default: neither.
    pub plate: crate::plate::PlateLink,
    /// A mod-vise jaw block's parameters (#820): its facets were
    /// generated from these. `None` - every other fixture.
    pub mod_jaw: Option<crate::modvise::JawParams>,
    /// A built-in accessory's parameters (#789: parallels, blocks, a
    /// strap clamp, a soft jaw); `None` - not one.
    pub accessory: Option<crate::accessory::Accessory>,
}

/// The document's active setup, stored as a flecs singleton like `Units`:
/// the context NEW furniture and NEW operations join, never a browser
/// row and never a saved entity. `None` means no explicit choice, which
/// reads as the first setup by name (the implicit-setup law, M3.2 A) -
/// so a document that never touches setups behaves exactly as it did.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct ActiveSetup(pub Option<String>);

/// A machining operation's definition: which tool, which geometry source
/// (by name), the parameters, and the stamp of the world state the last
/// successful generation saw (`None` until `GenerateOperation` runs).
/// Status is derived from `stamp` vs the live world - nothing here stores
/// "stale". (`gen` would be the obvious name, but it is a reserved keyword
/// in this Rust edition.)
#[derive(Component, Debug, Clone, PartialEq)]
pub struct Operation {
    pub tool: String,
    pub src: SrcRef,
    pub params: OpParams,
    pub stamp: Option<GenStamp>,
    /// The setup this operation machines against (M3.2 stage A). `None`
    /// is not "no setup" - it means "the first setup by name", which in
    /// a document with no setups at all is exactly the old global
    /// behavior (the implicit-setup law).
    pub setup: Option<String>,
    /// The area controls (M11.5): how far past - or short of - the
    /// selection the tool may go. Default is the identity.
    pub area: AreaControls,
    /// The cutting fluid the post switches on for this operation
    /// (#112). Flood by default - what every post emitted before.
    pub coolant: CoolantMode,
    /// What the machine does once this operation is done (#110):
    /// carry on by default - what every post emitted before.
    pub after: AfterOperation,
    /// Left out of the posted program (#110), toolpath and all kept:
    /// the post (and the program sim that judges it) skip it; it still
    /// generates, and un-suppressing restores it as it was.
    pub suppressed: bool,
}

/// User-facing operation label and explicit position within its setup.
/// The entity's `Name` remains a stable key for commands and undo history.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct OperationMeta {
    pub label: Option<String>,
    pub order: Option<u32>,
}

/// A standalone Pause or Home entry in the operation list (#155): an
/// after-action with no tool and no toolpath, placed between operations
/// by the same `OperationMeta::order` its siblings use. It is not an
/// `Operation`, so every program walk that needs a tool or a toolpath
/// (`setup::operation_order` and all who walk it) never sees it; the
/// post folds it into the gap it stands in (`program_entry`).
#[derive(Component, Debug, Clone, PartialEq)]
pub struct ProgramEntry {
    /// What the machine does here - never `AfterOperation::None`.
    pub action: AfterOperation,
    /// The setup it stands in, the `Operation::setup` law.
    pub setup: Option<String>,
    /// Left out of the posted program, like a suppressed operation.
    pub suppressed: bool,
}

/// The cached result of generating an operation: the path plus the
/// warnings `verify` attached to it. Deleting the operation entity frees
/// both through the Rust-owned drop hooks (AGENTS.md resource invariant).
/// `sig` is the path's [`crate::path_sig::of`], computed once in
/// [`Toolpath::new`], the only way to build one (the field is private),
/// so the program-ahead digest reads it per frame without a clone
/// (#109 review B1). The cache is written whole, never edited in place.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct Toolpath {
    pub path: ClPath,
    pub warnings: Vec<Warning>,
    sig: u64,
}

impl Toolpath {
    #[must_use]
    pub fn new(path: ClPath, warnings: Vec<Warning>) -> Self {
        let sig = crate::path_sig::of(&path.moves);
        Self { path, warnings, sig }
    }

    /// The stored signature of `path` (see the type's doc).
    #[must_use]
    pub fn path_sig(&self) -> u64 {
        self.sig
    }
}

impl Default for Toolpath {
    fn default() -> Self {
        Self::new(ClPath::default(), Vec::new())
    }
}

/// The document's display unit, stored as a flecs singleton (M1.1): on
/// the world, not on any entity - so it can never leak into a browser
/// row, a name lookup, or the saved entity list. Millimeters until a
/// `SetUnits` command or a v8 project file says otherwise; every stored
/// number stays millimeters whatever this says (see `units` module law).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Units(pub crate::geometry::units::Unit);

/// The machine this document posts to (M2.3 stage C), stored as a flecs
/// singleton like `Units`: setup furniture, never a browser row, a name
/// lookup, or a saved entity. `machine: None` means no definition was
/// loaded and the limit checker checks nothing; `path` names the `.mch`
/// it was read from (empty when not read from disk) for display only.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct MachineDef {
    pub path: String,
    pub machine: Option<crate::machine::def::Machine>,
}

/// Which post script the document runs (M3.3 A/B), a flecs singleton
/// like `MachineDef`: setup furniture, never a browser row.
/// `Builtin` is the app's own shipped default - the first registry
/// entry, and the meaning of a project file with no post choice at
/// all; `Named` addresses a shipped registry entry by name (M3.3 B:
/// names are stable across registry growth, indices are not, so the
/// document stores the name); `File` names the user's `.cps` by path -
/// all validated when selected (the `SelectPost` constructor compiles
/// files, checks names against the registry), re-read at post time and
/// refused by name if they went missing, never silently swapped back
/// to the default.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub enum PostRef {
    #[default]
    Builtin,
    Named(String),
    File(String),
}

/// The user's choices for the document's post (issue #34), a flecs
/// singleton beside `PostRef`: setup furniture, moved only by
/// `SetPostOptions` (undoable) or a document load. A script reads the
/// ones it knows from its `postOptions` global and ignores the rest;
/// the default is every post's own default, so a document that never
/// chose writes no key. `skip_stops` (#110) posts the program without
/// any operation's after-action (no M0 / M1, no trips home) - a
/// debugged program runs straight through, the ops keep their choices.
/// `per_op` (#111) is the Output choice: off (the default) posts one
/// combined program, on writes one standalone program per operation.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PostOptions {
    /// How a lathe post writes a thread (Fanuc Lathe: G32 passes, the
    /// default, or the two-block G76 cycle from the thread record).
    pub threading: ThreadingOutput,
    /// Post without any operation's stop or trip home (#110).
    pub skip_stops: bool,
    /// One standalone program per operation (#111).
    pub per_op: bool,
    /// Per-operation smoothing (#616): off (the default) posts no
    /// smoothing words; on, each op asks its tolerance and the dialect
    /// writes its control's mode (G64 P, G5.1, G187, CYCLE832, CYCL
    /// DEF 32), reset at the op's end. Opt-in because several controls
    /// need an option for it (Fanuc's AI contour control).
    pub smoothing: bool,
    /// The length unit the control is left in after the program (#596).
    pub end_units: EndUnits,
}

/// The end-units choice of [`PostOptions`] (#596). LinuxCNC's M2 / M30
/// reset list (linuxcnc.org/docs, M-codes, "M2, M30 Program End") names
/// offsets, plane, distance and feed modes, overrides, comp, spindle,
/// motion mode and coolant - never G20 / G21 - so a program posted in mm
/// for a control the operator runs in inches leaves it in mm. A post
/// that knows the option (Tormach PathPilot) writes the target's unit
/// word before its program end when it differs from the program's unit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EndUnits {
    /// The program's own unit stays modal (every older document).
    #[default]
    Leave,
    /// End in inches: a mm program writes G20 before M30.
    Inch,
    /// End in millimetres: an inch program writes G21 before M30.
    Millimeters,
}

/// The thread-output choice of [`PostOptions`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThreadingOutput {
    /// Every pass as synchronized G32 moves - the stream's own passes.
    #[default]
    G32Passes,
    /// One G76 multi-pass threading cycle per thread record.
    G76Cycle,
}

/// Which theme the user wears (M6.1 C), a flecs singleton. Since #877 a
/// user preference, not a document setting: `app::prefs` keeps it per
/// user, and no project file carries it. `Dark` is the shipped default;
/// `Named` addresses a built-in theme BY NAME (names survive growth,
/// indices don't); `File` names the user's `.json` theme by path. A pick
/// is validated before it is set (`commands::check_theme`, plus the ui
/// seam's JSON parse); a stored `File` reads as written and refuses by
/// name at apply time, never silently swapping back to the default.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub enum ThemeRef {
    #[default]
    Dark,
    Named(String),
    File(String),
}

/// The colour-vision remap the viewer wears (M13.4), a flecs singleton
/// kept per user with the theme (`app::prefs`, #877). `Off` is the
/// default and changes nothing on screen. Like `ThemeRef` it is a VIEW
/// preference, not a document setting: a deficiency belongs
/// to the person at the screen, not to the part - a protan's document
/// opened by a colleague must not wear the protan palette - so the
/// project file never carries it and opening a document leaves it as
/// it was (the world outlives the documents it shows).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorVision {
    #[default]
    Off,
    /// Protanopia: the long-wavelength (L) cones absent.
    Protan,
    /// Deuteranopia: the medium-wavelength (M) cones absent.
    Deutan,
    /// Tritanopia: the short-wavelength (S) cones absent.
    Tritan,
}

impl ColorVision {
    /// Every mode, Off first - the picker's row order.
    pub const ALL: [ColorVision; 4] =
        [ColorVision::Off, ColorVision::Protan, ColorVision::Deutan, ColorVision::Tritan];

    /// The picker's label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            ColorVision::Off => "Off",
            ColorVision::Protan => "Protanopia",
            ColorVision::Deutan => "Deuteranopia",
            ColorVision::Tritan => "Tritanopia",
        }
    }
}

/// The document's workpiece material (M13.2), a flecs singleton set
/// by `SetWorkMaterial` and saved with the project. `None` (and an
/// absent singleton) states no material: nothing is suggested for it
/// and no power estimate is claimed against it.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkMaterialDef(pub Option<crate::tools::WorkMaterial>);

/// Ribbon pins (M6.2): the kind indices the user pinned, in pin order -
/// the ribbon's own Pinned section reads this singleton. A document
/// SETTING like the theme reference (saved with the project), so like
/// the theme it changes only through a command - `SetRibbonPins`
/// (issue #143), undoable and counted as a document change. The
/// indices are registry-stable - the row order is the product order -
/// so a pin names its kind the way the theme names its built-in;
/// an index past the table (a document from a larger registry) is
/// dropped at read, never shown dark.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct RibbonPins(pub Vec<usize>);

/// The single stock entity's name. Identity by name, like everything else:
/// tree, undo, and save all reach stock through this.
pub const STOCK_NAME: &str = "Stock";

pub use crate::operation::types::WCS_SLOTS;

/// Registers every schema component with a fresh world. Called by
/// `world::new_world()` so every binary (and every test) gets identical
/// registration.
pub fn register_schema(world: &World) {
    world.component::<Name>();
    world.component::<Location>();
    world.component::<Facets>();
    world.component::<Revision>();
    world.component::<GeoSig>();
    world.component::<ReducedFidelity>();
    world.component::<StepBudget>();
    world.component::<Selected>();
    world.component::<StockDef>();
    world.component::<StockSourceOnly>();
    world.component::<GeomPrintMemo>();
    world.component::<Wcs>();
    world.component::<ToolDef>();
    world.component::<ToolBody>();
    world.component::<ToolCutter>();
    world.component::<Operation>();
    world.component::<OperationMeta>();
    world.component::<ProgramEntry>();
    world.component::<Toolpath>();
    world.component::<Units>();
    world.component::<MachineDef>();
    world.component::<PostRef>();
    world.component::<PostOptions>();
    world.component::<ThemeRef>();
    world.component::<PostOptions>();
    world.component::<ColorVision>();
    world.component::<WorkMaterialDef>();
    world.component::<RibbonPins>();
    world.component::<SetupDef>();
    world.component::<ActiveSetup>();
    world.component::<FixtureDef>();
}
