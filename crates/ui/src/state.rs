//! The shell's document state: world + history + deferred actions.
//!
//! Provenance: our own app state, specced from public CAM session behavior.
//! Panel code never mutates the world directly - it queues a `CommandKind`,
//! which is executed once per frame outside the ImGui borrow (dialog code
//! must not run mid-frame).
//!
//! File imports run the parse/tessellation on a worker thread so a heavy
//! STEP never freezes the window; the resulting `ImportModel` command is
//! executed on this thread, so the history stays the single source of truth.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use camcraft_app::clock::Instant;
use std::time::Duration;

use camcraft_app::commands::{
    ActivateSetup, AlignPartToSetup, SetSetupRelative, SetSetupStockSource, SetSetupWorkOffset, AddTool, HolderPick, ApplyCuttingData, ChangeOperationKind, ChangeTool, CommandError, CreateOperation,
    ClearToolpath, CreateSetup, DeleteEntity, DeleteFixture, DeleteOperation, DeleteSetup, DeleteTool,
    EditOperationLinks, EditOperationParams, GenerateOperation, History, ImportModel,
    MoveFixture, MoveModel, PasteOperation, RegenerateAll, RenameOperation, ReorderOperation, RepairModel, SelectEntity, SetOperationGeometry,
    SetFixtureFixed, SetOperationSetup, SetStock, SetToolNumber, SetUnits, SetWcsOrigin,
    SetWorkMaterial, TagFixture, UntagFixture, ImportFixture,
    UpdateModel, theme_of,
};
use camcraft_app::geometry::{
    Point3, feature, quat_from_euler, quat_from_yaw, quat_to_euler, transform_point,
};
use camcraft_app::operation::{
    self, AreaControls, FeatureSel, GeoSel, Height, HeightReference, OpParams, SrcRef,
};
use camcraft_app::schema::{ColorVision, Location, Operation as OperationDef, PostRef, ThemeRef, WCS_SLOTS};
use camcraft_app::stock::{self, StockShape};
use camcraft_app::tools::{Tool, ToolKind, catalog};
use camcraft_app::units::{self, Unit};
use camcraft_app::{machine, model, posting, project, world::new_world};

#[path = "state_per_op.rs"]
mod per_op;
#[path = "state_tool_library.rs"]
mod tool_library_io;
#[path = "state_setup_mount.rs"]
mod setup_mount;
#[path = "state_pockets.rs"]
mod pockets;
pub use pockets::PocketScan;
#[path = "state_setup_post_options.rs"]
mod setup_post_options;
#[path = "state_setup_workholding.rs"]
mod setup_workholding;
#[path = "state_fixture.rs"]
mod fixture_view;
#[path = "state_locate.rs"]
mod locate_ui;
use locate_ui::with_warning;
#[path = "state_fixture_library.rs"]
mod fixture_library;
pub use fixture_library::{FixtureLibDialog, InsertFixtureDraft, LibraryRow, SaveFixtureDraft};
#[path = "state_vise.rs"]
mod vise_ui;
pub use vise_ui::{ViseDraft, ViseOpeningDraft};
#[path = "state_plate.rs"]
mod plate_ui;
pub use plate_ui::{PlateDraft, PlatePlaceDraft};
#[path = "state_modvise.rs"]
mod modvise_ui;
pub use modvise_ui::ModViseDraft;
#[path = "state_accessory.rs"]
mod accessory_ui;
pub use accessory_ui::{AccessoryDraft, AccessorySizeDraft};
#[path = "state_program_entry.rs"]
mod program_entry;
pub use program_entry::EntryVerb;
#[path = "state_tool_body.rs"]
mod tool_body_ui;
pub use tool_body_ui::{BodyDraft, BodyPickMode, ToolBodyUi, ZOOM_MAX, ZOOM_MIN, clamp_zoom};
#[path = "state_tool_profile.rs"]
mod tool_profile_ui;
#[path = "state_tool_cutter.rs"]
mod tool_cutter_ui;
pub use tool_cutter_ui::ToolCutterUi;
#[path = "state_recent.rs"]
mod recent_ui;
#[path = "state_view.rs"]
mod view_prefs_ui;
#[path = "state_prefs.rs"]
mod prefs_ui;
#[path = "state_step_budget.rs"]
mod step_budget_ui;
pub use step_budget_ui::StepBudgetUi;
pub use tool_profile_ui::{ToolColumn, ToolProfileUi, is_spun_kind, parametric_points};
use flecs_ecs::prelude::World;

use crate::egui_app::{theme, theme_files};
use crate::viewport::Camera;

#[derive(Debug)]
pub enum CommandKind {
    OpenModelDialog,
    ImportFile(PathBuf),
    NewDocument,
    OpenProjectDialog,
    OpenProjectFile(PathBuf),
    SaveProjectDialog,
    SaveProjectAsDialog,
    SaveProjectFile(PathBuf),
    /// One recent project or model (#662): the same open / import the
    /// dialogs run, a missing file refused by name.
    OpenRecent(camcraft_app::recent::RecentKind, PathBuf),
    /// "Clear recent" (#662): empty one list (preference state, not undoable).
    ClearRecent(camcraft_app::recent::RecentKind),
    /// Post NC through the save-dialog law (M1.7b): path via rfd, gate
    /// via `posting::post_setup`.
    PostProgramDialog,
    /// The same post at a known path - what the headless tests run.
    PostProgramFile(PathBuf),
    /// One file per operation into a known folder (#111) - the
    /// headless twin of the folder dialog.
    PostProgramFolder(PathBuf),
    Undo,
    Redo,
    DeleteSelected,
    DeleteByName(String),
    Select(Option<String>),
    FrameAll,
    Quit,
    /// The answer to the unsaved-changes prompt (#107): New, Open or Quit
    /// over a dirty document is held in `unsaved_prompt` until this lands.
    ResolveUnsaved(crate::unsaved::UnsavedChoice),
    /// The Setup pane's shape+placement draft; the command itself is built
    /// here, so validation and status reporting live in one place.
    SetStock {
        shape: StockShape,
        pos: [f64; 3],
        yaw_deg: f64,
    },
    /// #1094: the stock page's "raw stock only" checkbox for a from-model
    /// stock's source model, one undo unit (`SetStockSourceOnly`).
    SetRawStockOnly(bool),
    /// Stock that wraps every model's world AABB plus `margin` mm, shaped
    /// like the pane's radio selection at click time (0 box, 1 cylinder).
    StockFit {
        margin: f64,
        shape: i32,
    },
    /// Box stock that wraps every model's world AABB by a separate
    /// allowance per side (#198): `[-X, +X, -Y, +Y, bottom, top]`, mm.
    StockFitSides {
        sides: [f64; 6],
    },
    /// The document's display unit (M1.1): a lens over numbers that all
    /// stay millimeters in the world. Undo restores the previous lens.
    SetUnits(Unit),
    /// The document's machine (M2.3 C): setup furniture like the units.
    /// The dialog arm reads a `.mch` through the file-pick law; the path
    /// arm is what tests run. Clearing states the absence - undo puts
    /// the previous machine back.
    LoadMachineDialog,
    LoadMachineFile(PathBuf),
    ClearMachine,
    /// M3.3 stage C, the machine group's other two hands: pick a
    /// machine from the built-in library BY NAME (a name missing from
    /// the library refuses, it never mounts a neighbor), or open/apply
    /// the editor. Editing is a command too - the draft in the window
    /// is ephemeral, and only Apply writes through `LoadMachine`, so
    /// undo puts the machine's previous numbers back exactly.
    LoadBuiltinMachine(String),
    EditMachineDialog,
    EditMachineApply {
        path: String,
        machine: Box<camcraft_app::machine::Machine>,
    },
    /// The Machines pane's "Use for this setup" (#384): re-read and
    /// re-validate a catalogue machine (a refused file never mounts),
    /// then route it like every machine verb (`state_setup_mount`).
    UseCatalogueMachine(camcraft_app::machine::library::MachineSource),
    /// The post script (M3.3 A): the built-in arm ships, the dialog arm
    /// picks a user `.cps` through the file-pick law, and the path arm
    /// is what the headless tests run. Selection is a command like
    /// every other piece of setup furniture - undo puts the previous
    /// script back, and the file is compiled at selection, refused by
    /// name if it does not run.
    SelectPostBuiltin,
    SelectPostDialog,
    SelectPostFile(PathBuf),
    /// Pick a built-in from the shipped registry by NAME (M3.3 B) -
    /// the combo's index is screen furniture; what the document stores
    /// is the name, so a registry re-growth never re-points a saved
    /// file's post.
    SelectPostNamed(String),
    /// #343: a setup takes its own machine / post (a copy of what it
    /// mounts now) or follows the document's again. Undoable
    /// (`SetSetupMachine` / `SetSetupPost`).
    SetupOwnMachine {
        setup: String,
        own: bool,
    },
    SetupOwnPost {
        setup: String,
        own: bool,
    },
    /// #389: a setup takes its own post options (a copy of what it posts
    /// with now) or follows the document's again (`SetSetupPostOptions`).
    SetupOwnPostOptions {
        setup: String,
        own: bool,
    },
    /// #1031: state (`Some`) or clear a setup's workholding maximum rpm
    /// (`SetSetupWorkholdingRpm`).
    SetSetupWorkholdingRpm { setup: String, max_rpm: Option<f64> },
    /// The post's user options (issue #34): the undoable
    /// `SetPostOptions` with the picked value.
    SetPostOptions(camcraft_app::schema::PostOptions),
    /// The user's theme (M6.1 C; a preference since #877, never on the
    /// history): the two built-ins and a discovered file arrive as a
    /// reference (what the headless tests queue too), and the dialog arm
    /// opens the picker. A `File` theme is parse-checked before it is
    /// set - the surface holds the JSON parser (layering), so a corrupt
    /// theme refuses at the pick and is never stored.
    SelectTheme(ThemeRef),
    SelectThemeDialog,
    /// The viewer's colour-vision remap (M13.4), `SelectTheme`'s twin:
    /// gated (separation + readability through the simulated eye)
    /// before it is set, so a failing mode is never worn or stored.
    SelectColorVision(ColorVision),
    /// The active setup's WCS (#342: one per setup): origin plus full
    /// orientation as yaw-pitch-roll degrees (R = Rz ⊗ Ry ⊗ Rx). The world
    /// stores a unit quaternion; pure-yaw setups use roll = pitch = 0.
    SetWcs {
        pos: [f64; 3],
        roll_deg: f64,
        pitch_deg: f64,
        yaw_deg: f64,
    },
    /// Tool library: create a tool entity (catalog row or hand-typed page).
    AddTool {
        name: String,
        tool: Tool,
    },
    /// Tool library: replace a tool entity's numbers (referencing
    /// operations read Stale from their stamp - no cascade writes).
    ChangeTool {
        name: String,
        tool: Tool,
    },
    /// #221: set (`Some`) or clear (`None`) a tool's controller number.
    SetToolNumber {
        name: String,
        number: Option<u32>,
    },
    DeleteTool(String),
    /// #371: set (`Some`) or clear (`None`) a tool's modelled body - the
    /// tool editor's Apply body / Clear body, one `SetToolBody`.
    SetToolBody {
        name: String,
        body: Option<BodyDraft>,
    },
    /// #93: set a milling tool's drawn spun holder (`Some` points) or
    /// return it to its parametric cylinder (`None`) - the tool editor's
    /// Apply profile, one `SetToolBody::spun`.
    SetToolProfile {
        name: String,
        points: Option<Vec<camcraft_app::spun::SpunPoint>>,
    },
    /// #93 slice 2: set a milling tool's drawn cutter outline (`Some`)
    /// or clear it back to the numbers' outline (`None`) - the tool
    /// editor's Apply cutter, one `SetToolCutter`.
    SetToolCutter {
        name: String,
        points: Option<Vec<camcraft_app::spun::SpunPoint>>,
    },
    /// #93 slice 3: a milling tool's numbers and its drawn cutter landed
    /// together (`Some`: the outline carried onto the numbers; `None`:
    /// the numbers' own) - the outline editor's parameter edits, one
    /// `ReshapeTool`.
    ReshapeTool {
        name: String,
        tool: Tool,
        points: Option<Vec<camcraft_app::spun::SpunPoint>>,
    },
    /// #793: drop a built-in library holder ("BT30 ER20") into a milling
    /// tool's holder at a gauge length (`None`: the entry's own) and a
    /// projection (#792: nose face above the tip; `None`: the tool's
    /// stickout) - the tool editor's Apply holder, one
    /// `SetToolBody::library`.
    SetToolHolder {
        name: String,
        entry: String,
        gauge: Option<f64>,
        projection: Option<f64>,
    },
    /// The document's workpiece material (M13.2); `None` states none.
    SetWorkMaterial(Option<camcraft_app::tools::WorkMaterial>),
    /// The op page's "Apply suggestion" (M13.2): feed + spindle speed
    /// from the cutting-data table, one undo unit.
    ApplyCuttingData(String),
    /// New operation entity; deliberately never generated until
    /// [`CommandKind::GenerateOperation`] runs.
    CreateOperation {
        name: String,
        tool: String,
        src: SrcRef,
        params: OpParams,
    },
    PasteOperation { name: String, operation: OperationDef },
    /// The properties page's parameter draft.
    EditOperationParams {
        name: String,
        params: OpParams,
    },
    /// The same draft with its Boundary group (M11.5): one undo unit.
    EditOperationBoundary {
        name: String,
        params: OpParams,
        area: AreaControls,
    },
    /// The op page's Coolant row (#112): the post's fluid word for one
    /// operation, one undo unit through `EditOperationParams`.
    EditOperationCoolant {
        name: String,
        coolant: camcraft_app::operation::CoolantMode,
    },
    /// The op menu's "After this operation" rows (#110): continue, an
    /// optional or program stop, or a trip home - one undo unit.
    EditOperationAfter {
        name: String,
        after: camcraft_app::operation::AfterOperation,
    },
    /// The op menu's Suppress / Unsuppress row (#110): left out of the
    /// posted program, toolpath kept - one undo unit.
    SuppressOperation { name: String, suppressed: bool },
    /// The operation list's Pause / Home entries (#155): add, change,
    /// suppress - each one undo unit.
    ProgramEntry(EntryVerb),
    /// The post page's "Skip pauses and homes" switch (#110).
    SetSkipStops(bool),
    /// The post page's Output choice (#111): `true` = one file per op.
    SetPerOpOutput(bool),
    /// The op's context-menu retype (M9.1): change the operation's kind
    /// in place, numbers carried by the one retype law, one undo unit.
    ChangeOperationKind {
        name: String,
        kind: usize,
    },
    /// Retarget an operation's tool and/or source (combo changes).
    EditOperationLinks {
        name: String,
        tool: Option<String>,
        src: Option<SrcRef>,
    },
    /// The op's full feature pick list (Geometry section, M1.5 step 7
    /// drives this; adding a chip sends the list with it appended).
    SetOperationGeometry {
        name: String,
        items: Vec<GeoSel>,
    },
    DeleteOperation(String),
    RenameOperation { name: String, label: String },
    ReorderOperation { name: String, target: String, after: bool },
    /// Throw away the operation's generated path and keep the operation
    /// (undoable: the cache comes back byte for byte).
    ClearToolpath(String),
    /// Generate and verify the cached path. The command does the work at
    /// construction, synchronously; status arrives with the result.
    GenerateOperation(String),
    /// The batch fix (M3.1 C): regenerate every Stale operation in one
    /// undo unit. What it finds stale at dispatch is what it fixes.
    RegenerateAll,
    /// Setups (M3.2 A): the context a clamping creates. `datum_from`
    /// clones a setup's stock and work offsets into the new one's own
    /// entities (the transfer law); `None` starts with empty furniture -
    /// except on a document with no setups at all, where the command is
    /// by law the MATERIALIZATION of the implicit first setup.
    CreateSetup {
        name: Option<String>,
        datum_from: Option<String>,
    },
    DeleteSetup(String),
    /// Move the active-setup cursor. Inert for every stamp - it decides
    /// where NEW things join, never what already cuts.
    ActivateSetup(Option<String>),
    /// Pick the program offset a setup posts at (#196): `Some(i)` is
    /// `WCS_SLOTS[i]`, `None` clears the pick - an unset setup is `G54`.
    /// Undoable (`SetSetupWorkOffset`).
    SetSetupWorkOffset {
        setup: String,
        slot: Option<usize>,
    },
    /// Turn the part in a setup (#326): about X, then Y, then Z, degrees,
    /// about `pivot` (#606: the setup's WCS origin, or the centre of the
    /// parts' bounds - the presets'). All zero is "As modelled".
    /// Undoable (`AlignPartToSetup`). Never touches the work offset.
    AlignPart {
        setup: String,
        angles: [f64; 3],
        pivot: camcraft_app::setup_turn::TurnPivot,
    },
    /// Place a setup relative to another (#316): `to` the setup it is a
    /// re-clamp of, turned by `angles` (about X, then Y, then Z, degrees)
    /// from how that setup holds the part; `to: None` unlinks (the part
    /// stays). Undoable (`SetSetupRelative`). Never touches the work offset.
    SetSetupRelative {
        setup: String,
        to: Option<String>,
        angles: [f64; 3],
    },
    /// Pick where a setup's stock comes from (#320): its own, or what the
    /// setup before it leaves behind. Undoable (`SetSetupStockSource`).
    SetSetupStockSource {
        setup: String,
        source: camcraft_app::setup_stock::StockSource,
    },
    /// Lay the part on a G17 / G18 / G19 plane in a setup (#330): one
    /// flat face (#554: faces only), read by the app. Undoable
    /// (`AlignPartToSetup`). Never touches the work offset.
    LayPartOnPlane {
        setup: String,
        picks: Vec<camcraft_app::operation::GeoSel>,
        lay: camcraft_app::pick::PlaneLay,
    },
    /// Turn the part in a setup's work plane (#557): a straight edge or
    /// two points for +Y and/or +X, read by the app. Undoable
    /// (`AlignPartToSetup`). Never touches the work offset.
    TurnPartInPlane {
        setup: String,
        axes: camcraft_app::pick::InPlaneAxes,
    },
    /// Lay the part on a G17 / G18 / G19 plane through three picked
    /// points (#555), read by the app through the face pick's apply
    /// path; optionally the 3-2-1 frame. Undoable (`AlignPartToSetup`).
    /// Never touches the work offset.
    LayPartOnPoints {
        setup: String,
        picks: Vec<camcraft_app::operation::GeoSel>,
        lay: camcraft_app::pick::PointsLay,
    },
    /// Which setup an operation machines in (the Properties page's Setup
    /// combo). Keeps the operation's stamp; the resolved-Z digest answers
    /// Ok-or-Stale from the two setups' placements.
    SetOperationSetup {
        name: String,
        setup: Option<String>,
    },
    /// Fixtures (M3.2 stage B): the marker is the whole fixture, so these
    /// are the marker and placement verbs a model already knows - tag
    /// marks one as the setup context's workholding steel, untag demotes
    /// it (the solid stays put, the collision law stops seeing it),
    /// `MoveFixture` is the placement every part shares, `DeleteFixture`
    /// removes the solid. `ImportFixtureDialog` rides the model-import
    /// worker and tags the landed name on finish - a command constructor
    /// never reads disk, so the STEP tessellates off-thread as always.
    TagFixture {
        name: String,
        setup: Option<String>,
    },
    UntagFixture(String),
    /// Whether the steel stands still while a rotary turns the part
    /// (M13.8 (2)): the fixtures page's check row, one undo unit.
    SetFixtureFixed {
        name: String,
        fixed: bool,
    },
    MoveFixture {
        name: String,
        pos: [f64; 3],
        yaw_deg: f64,
    },
    DeleteFixture(String),
    ImportFixtureDialog,
    /// Locate the part in a fixture, or the fixture on the part, by mates
    /// (#561): one undoable `Locate` (`LocatePart` / `LocateFixture`).
    Locate {
        spec: camcraft_app::fixture_locate::LocateSpec,
        mover: camcraft_app::fixture_locate::Mover,
    },
    /// #339 slice 6: open the fixture library's Save / Insert dialog over
    /// a setup's Fixtures node (`""` - the implicit setup). View state;
    /// saving and deleting entries are confirmed file edits, not undo
    /// entries (the #383 machine-file law).
    FixtureLibrarySaveDialog(String),
    FixtureLibraryInsertDialog(String),
    /// Insert library entry `entry` as a fixture of `setup`, its datum
    /// at `pos` (mm) turned by roll/pitch/yaw degrees (R = Rz ⊗ Ry ⊗ Rx):
    /// one `InsertFixture`, one undo unit.
    InsertFixtureFromLibrary {
        entry: String,
        setup: Option<String>,
        pos: [f64; 3],
        rpy_deg: [f64; 3],
    },
    /// #788: open the "Add built-in vise…" dialog over a setup's
    /// Fixtures node (`""` - the implicit setup). View state.
    BuiltinViseDialog(String),
    /// Place built-in vise `params` into `setup`: fitted to its stock, or
    /// its frame at `pos` (mm) turned `yaw_deg` - one `InsertVise`.
    InsertBuiltinVise {
        setup: Option<String>,
        params: camcraft_app::vise::ViseParams,
        fit_stock: bool,
        pos: [f64; 3],
        yaw_deg: f64,
    },
    /// Change placed vise `name` to `params` (its jaw opening, or the
    /// opening from the setup's stock) - one `SetVise`.
    SetVise {
        name: String,
        params: camcraft_app::vise::ViseParams,
        from_stock: bool,
    },
    /// #819: open "Add fixture plate…" over a setup's Fixtures node.
    FixturePlateDialog(String),
    /// Place built-in plate `params` into `setup`: centred on the machine
    /// table, or its frame at `pos` (mm) turned `yaw_deg` - one `InsertPlate`.
    InsertFixturePlate {
        setup: Option<String>,
        params: camcraft_app::plate::PlateParams,
        on_table: bool,
        pos: [f64; 3],
        yaw_deg: f64,
    },
    /// Put fixture `fixture` on plate `plate` at `at` - one `PlaceOnPlate`.
    PlaceOnPlate {
        fixture: String,
        plate: String,
        at: camcraft_app::plate::MountAt,
        yaw_deg: f64,
    },
    /// Detect fixture `name`'s plate grid from its holes, or (`clear`)
    /// stop it being a plate - one `SetPlateGrid`.
    DetectPlateGrid { name: String, clear: bool },
    /// #820: a mod vise pair on plate `plate`, the fixed block at hole
    /// `at` turned `yaw_deg`, fitted to the stock - one `PlaceModVisePair`.
    PlaceModVisePair {
        plate: String,
        fixed: camcraft_app::modvise::JawParams,
        adjustable: camcraft_app::modvise::JawParams,
        at: String,
        yaw_deg: f64,
    },
    /// #832: change placed jaw block `name`'s figures - one `SetModJaw`.
    SetModJaw {
        name: String,
        params: camcraft_app::modvise::JawParams,
    },
    /// #832: re-fit adjustable jaw block `adjustable` to the stock as it
    /// stands, against the fixed block on its plate - one `RefitModVise`.
    RefitModVise { adjustable: String },
    /// #789: open the "Add accessory…" dialog over a setup's Fixtures
    /// node (`""` - the implicit setup). View state.
    AccessoryDialog(String),
    /// Place built-in accessory `params` into `setup` - one
    /// `InsertAccessory`.
    InsertAccessory {
        setup: Option<String>,
        params: camcraft_app::accessory::Accessory,
        placement: camcraft_app::commands::AccessoryPlacement,
    },
    /// Resize placed accessory `name` to `params` - one `SetAccessory`.
    SetAccessory {
        name: String,
        params: camcraft_app::accessory::Accessory,
    },
    /// Part orientation (M5.2 G): a model's full rigid placement -
    /// position plus yaw-pitch-roll degrees (R = Rz ⊗ Ry ⊗ Rx, the
    /// `SetWcs` voice). The world stores a unit quaternion; the
    /// M3.2 flip is this arm with roll = pitch = 0.
    MoveModel {
        name: String,
        pos: [f64; 3],
        roll_deg: f64,
        pitch_deg: f64,
        yaw_deg: f64,
    },
    /// Mesh repair (M13.11): weld within `weld_tol` (mm) and fill every
    /// boundary loop of at most `max_hole_edges` edges, one undo unit
    /// over the model's facets. Only ever by this verb - never on
    /// import - and refused by name when there is nothing to repair.
    RepairModel {
        name: String,
        weld_tol: f64,
        max_hole_edges: usize,
    },
    /// Setup sheets (M3.2 stage D): render every setup and write the
    /// flip chart, one page per setup, form feeds as page breaks. The
    /// export is a VIEW of the document - no command, no history, no
    /// world write, exactly like posting a program. The dialog arm is
    /// the save-dialog law; the path arm is what headless tests run.
    SetupSheetDialog,
    SetupSheetFile(PathBuf),
    /// #389: one setup's tool list (`None` - the implicit setup) as CSV
    /// or text, the sheet's law: a view, no history. Dialog arm and the
    /// headless path arm.
    ToolListExportDialog(Option<String>),
    ToolListExportFile { setup: Option<String>, path: PathBuf },
    /// M7.2 D: what the program leaves, as binary STL - the traced
    /// stock's field iso-extracted. A view of the document like the
    /// sheet: no command, no history; dialog arm and headless path arm.
    ExportStockDialog,
    ExportStockFile(PathBuf),
}

/// Editable fields of the Setup pane's stock group. Transient UI state,
/// never authoritative: the world's `StockDef` is, refetched by
/// [`State::sync_setup_ui`] whenever the document moves under the pane.
#[derive(Clone, Debug)]
pub struct StockUi {
    /// 0 = box, 1 = cylinder, 2 = from model (radio order in the pane).
    pub shape: i32,
    pub dx: f64,
    pub dy: f64,
    pub dz: f64,
    pub diameter: f64,
    pub height: f64,
    pub source: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw_deg: f64,
    pub fit_margin: f64,
    /// #198: fit a box by a separate allowance per side instead of the
    /// one margin.
    pub fit_per_side: bool,
    /// #198: `[-X, +X, -Y, +Y, bottom, top]`, mm.
    pub fit_sides: [f64; 6],
}

impl Default for StockUi {
    fn default() -> Self {
        // A billet that fits a hand-held part: plausible first numbers beat
        // zeros, which the command layer would only refuse.
        StockUi {
            shape: 0,
            dx: 60.0,
            dy: 60.0,
            dz: 30.0,
            diameter: 40.0,
            height: 20.0,
            source: String::new(),
            x: 0.0,
            y: 0.0,
            z: 0.0,
            yaw_deg: 0.0,
            fit_margin: 2.0,
            fit_per_side: false,
            fit_sides: [2.0; 6],
        }
    }
}

/// Editable fields of the Setup pane's WCS group; see [`StockUi`] for the
/// transient-ness rule. The draft is the active setup's one WCS (#342).
#[derive(Clone, Debug, Default)]
pub struct WcsUi {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Orientation as yaw-pitch-roll degrees (see [`CommandKind::SetWcs`]).
    /// The quick-turn buttons in the pane compose onto these, so the draft
    /// always round-trips with the world's quaternion.
    pub roll_deg: f64,
    pub pitch_deg: f64,
    pub yaw_deg: f64,
    /// XY grid point last clicked (row-major north-to-south, west-to-east;
    /// 4 = center). Display-only highlight; the world stores the position.
    pub anchor_xy: usize,
    /// Z level grid clicks use: 0 bottom, 1 middle, 2 top of the anchor box.
    pub anchor_z: usize,
    /// Which box the anchor points live on: 0 = stock box, 1 = model bounds.
    /// The pane states it explicitly; a chosen source that doesn't exist
    /// falls back to the other, with a note (interact::anchor_reference).
    pub anchor_source: usize,
    /// Viewport picking mode: the anchor points of the current level show as
    /// clickable markers in the viewport. Transient UI mode, not undoable
    /// (AGENTS.md) - the origin change it triggers is the command.
    pub pick_anchor: bool,
    /// Whether the selected slot currently has a placed origin entity. Lets
    /// `sync_setup_ui` zero the readout when an origin is undone without
    /// wiping typing into a never-placed slot.
    pub(crate) placed: bool,
}

/// Editable fields of the Setups page (M3.2 A). Transient like [`StockUi`]:
/// the world's SetupDef entities and the active singleton are the truth.
/// `pick` indexes the page's setup list (clamped at draw); `datum` rides
/// the New button's checkbox - transfer the current setup's stock and
/// WCS into the new one, the shop's
/// re-clamp law.
#[derive(Clone, Debug, Default)]
pub struct SetupsUi {
    pub pick: usize,
    pub datum: bool,
}

/// The fixtures section's placement draft (M3.2 stage B): one fixture's
/// pose, editable before Apply queues the `MoveFixture` - the [`StockUi`]
/// law, and re-synced from the world whenever the row or document moves
/// under the pane.
#[derive(Clone, Debug, Default)]
pub struct FixtureUi {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw_deg: f64,
}

/// The Model page's placement draft (M5.2 G): one part's full rigid
/// pose - position plus yaw-pitch-roll degrees - editable before Apply
/// queues the `MoveModel`, the [`FixtureUi`] law with the two angles
/// the parts page adds. Re-synced from the world whenever the
/// selection or the document moves under the pane; a draft whose
/// model vanished retires with it (the pane never edits a ghost).
#[derive(Clone, Debug, Default)]
pub struct ModelUi {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub roll_deg: f64,
    pub pitch_deg: f64,
    pub yaw_deg: f64,
}

/// The Model page's repair draft (M13.11): the weld tolerance (mm) and
/// the largest hole the fill may close, plus the last report so the
/// page can show what the command did. Transient, never authoritative.
#[derive(Clone, Debug)]
pub struct RepairUi {
    pub weld_tol: f64,
    pub max_hole_edges: usize,
    /// `(model name, report line)` of the last successful repair.
    pub last: Option<(String, String)>,
}

impl Default for RepairUi {
    fn default() -> Self {
        RepairUi {
            weld_tol: 0.001,
            max_hole_edges: 256,
            last: None,
        }
    }
}

/// Editable fields of the Properties pane's operation page. Transient UI
/// state, never authoritative: the world's `Operation` component is,
/// refetched by [`State::sync_ops_ui`] whenever the selection or the
/// document moves under the pane (same typing rule as [`StockUi`]). Since
/// M9.1 it is only ever a mirror: the "new operation" form it once doubled
/// as is gone, and creation rides the ribbon doors and the tree.
#[derive(Clone, Debug)]
pub struct OpUi {
    /// Operation this draft mirrors; empty = nothing selected.
    pub name: String,
    /// Tool entity the operation cuts with.
    pub tool: String,
    /// 0 = the stock solid, 1 = a named model, 2 = feature picks (`SrcRef`).
    pub src_kind: usize,
    /// Model name when `src_kind == 1`.
    pub src_name: String,
    /// The pick list when `src_kind == 2`, mirrored for the Geometry
    /// section's chips and for `src` below. Never edited through the pane
    /// fields: viewport picks and chip removal queue commands, and the
    /// next sync re-mirrors what the world actually holds.
    pub geo: Vec<GeoSel>,
    /// The parameter set; the kind the fields show is `params.kind()`.
    pub params: OpParams,
    /// The Boundary group's draft (M11.5): rides the same Apply as the
    /// parameters, one undo unit.
    pub area: AreaControls,
}

impl Default for OpUi {
    fn default() -> Self {
        OpUi {
            name: String::new(),
            tool: String::new(),
            src_kind: 0,
            src_name: String::new(),
            geo: Vec::new(),
            area: AreaControls::default(),
            // A facing pass over a small billet, in references: 25 above
            // the stock top, skimming the stock top itself. `retype`
            // carries this clearance into every kind, so the draft
            // default must already be the seed shape the panes display -
            // quick-add and the retype menu then say the same thing.
            params: OpParams::Face { holes: Default::default(),
                tilt: None,
                stepover: 3.0,
                clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
                cut_z: Height::of(HeightReference::TopOfStock, 0.0),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
                // Opt-in, like `seed_face` (M11.1 A).
                blend_turns: false,
            },
        }
    }
}

impl OpUi {
    /// Snapshot an operation component into draft form.
    #[must_use]
    pub fn from_op(name: &str, op: &OperationDef) -> Self {
        let (src_kind, src_name, geo) = match &op.src {
            SrcRef::Stock => (0, String::new(), Vec::new()),
            SrcRef::Entity(n) => (1, n.clone(), Vec::new()),
            // Feature picks mirror as kind 2 plus the list itself: the
            // combo cannot express them (the Geometry section draws the
            // chips from `geo`), but a links edit must never rewrite a
            // selection's source by accident - `src` below round-trips it.
            SrcRef::Selection(items) => (2, String::new(), items.clone()),
        };
        OpUi {
            name: name.to_string(),
            tool: op.tool.clone(),
            src_kind,
            src_name,
            geo,
            params: op.params.clone(),
            area: op.area.clone(),
        }
    }

    /// The source the drafts describe, for the queue.
    #[must_use]
    pub fn src(&self) -> SrcRef {
        match self.src_kind {
            0 => SrcRef::Stock,
            2 => SrcRef::Selection(self.geo.clone()),
            _ => SrcRef::Entity(self.src_name.clone()),
        }
    }
}

/// One Geometry-section row: the pane-facing view of a pick-list entry.
/// `label` is the whole display text - a readable descriptor of the
/// geometry the pick names, falling back to entity and short id when the
/// id no longer resolves; `missing` is the amber flag for exactly that.
#[derive(Clone, Debug, PartialEq)]
pub struct GeoChip {
    pub sel: GeoSel,
    pub label: String,
    pub missing: bool,
}

/// A feature id as the chips show it: the 16 hex digits, shortened to
/// head and tail with an ellipsis. Long enough to tell two picks of one
/// model apart at a glance, short enough for a chip row to stay one row.
fn short_id(id: u64) -> String {
    let h = format!("{id:016x}");
    format!("{}…{}", &h[..4], &h[14..])
}

/// XY vertex set of a ring, quantized and sorted: the identity a patch
/// chip dedupes coincident rims by (top and bottom rims of a vertical
/// wall share it; a cone's differ, as they must). Display-only helper -
/// the generator's dedupe is exact and lives in `operation::push_loop`.
fn xy_set_key(pts: &[[f64; 3]]) -> Vec<(i64, i64)> {
    let mut k: Vec<(i64, i64)> = pts
        .iter()
        .map(|p| {
            (
                (p[0] * 1e6).round().clamp(i64::MIN as f64, i64::MAX as f64) as i64,
                (p[1] * 1e6).round().clamp(i64::MIN as f64, i64::MAX as f64) as i64,
            )
        })
        .collect();
    k.sort_unstable();
    k.dedup();
    k
}

/// XY footprint span (w × h) of a point run; 0.0 if it has no extent.
fn span(pts: &[[f64; 3]]) -> f64 {
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in pts {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (hi[0] - lo[0]).max(0.0) * (hi[1] - lo[1]).max(0.0)
}

/// |Area| of a closed XY ring by the shoelace sum.
fn shoelace(pts: &[[f64; 3]]) -> f64 {
    if pts.len() < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..pts.len() {
        let (p, q) = (&pts[i], &pts[(i + 1) % pts.len()]);
        a += p[0] * q[1] - q[0] * p[1];
    }
    (a * 0.5).abs()
}

/// Kåsa circle fit (mean-radius refinement): Some((center, r)) when the
/// points are too many and too non-collinear to leave the fit undefined.
/// Display only, for the chip's "bore Ø" descriptor.
fn fit_circle(pts: &[[f64; 3]]) -> Option<([f64; 2], f64)> {
    let n = pts.len();
    if n < 3 {
        return None;
    }
    let (mut sx, mut sy) = (0.0, 0.0);
    for p in pts {
        sx += p[0];
        sy += p[1];
    }
    let (mx, my) = (sx / n as f64, sy / n as f64);
    let (mut sxx, mut sxy, mut syy, mut bxx, mut byy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for p in pts {
        let (x, y) = (p[0] - mx, p[1] - my);
        let z = x * x + y * y;
        sxx += x * x;
        sxy += x * y;
        syy += y * y;
        bxx += x * z;
        byy += y * z;
    }
    let det = sxx * syy - sxy * sxy;
    if !det.is_finite() || det.abs() <= f64::EPSILON {
        return None;
    }
    let u = (0.5 * bxx * syy - 0.5 * sxy * byy) / det;
    let v = (0.5 * byy * sxx - 0.5 * sxy * bxx) / det;
    let c = [mx + u, my + v];
    let r = pts
        .iter()
        .map(|p| ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2)).sqrt())
        .sum::<f64>()
        / n as f64;
    (r.is_finite() && r > 0.0).then_some((c, r))
}

/// Editable fields of the Tool Library pane's tool page; see [`StockUi`]
/// for the transient-ness rule. `exists` records whether `name` resolved
/// to a tool entity when the draft was fetched: Apply edits, Add creates.
#[derive(Clone, Debug)]
pub struct ToolUi {
    pub name: String,
    pub exists: bool,
    /// The tool's controller number (#221) as the editor shows it: 0 = not
    /// set, and the tool posts as its position in the tool list.
    pub number: u32,
    /// 0 end mill, 1 ball nose, 2 bull nose, 3 drill, 4 tap, 5 bore,
    /// 6 turning insert, 7 grooving blade, 8 threading insert,
    /// 9 form gauge, 10 chamfer mill.
    pub kind: usize,
    pub diameter: f64,
    pub corner_r: f64,
    /// Included degrees of a drill's conical point; inert on the other
    /// kinds (the same rule the app's `Tool` field follows).
    pub tip_angle: f64,
    /// Thread pitch, mm per turn; the tap cycle feeds at rpm x pitch.
    pub pitch: f64,
    /// Insert nose radius; the turning kind's cutting ball, inert on
    /// the mills (the app's `Tool` field's law, mirrored).
    pub nose_radius: f64,
    /// Blade width; the groove kind's cutting edge along the axis -
    /// the kerf's width IS the tool, inert on the others (the app's
    /// `Tool` field's law, mirrored a fourth time).
    pub width: f64,
    pub flute_len: f64,
    pub shank_d: f64,
    pub shank_len: f64,
    pub holder_d: f64,
    pub holder_len: f64,
    pub material_group: String,
    pub rpm: f64,
    pub feed: f64,
    /// M13.2: carried through the draft so an editor Apply never wipes
    /// what the op page's Feeds section stated.
    pub tool_material: Option<camcraft_app::tools::ToolMaterial>,
    pub flutes: Option<u32>,
    /// M11.3: may the tool plunge axially into solid?
    pub center_cutting: bool,
    /// #630: the lathe tool orientation (LinuxCNC 1-9), None = the
    /// kind's default - carried so an Apply never rewrites it.
    pub orientation: Option<u8>,
    /// #891: the turning cutting data (Vc, f/rev, max rpm), None = not
    /// stated - carried so an Apply never drops it.
    pub turning: Option<camcraft_app::tools::TurningData>,
}

impl Default for ToolUi {
    fn default() -> Self {
        // An unnamed copy of the catalog's smallest mill: plausible
        // starting numbers beat zeros the commands would only refuse.
        let starter = catalog().remove(0);
        ToolUi::from_tool("", &starter.1, false)
    }
}

impl ToolUi {
    #[must_use]
    pub fn from_tool(name: &str, t: &Tool, exists: bool) -> Self {
        ToolUi {
            name: name.to_string(),
            exists,
            number: 0,
            kind: match t.kind {
                ToolKind::EndMill => 0,
                ToolKind::BallNose => 1,
                ToolKind::BullNose => 2,
                ToolKind::Drill => 3,
                ToolKind::Tap => 4,
                ToolKind::Bore => 5,
                ToolKind::Turning => 6,
                ToolKind::Groove => 7,
                ToolKind::Thread => 8,
                ToolKind::Form => 9,
                ToolKind::Taper => 10,
                ToolKind::Probe => 11,
                ToolKind::Torch => 12,
            },
            diameter: t.diameter,
            corner_r: t.corner_r,
            tip_angle: t.tip_angle,
            pitch: t.pitch,
            nose_radius: t.nose_radius,
            width: t.width,
            flute_len: t.flute_len,
            shank_d: t.shank_d,
            shank_len: t.shank_len,
            holder_d: t.holder_d,
            holder_len: t.holder_len,
            material_group: t.material_group.clone(),
            rpm: t.rpm,
            feed: t.feed,
            tool_material: t.tool_material,
            flutes: t.flutes,
            center_cutting: t.center_cutting,
            orientation: t.orientation,
            turning: t.turning,
        }
    }

    /// The draft's kind as the app names it: the inverse of the numbering
    /// [`ToolUi::from_tool`] writes (anything unknown reads as an end mill).
    #[must_use]
    pub fn tool_kind(&self) -> ToolKind {
        match self.kind {
            1 => ToolKind::BallNose,
            2 => ToolKind::BullNose,
            3 => ToolKind::Drill,
            4 => ToolKind::Tap,
            5 => ToolKind::Bore,
            6 => ToolKind::Turning,
            7 => ToolKind::Groove,
            8 => ToolKind::Thread,
            9 => ToolKind::Form,
            10 => ToolKind::Taper,
            11 => ToolKind::Probe,
            12 => ToolKind::Torch,
            _ => ToolKind::EndMill,
        }
    }

    /// The tool this draft states. A ball's corner is saved as its ball
    /// (#263, [`Tool::tip_corner_r`]), so the document holds the radius
    /// the readers cut with, not whatever the draft carried over from
    /// another kind.
    #[must_use]
    pub fn to_tool(&self) -> Tool {
        let mut tool = Tool {
            kind: self.tool_kind(),
            diameter: self.diameter,
            corner_r: self.corner_r,
            tip_angle: self.tip_angle,
            pitch: self.pitch,
            nose_radius: self.nose_radius,
            width: self.width,
            flute_len: self.flute_len,
            shank_d: self.shank_d,
            shank_len: self.shank_len,
            holder_d: self.holder_d,
            holder_len: self.holder_len,
            material_group: self.material_group.clone(),
            rpm: self.rpm,
            feed: self.feed,
            tool_material: self.tool_material,
            flutes: self.flutes,
            center_cutting: self.center_cutting,
            orientation: self.orientation,
            turning: self.turning,
        };
        tool.corner_r = tool.tip_corner_r();
        tool
    }
}

/// Yaw in degrees of a (pure-yaw) quaternion: the exact inverse of
/// `quat_from_yaw`, and the Z-twist of any other rotation. Display/draft
/// only - the data model keeps the full quaternion through Apply.
#[must_use]
pub fn quat_yaw_deg(q: [f64; 4]) -> f64 {
    (2.0 * q[2].atan2(q[3])).to_degrees()
}

/// Which viewport drag is live; ephemeral UI state, deliberately not
/// undoable (AGENTS.md). The shell's pointer layer owns it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragMode {
    Orbit,
    Pan,
}

/// Where a Face/Edge viewport pick lands while the Boundary group is
/// collecting (M11.5): an include or exclude boundary, or the open
/// boundary - into the page's draft, committed by Apply like every
/// other draft field. `None` on [`State::boundary_pick`] is the plain
/// selection pick. Ephemeral view state, never a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryPick {
    /// A new include boundary: the first pick creates it, and the target
    /// becomes [`BoundaryPick::Extra`] of it (`boundary_draft::take_pick`).
    Include,
    /// A new exclude boundary, likewise.
    Exclude,
    /// The open boundary (created by the first pick if there is none).
    Open,
    /// An existing entry of `AreaControls::extra` (M11.5 UX re-pick).
    Extra(usize),
}

/// What a viewport click selects. Transient view state - the mode
/// itself is not undoable; what the click does with it is (a pick in
/// Face/Edge mode queues `CommandKind::SetOperationGeometry`). The
/// same rule `wcs_ui.pick_anchor` follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeoPick {
    /// Whole entities: click raycasts to a facet and selects the model.
    Model,
    /// One planar face of a feature: click raycasts, then maps the facet
    /// to its feature face id.
    Face,
    /// One tangent-continuous edge chain: click finds the nearest visible
    /// feature edge in screen space (Alt narrows it to that single
    /// segment). Falls back to Face behavior when nothing is close.
    Edge,
    /// One feature corner (M13.4): the nearest visible vertex in screen
    /// space, depth-tested against the part. Nothing else falls through.
    Vertex,
}

/// How the viewport paints solid geometry. Transient view state - like
/// the camera and [`GeoPick`], never a command, never saved. The wire is
/// always the same feature-edge set (`geometry::feature`, crease and
/// boundary); the modes differ in what happens *behind* it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayMode {
    /// Flat-shaded faces with the CAD wire over them - the default look.
    #[default]
    ShadedEdges,
    /// Wireframe only: faces go to the depth buffer and nowhere else, so
    /// the model reads as a hidden-line drawing; the stock - never a
    /// surface you trust to occlude anything - draws as its full wire.
    Wireframe,
}

/// The thing under the cursor, from the viewport hover pass: the named
/// entity, the facet index the raycast resolved (into `model::facets_of`'s
/// order for that entity), and - in Face/Edge pick mode - the feature the
/// hover resolves to, so the highlight draws the face boundary loops or the
/// chain instead of one triangle. Ephemeral view state, not a command
/// (AGENTS.md) - it only drives the screen-space highlight, never selection.
#[derive(Clone, Debug, PartialEq)]
pub struct HoverHit {
    pub entity: String,
    pub facet: usize,
    pub feature: Option<FeatureSel>,
}

/// M1.7c move-simulation playhead. View state like the camera and
/// [`DisplayMode`] - never a command, never saved (AGENTS.md exempts
/// playback position explicitly). The timeline itself is not stored
/// here: the viewport pass rebuilds it from the world every frame, so
/// a document switch needs no reset - the same `t` simply indexes the
/// new program.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sim {
    /// Playhead, seconds into the script.
    pub t: f64,
    pub playing: bool,
    /// Playback multiplier; real programs run minutes, so watching one
    /// at 1× is not a workflow - 1/2/4/8 are the transport's entries.
    pub speed: f64,
}

impl Sim {
    /// Arm the simulation at the start of the timeline, playing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            t: 0.0,
            playing: true,
            speed: 1.0,
        }
    }

    /// The playback law, pure so the ui tests pin it deterministically
    /// (frames under the harness advance by ~0): time moves only while
    /// playing, clamps at the timeline's end, and stops there - no
    /// wrap. Pathological seconds (NaN dt or total) freeze the
    /// playhead rather than poison it.
    pub fn advance(&mut self, dt: f64, total: f64) {
        if !self.playing || !(dt.is_finite() && total.is_finite()) {
            return;
        }
        self.t = (self.t + dt.max(0.0) * self.speed.max(0.0)).min(total.max(0.0));
        if self.t >= total {
            self.playing = false;
        }
    }
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

/// The M1.7d G-code viewer's content: the program one successful post
/// wrote, plus its scroll cursor. Unlike [`Sim`] - which is a clock over
/// live toolpaths and rebuilds itself every frame - this holds frozen
/// evidence of one specific file, so a document switch retires it (New
/// and Open clear it; an in-document edit does not: the program still
/// truthfully says what was posted, and re-posting replaces it).
#[derive(Clone, Debug)]
pub struct PostView {
    /// The file the program went to - the window's title.
    pub path: std::path::PathBuf,
    pub program: String,
    /// Per program line, the world position of the move it writes
    /// (`posting::Posted::line_pos`).
    pub line_pos: Vec<Option<[f64; 3]>>,
    /// Window visibility (egui::Window::open).
    pub open: bool,
    /// Sync cursor: the program line at the viewer's middle. Immediate
    /// view state like the playhead (AGENTS.md), written by the window's
    /// pass and read by the viewport's highlight.
    pub line: usize,
    /// The block-by-block trace of this same run (M3.3 stage D): which
    /// record wrote which lines. Frozen evidence like the text beside
    /// it - the viewer shows the trace the post actually did, never a
    /// re-derivation.
    pub blocks: Vec<camcraft_app::post::TraceBlock>,
    /// The trace pane's visibility. View state like the camera - not
    /// undoable, not saved; the block list belongs to the one program
    /// on screen, so a document switch retires the whole viewer.
    pub trace: bool,
    /// The material sim's not-judged line for this program
    /// (`posting::Posted::not_judged`), shown in the viewer's header
    /// row; `None` when the sim judged it.
    pub not_judged: Option<String>,
}

/// A dock panel the ribbon's View door can close and reopen. The
/// viewport is not one of them: it is permanent (the dock refuses to
/// close it), so the type cannot even name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Browser,
    Properties,
    /// The Machines pane (#287): closed by default, opened from the View
    /// door beside the properties pane.
    Machines,
}

/// A dock-layout ask from the View door, applied by the shell on the
/// same frame (the dock lives beside `State`, not in it). Layout is
/// view state like the camera - immediate-mode, never a command, never
/// saved - so this is a one-frame mailbox, not a history entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutRequest {
    /// Close the panel if open, reopen it at its default side if not.
    Toggle(Panel),
    /// Rebuild the default three-pane dock.
    Reset,
}

/// Which closable panels the dock holds this frame - mirrored from the
/// dock by the shell before the ribbon draws, so the View door's
/// checkmarks read the truth without the ribbon touching the dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelsOpen {
    pub browser: bool,
    pub properties: bool,
    pub machines: bool,
}

impl PanelsOpen {
    #[must_use]
    pub fn get(self, p: Panel) -> bool {
        match p {
            Panel::Browser => self.browser,
            Panel::Properties => self.properties,
            Panel::Machines => self.machines,
        }
    }
}

/// Snapshot held by Copy until another operation is copied. Generated
/// toolpaths are intentionally absent: Paste creates a fresh operation.
#[derive(Clone)]
pub struct OperationClipboard {
    pub label: String,
    pub operation: OperationDef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeMenuTarget {
    Operation(String),
    Operations,
    /// #339 slice 4: a model row (Models section, or a setup's Fixtures
    /// child) - "Use as fixture" / "Stop using as fixture". `row` is the
    /// clicked row's index, so the menu anchors on the row clicked even
    /// when the name stands in the tree twice.
    Model { name: String, row: usize },
    /// A setup's Fixtures node (`""` - the implicit setup): "Import
    /// fixture…".
    Fixtures(String),
    /// A Pause / Home entry row (#155).
    Entry(String),
}

pub struct State {
    pub world: World,
    pub history: History,
    pub status: String,
    /// Last successfully opened or saved project in this window.
    pub project_path: Option<PathBuf>,
    /// #225: the Feeds tab shows the spindle speed as surface (cutting)
    /// speed instead of rpm. View state - never a command, never saved.
    pub spindle_as_vc: bool,
    /// #899: how the Tool library page's Import treats a name the
    /// document already has. View state - the import itself is the command.
    pub tool_import_mode: camcraft_app::commands::ImportMode,
    /// Bumped whenever documents changed; drives GPU re-upload of meshes.
    pub doc_version: u64,
    /// Bumped whenever selection changed; re-sends the vertex stream (the
    /// selection tint is baked per vertex) without re-uploading geometry.
    pub selection_version: u64,
    /// Viewport image rect in logical screen coords; the shell publishes it
    /// each frame, `interact` hit-tests pointer events against it.
    pub viewport_rect: ([f32; 2], [f32; 2]),
    /// Live viewport drag (orbit/pan), driven by the shell's pointer layer.
    pub drag: Option<DragMode>,
    /// Facet currently under the cursor (`interact` refreshes it from the
    /// shell's pointer events with a filtered raycast; it clears when the
    /// pointer leaves the viewport). View state like the camera and the
    /// drag - not undoable, not saved.
    pub hover: Option<HoverHit>,
    /// The machined mesh's facet under the pointer (#25), world space:
    /// set with `hover` when the mesh answers a stock hover, cleared on
    /// every hover refresh. View state - not undoable, not saved.
    pub hover_mesh: Option<camcraft_app::geometry::Triangle>,
    /// True when the viewport panel was under the pointer last frame (the
    /// shell writes it, and gates pointer drags on it - a click on a menu
    /// or popup over the image must never orbit).
    pub viewport_hot: bool,
    pub cam: Camera,
    /// Models hidden in the viewport (eye toggle in the browser). View
    /// state like the camera - not undoable, not saved; a hidden model
    /// still cuts: visibility is display, not suppression (a toolpath
    /// computed from it keeps running - hiding geometry must never
    /// silently change what the machine does).
    pub hidden_models: Vec<String>,
    /// Stock entities hidden from the viewport by the browser eye.
    pub hidden_stocks: Vec<String>,
    /// Work-offset gizmos hidden by their browser eyes.
    pub hidden_offsets: Vec<String>,
    /// Operations whose cached path stays undrawn (eye toggle per op).
    /// Same rule: the path still exists and posts.
    pub hidden_paths: Vec<String>,
    pub operation_clipboard: Option<OperationClipboard>,
    /// Expanded state of Setup, Models, Tools, and Operations in the browser.
    pub browser_expanded: [bool; 3],

    /// Show the dimmed world origin axes in the viewport. View state like
    /// the camera - not undoable, not saved - toggled from the Models panel;
    /// the triads of real work offsets are always shown, because they are
    /// setup, not decoration. Hiding the world axes is for the moment a
    /// G5x offset sits away from machine zero and two gizmos read as clutter.
    pub show_world_axes: bool,
    /// Viewport picks may reach the far side of the part (#692): when on,
    /// a click sends `through: true` and can pick a hidden edge or point.
    /// Off by default, shown as off. View state like the axes toggle -
    /// not undoable, not saved; flipped from the viewport's nav column.
    pub pick_through: bool,
    /// Do the origin gizmos wear their names (#727 drew them, #645
    /// asked for the switch)? View state like the camera - never a
    /// command, never on the history - but unlike the camera it is a
    /// kept preference: read at startup from the per-user view file
    /// and written back when the browser checkbox flips it. Default
    /// ON, the #727 decision; the `X`/`Y`/`Z` axis letters never
    /// depend on it - only the names (`World`, each offset's entity
    /// name) do.
    pub show_gizmo_names: bool,
    /// Show cached toolpaths beyond the selected operation's: with this on,
    /// every generated path draws (dimmed where unselected) instead of only
    /// the one the tree highlights. View state like the camera - not
    /// undoable, not saved; the overlay rebuild gate reads it.
    pub show_all_paths: bool,
    /// How solids paint: shaded-with-wire or hidden-line wireframe. View
    /// state like the camera - not undoable, not saved; the render call
    /// reads it, nothing in the document does.
    pub display: DisplayMode,
    /// M1.7c move simulation: None while off, [`Sim`] while the
    /// transport bar is up. View state like the camera - what it holds
    /// is only the playhead; the timeline is rebuilt from the world
    /// each frame, so editing or switching documents never strands it.
    pub sim: Option<Sim>,
    /// M1.7d G-code viewer: None after any document switch or refused
    /// post, `Some` while a posted program is being reviewed.
    pub post_view: Option<PostView>,
    /// #265: the last post's refusal, `Some` from a refused post until
    /// the user closes its window, a later post succeeds, or the
    /// document is switched. The status line says it too, but the next
    /// click or command writes over that line; this keeps the reason.
    pub post_refusal: Option<crate::egui_app::post_refusal::PostRefusal>,
    /// #157: the Debug info window is open. View state - never a
    /// command, never saved.
    pub debug_info_open: bool,
    /// #157: the GPU the window came up on (`gpu::describe`'s line),
    /// set once by the windowed shell; `None` headless.
    pub gpu_line: Option<String>,
    /// M2.3 program material sim: None while the Deviate cell is dark,
    /// `Some` while its sheet paints the stock's place. Frozen evidence
    /// of one program state, exactly like [`PostView`]'s text - never a
    /// command, never saved, retired by a document switch; an edit does
    /// not silently re-run it, re-arming re-judges.
    pub program_sim: Option<camcraft_app::stock_sim::ProgramSim>,
    /// M7.2 B3b: the program walked through the field arm, for the
    /// traced viewport stock. `None` until a GPU-backed frame asks for
    /// it (`actions::ensure_field_view`), then the view or its refusal -
    /// computed once per arming, retired with `program_sim`.
    pub field_view: Option<Result<std::sync::Arc<camcraft_app::stock_view::FieldView>, String>>,
    /// M7.2 C: the playback field while the transport is up - the stock
    /// cut to the playhead on a worker thread, its newest picture
    /// feeding `field_view` (dropping it stops and joins the worker).
    pub field_play: Option<camcraft_app::stock_play_worker::PlayWorker>,
    /// The transport's timeline, built once per `doc_version` (never
    /// per frame: it walks every op's path) - see `shell::cached_script`.
    pub sim_script_cache: Option<(u64, std::sync::Arc<camcraft_app::sim::Script>)>,
    /// #743: the revolved mesh of the tool at the playhead, as the
    /// collision laws read it - rebuilt only when its judged outline
    /// changes, emptied when the tool, its operation or the transport
    /// is gone. Display-only (a Mutex because the scene is collected
    /// from `&State`).
    pub sim_tool_cache: std::sync::Mutex<camcraft_app::sim_tool_body::SimToolCache>,
    /// What `field_view` was walked for: (transport up, Deviate armed,
    /// the document version a playback plan was frozen at). A change
    /// retires the view and the playback - never a stale picture.
    pub field_mode: Option<(bool, bool, u64)>,
    /// M7.2 D: the machined stock's mesh, one extraction per field view.
    pub stock_mesh: camcraft_app::stock_export::MeshCache,
    /// The open ribbon section dropdown (M6.2): while `Some`, that
    /// section's overflow button stands pressed and its folded kinds
    /// draw as a popup column. View state like the sim's playhead -
    /// not undoable, not saved; the ribbon's draw retires it when a
    /// machine or document change takes the section away.
    pub ribbon_more: Option<camcraft_app::operation::Section>,
    /// The machine door's open flag (the ribbon law): same law as
    /// `ribbon_more` - a frame fact, not a document fact, retired by
    /// any choice or outside press; at most one dropdown is open at a
    /// time, so the two toggles close each other.
    pub ribbon_mode_open: bool,
    /// The View door's open flag: the same dropdown law as
    /// `ribbon_mode_open` (one door open at a time, a frame fact).
    pub ribbon_view_open: bool,
    /// The Theme door's open flag (#441): the same dropdown law as
    /// `ribbon_view_open` - a frame fact, one door open at a time.
    pub ribbon_theme_open: bool,
    /// The open recent-files drop-down (#662) under the Open (projects)
    /// or Model (models) button - the same door law, one open at a time.
    pub ribbon_recent_open: Option<camcraft_app::recent::RecentKind>,
    /// The recent projects and models (#662): user preference state,
    /// never document state and never on the history.
    pub recent: camcraft_app::recent::RecentFiles,
    /// Where the recents are written back; `None` (every headless
    /// State) keeps them in memory only.
    pub recent_store: Option<PathBuf>,
    /// Where the view preferences (#645) are written back; `None`
    /// (every headless State) keeps them in memory only.
    pub view_store: Option<PathBuf>,
    /// Where the theme and colour-vision preferences (#877) are written
    /// back; `None` (every headless State) keeps them in memory only.
    pub prefs_store: Option<PathBuf>,
    /// #889: the Tool library page's "Import tool holders" - a per-user
    /// preference (kept beside the theme), the `holders` parameter of
    /// the `ImportTools` command an import runs. On by default.
    pub import_tool_holders: bool,
    /// The dock's closable panels as of this frame (see [`PanelsOpen`]).
    pub panels_open: PanelsOpen,
    /// The View door's pending dock ask (see [`LayoutRequest`]).
    pub layout_request: Option<LayoutRequest>,
    /// The browser row whose retype menu stands open (M9.1): the op's
    /// name, anchored to its row and recomputed per frame like the
    /// ribbon's dropdowns - a frame fact, not a document fact; the
    /// draw retires it on any choice, outside press, or a row whose
    /// operation is gone.
    pub tree_menu: Option<TreeMenuTarget>,
    /// A type change chosen once in the operation menu; a second click confirms it.
    pub retype_confirm: Option<(String, usize)>,
    /// Inline rename editor: stable key and current draft.
    pub rename_draft: Option<(String, String)>,
    /// Operation key being dragged in the browser; discarded on release.
    pub dragging_op: Option<String>,
    pub pending: VecDeque<CommandKind>,
    /// #183: the live MCP session's inbox when the app was started with
    /// `--mcp-port` / `CAMCRAFT_MCP_PORT`; drained with `pending`.
    pub mcp: Option<camcraft_app::mcp::Inbox>,
    pub request_quit: bool,
    /// #107: a New / Open / Quit held back because the document has
    /// unsaved changes; while Some, the shell shows the Save / Don't Save
    /// / Cancel prompt. Ephemeral UI state - never on the history.
    pub unsaved_prompt: Option<CommandKind>,
    /// Set only while a held action runs after the user chose to discard
    /// (or saved first), so the guard lets it through once.
    pub(crate) discard_confirmed: bool,
    /// Receiver for the in-flight import worker (None when idle). The worker
    /// runs `ImportModel::new` off-thread and sends the finished command back.
    import_rx: Option<mpsc::Receiver<Result<ImportModel, CommandError>>>,
    /// Path currently being imported on the worker; drives the status text.
    import_file: Option<PathBuf>,
    /// Wall-clock start of the live import, for an elapsed-seconds readout.
    import_started: Option<Instant>,
    /// Imports queued while one is already running (drained FIFO on finish).
    import_queue: VecDeque<PathBuf>,
    /// Setup pane drafts (ephemeral; see [`StockUi`]).
    pub stock_ui: StockUi,
    pub wcs_ui: WcsUi,
    /// Setups page draft (ephemeral; see [`SetupsUi`]).
    pub setups_ui: SetupsUi,
    /// Fixtures section placement draft (ephemeral; see [`FixtureUi`]).
    pub fixture_ui: FixtureUi,
    /// Model page placement draft (ephemeral; see [`ModelUi`]).
    pub model_ui: ModelUi,
    /// The Model page's repair draft (M13.11).
    pub repair_ui: RepairUi,
    /// The Model page's last Detect pockets / slots pass (#511).
    pub pocket_scan: PocketScan,
    /// The setups-page context (setup binding) the next finished import
    /// gets tagged with - `Some` only between ImportFixtureDialog and the
    /// import's finish; discarded with the import itself.
    /// Set by the import-fixture dialog, consumed by `finish_import`: the
    /// binding the landed model is tagged with. `Some(None)` is the
    /// implicit first setup. Public so tests run the dialog hand-off
    /// without a modal file picker.
    pub import_as_fixture: Option<Option<String>>,
    /// #1054: the STEP facet budget the next import runs under (an import
    /// setting, not on the history; the import that uses it is).
    pub step_budget: StepBudgetUi,
    /// #789: each built-in accessory row's size draft, by name - view
    /// state; Apply lands it through `SetAccessory`.
    pub accessory_sizes: HashMap<String, AccessorySizeDraft>,
    /// #339 slice 4: each fixture's mesh state, per document version
    /// (view cache - never truth, never saved).
    pub fixture_mesh_cache: camcraft_app::fixture_view::MeshStateCache,
    /// #339 slice 6: the open fixture library dialog (view state).
    pub fixture_lib: Option<FixtureLibDialog>,
    /// The fixture library folder override (tests; `None` = the user
    /// folder, `CAMCRAFT_FIXTURES` first).
    pub fixture_lib_dir: Option<PathBuf>,
    /// #788: each built-in vise row's jaw-opening draft, by vise name -
    /// view state; Set lands it through `SetVise`. See
    /// [`State::vise_opening_draft`].
    pub vise_opening: HashMap<String, ViseOpeningDraft>,
    /// #819: each fixture row's "Place on plate" draft, by fixture name.
    pub plate_place: HashMap<String, PlatePlaceDraft>,
    /// #820: each plate row's "Add mod vise pair" draft, by plate name.
    pub mod_vise: HashMap<String, ModViseDraft>,
    /// #832: each jaw block row's figure-edit draft, by block name.
    pub mod_jaw_edit: HashMap<String, camcraft_app::modvise::JawParams>,
    /// The selected op's struck fixtures, per (version, op, path sig).
    pub struck_cache: camcraft_app::fixture_view::StruckCache,
    /// Properties / Tool Library pane drafts (ephemeral; see [`OpUi`]).
    pub op_ui: OpUi,
    /// Viewport click routing: whole models vs feature faces vs edge
    /// chains. Ephemeral view state like the drafts above - the mode is
    /// set from the Operations page radio row, never saved or undone.
    pub geo_pick: GeoPick,
    /// The Boundary group's pick target (M11.5); `None` routes picks to
    /// the selection as always.
    pub boundary_pick: Option<BoundaryPick>,
    /// The Setups page's lay-on-a-plane draft (#330); `Some` routes
    /// Face picks into it ahead of the boundary and the selection.
    pub plane_pick: Option<crate::plane_pick::PlanePick>,
    /// The locate-in-fixture mate draft (#561); `Some` routes Face and
    /// Vertex picks into it ahead of everything else.
    pub locate_pick: Option<crate::locate_pick::LocateDraft>,
    /// Was the locate form drawn this frame? Reset before the dock, set
    /// by the form; a draft whose form is off screen is dropped after
    /// the dock (`locate_pick::sweep`) - a kept draft must never capture
    /// picks with no form to show them (review of #566).
    pub locate_form_drawn: bool,
    /// The Setups page's axes-in-the-plane draft (#557); while it is
    /// collecting, Edge/Vertex picks land in it ahead of everything else.
    pub axis_pick: Option<crate::axis_pick::AxisDraft>,
    /// The Setups page's three-points draft (#555); `Some` routes Vertex
    /// picks into it (and refuses faces and edges by name).
    pub points_pick: Option<crate::points_pick::PointsPick>,
    /// The axes-in-the-plane rows were drawn this frame (#557): a
    /// feature click with no axis armed says what to do, never falls
    /// through to the operation pick.
    pub axis_rows_shown: bool,
    /// The setups whose pick rows (plane, axes) were drawn this frame
    /// (#664): any other setup's armed pick is disarmed at frame end.
    pub setup_pick_rows: Vec<String>,
    /// The Setups page's WCS-from-features role draft (#701); while it
    /// is collecting, feature picks land in it ahead of everything else.
    pub wcs_pick: Option<crate::wcs_pick::WcsPick>,
    /// The Boundary group's view state and caches (M11.5 UX): open this
    /// frame, the draft's machined-area preview, the entries' shapes.
    pub boundary_view: crate::boundary_draft::BoundaryView,
    /// The incoming stock's viewport sheet (#406), cached per document
    /// version while a carried setup's settings page is open.
    pub incoming_view: crate::incoming_view::IncomingView,
    /// The Geometry list row under the pointer this frame, if any: the
    /// pane answers "which one is that?" by lighting the feature in the
    /// viewport, like pointer hover does. Ephemeral view state (the
    /// hover itself is the precedent) - refreshed every frame, never a
    /// command, never an overlay rebuild.
    pub geo_hover: Option<GeoSel>,
    /// Which named section of the operation page is open (the M1.4 tab
    /// bar, restored after M9.1 B's stack on user feedback). An index into
    /// `egui_app::op_tabs::SECTIONS`. Ephemeral pane state like the drafts
    /// it reveals - never a command, never saved; a kind without the
    /// section falls back to the first tab. Deliberately NOT inside
    /// [`OpUi`]: the drafts rebuild on every sync, and a tab reset on
    /// Apply or on reselecting the same op would read as a bug.
    pub op_tab: usize,
    pub tool_ui: ToolUi,
    /// The tool editor window (M1.4): `None` closed, `Some(name)` editing
    /// that tool, `Some("")` the blank creation form. Ephemeral window
    /// state - never a command, never saved, never
    /// undoable; the edits it drives are. `State::drain_actions` closes it
    /// when the named tool vanishes (the ghost law, mirrored from
    /// [`State::pane_target`]).
    pub tool_editor: Option<String>,
    /// #371: the tool editor's body draft (imported mesh, placement,
    /// pick mode). Ephemeral like `tool_ui`; Apply body is the command.
    pub tool_body_ui: ToolBodyUi,
    /// #93: the tool editor's spun-profile draft (Parametric/Custom, the
    /// points). Ephemeral like `tool_body_ui`; Apply profile is the command.
    pub tool_profile_ui: ToolProfileUi,
    /// #93 slice 2: the tool editor's drawn-cutter draft; Apply cutter
    /// is the command.
    pub tool_cutter_ui: ToolCutterUi,
    /// The machine editor window (M3.3 stage C): `None` closed. An
    /// ephemeral draft like [`State::tool_editor`] - the numbers under
    /// edit are a copy of the machine until Apply pushes `LoadMachine`,
    /// which is the command, which is the undo.
    pub machine_editor: Option<crate::egui_app::machine_editor::MachineEditor>,
    /// The Machines pane's view state (#287): the cached catalogue and
    /// the selected row. Ephemeral like the dock itself - browsing a
    /// machine is not a command; assigning one will be.
    pub machines_pane: crate::egui_app::machines_pane::MachinesPane,
    /// M6.1 C's resolution furniture (never a command, never saved -
    /// the camera's law): the parse cache for `File` themes, the
    /// discovered theme files behind the switcher's rows, and the last
    /// reference the tokens were resolved for - the guard that keeps
    /// resolve/refuse work off the per-frame path. The live table
    /// itself is `theme::current()`'s cell; the key here only says
    /// whether that cell still matches the document.
    pub(crate) theme_cache: theme_files::Cache,
    pub(crate) theme_paths: Vec<PathBuf>,
    theme_key: ThemeRef,
    /// `doc_version` the setup drafts were last refetched at.
    ui_synced_doc: u64,
    /// (draft identity, `doc_version` it was fetched at) per pane; see
    /// [`State::sync_ops_ui`] for the rule.
    op_synced: (String, u64),
    tool_synced: (String, u64),
    /// Which entity the docked context panes edit right now. Ephemeral view
    /// state like the camera - not a command, not undoable (AGENTS.md); the
    /// selection its clicks mirror is the command. [`State::pane_target`]
    /// validates it against the world every frame, so a target that
    /// disappears under it (delete, undo) falls back to the selection on
    /// its own. Replaces the old per-pane show-flags and the `.opened()`
    /// dance: the panes are persistent, what changes is their subject.
    pub ui_target: UiTarget,
    /// The last command's refusal, tagged with the pane it belongs to, so
    /// the failing pane can show the reason inline. The bottom status strip
    /// alone was invisible enough that "Add does nothing" read as a dead
    /// button. Cleared by every command that runs (see [`State::run`]).
    pub feedback: Option<(ErrorScope, String)>,
    /// What an operation-scope refusal described when it fired (the op,
    /// the document version, the page draft): once any of it moves, the
    /// refusal retires - see [`crate::feedback`].
    pub feedback_ctx: Option<crate::feedback::FeedbackCtx>,
    /// #261 part 2: the world spot a refused generation named, beside
    /// [`State::feedback`]; read only through [`State::refusal_focus`],
    /// which retires it with the refusal it belongs to.
    pub feedback_at: Option<[f64; 3]>,
    /// #261 part 3: the moves a refused generation had built, beside
    /// [`State::feedback`]; read only through [`State::refusal_ghost`].
    /// Display-only and ephemeral: never cached on the operation, freed
    /// with the refusal it belongs to.
    pub feedback_ghost: Option<camcraft_app::machining2d::Ghost>,
}

/// Which page of the Operations pane a refused command's message belongs
/// to (so a refusal lands where the user is looking, not only in the
/// status strip).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ErrorScope {
    /// The Tool page (add/update/delete a tool).
    Tool,
    /// The Operation page (create/edit/generate/delete an operation).
    Operation,
    /// The Stock and WCS pages.
    Setup,
}

/// What the docked context panes should edit. Set by browser-tree and
/// ribbon clicks and the menu through [`crate::actions::target`]; view
/// state, never a command (the `Select` a click mirrors into the world is
/// the command).
#[derive(Clone, Debug, PartialEq)]
pub enum UiTarget {
    /// Nothing targeted: panes fall back to the world selection.
    None,
    /// The stock solid (its page doubles as the creation form).
    Stock,
    /// The active setup's one WCS (#342).
    Wcs,
    /// A model entity: the info page.
    Model(String),
    /// A tool entity, or the empty name for the tool page as destination
    /// (the catalog / new-draft form, e.g. CAM > Tool Library).
    Tool(String),
    /// An operation entity: its properties.
    Operation(String),
    /// The Setups page (M3.2 A): always resolves - it doubles as the
    /// creation form, like Stock and the empty-name Tool page.
    Setups,
    /// One setup's own page from its browser node (#343): the setup's
    /// settings, machine, post, fixtures or tool list. The empty name is
    /// the implicit context of a document without setups.
    SetupNode(String, SetupPage),
}

/// Which of a setup's browser nodes a [`UiTarget::SetupNode`] opens (#343).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupPage {
    Settings,
    Machine,
    Post,
    Fixtures,
    Tools,
}

impl UiTarget {
    /// Whether this target still has something to show: named entities
    /// must exist with the component they were targeted for (an undone
    /// operation stops holding the pane), slot indexes must be real, and
    /// the pages that double as creation forms - Stock, and the tool page
    /// with an empty name - survive having no entity yet.
    #[must_use]
    pub fn resolves_in(&self, world: &World) -> bool {
        match self {
            UiTarget::None => false,
            UiTarget::Stock => true,
            UiTarget::Wcs => true,
            UiTarget::Model(name) => model::model_entities(world)
                .into_iter()
                .any(|(n, _)| *name == n),
            UiTarget::Tool(name) => {
                name.is_empty()
                    || model::find_by_name(world, name).is_some_and(|e| model::is_tool(world, &e))
            }
            UiTarget::Operation(name) => {
                model::find_by_name(world, name).is_some_and(|e| model::is_operation(world, &e))
            }
            UiTarget::Setups => true,
            UiTarget::SetupNode(name, _) => {
                if name.is_empty() {
                    camcraft_app::setup::names(world).is_empty()
                } else {
                    camcraft_app::setup::exists(world, name)
                }
            }
        }
    }
}

impl State {
    #[must_use]
    pub fn new() -> Self {
        // The theme folder is a run-time lookup (M13.3): beside the exe
        // in the zip, the repo on a dev run. None found is said ONCE,
        // here, on the status line - the built-in rows stand regardless.
        let themes = crate::theme_dir::from_process();
        let status = match &themes {
            Ok(_) => String::from("ready"),
            Err(why) => why.clone(),
        };
        State {
            world: new_world(),
            history: History::default(),
            status,
            project_path: None,
            debug_info_open: false,
            gpu_line: None,
            spindle_as_vc: false,
            tool_import_mode: camcraft_app::commands::ImportMode::default(),
            doc_version: 0,
            selection_version: 0,
            viewport_rect: ([0.0, 0.0], [0.0, 0.0]),
            drag: None,
            hover: None,
            hover_mesh: None,
            viewport_hot: false,
            geo_pick: GeoPick::Model,
            boundary_pick: None,
            plane_pick: None,
            locate_pick: None,
            locate_form_drawn: false,
            axis_pick: None,
            points_pick: None,
            axis_rows_shown: false,
            setup_pick_rows: Vec::new(),
            wcs_pick: None,
            boundary_view: Default::default(),
            incoming_view: Default::default(),
            geo_hover: None,
            op_tab: 0,
            hidden_models: Vec::new(),
            hidden_stocks: Vec::new(),
            hidden_offsets: Vec::new(),
            hidden_paths: Vec::new(),
            operation_clipboard: None,
            browser_expanded: [true; 3],
            cam: Camera::default(),
            show_world_axes: true,
            pick_through: false,
            show_gizmo_names: true,
            show_all_paths: false,
            display: DisplayMode::default(),
            sim: None,
            post_view: None,
            post_refusal: None,
            program_sim: None,
            field_view: None,
            field_play: None,
            sim_script_cache: None,
            sim_tool_cache: std::sync::Mutex::default(),
            field_mode: None,
            stock_mesh: camcraft_app::stock_export::MeshCache::default(),
            ribbon_more: None,
            ribbon_mode_open: false,
            ribbon_view_open: false,
            ribbon_theme_open: false,
            ribbon_recent_open: None,
            recent: camcraft_app::recent::RecentFiles::default(),
            recent_store: None,
            view_store: None,
            prefs_store: None,
            import_tool_holders: true,
            panels_open: PanelsOpen { browser: true, properties: true, machines: false },
            layout_request: None,
            tree_menu: None,
            retype_confirm: None,
            rename_draft: None,
            dragging_op: None,
            pending: VecDeque::new(),
            mcp: None,
            request_quit: false,
            unsaved_prompt: None,
            discard_confirmed: false,
            import_rx: None,
            import_file: None,
            import_started: None,
            import_queue: VecDeque::new(),
            stock_ui: StockUi::default(),
            wcs_ui: WcsUi {
                // Neutral highlight: center-middle, before any pick.
                anchor_xy: 4,
                anchor_z: 1,
                ..WcsUi::default()
            },
            setups_ui: SetupsUi::default(),
            fixture_ui: FixtureUi::default(),
            model_ui: ModelUi::default(),
            repair_ui: RepairUi::default(),
            pocket_scan: PocketScan::default(),
            import_as_fixture: None,
            step_budget: StepBudgetUi::default(),
            accessory_sizes: HashMap::new(),
            fixture_mesh_cache: Default::default(),
            fixture_lib: None,
            fixture_lib_dir: None,
            vise_opening: HashMap::new(),
            plate_place: HashMap::new(),
            mod_vise: HashMap::new(),
            mod_jaw_edit: HashMap::new(),
            struck_cache: Default::default(),
            // The tool draft carries a plausible name from the first
            // frame: an empty Name field made Add look dead (refused
            // silently). The operation draft has no form to prefill
            // since M9.1 - the page mirrors a real operation or shows
            // the empty state - so it starts blank and stays blank
            // until a target gives it a reality to mirror.
            op_ui: OpUi::default(),
            tool_ui: ToolUi {
                name: String::from("Tool1"),
                ..ToolUi::default()
            },
            tool_editor: None,
            tool_body_ui: ToolBodyUi::default(),
            tool_profile_ui: ToolProfileUi::default(),
            tool_cutter_ui: ToolCutterUi::default(),
            machine_editor: None,
            machines_pane: Default::default(),
            theme_cache: theme_files::Cache::new(),
            // One walk per window, at construction: the switcher's file
            // rows are furniture, not live filesystem polling (the
            // parse cache re-stats per file on the sync's change edge).
            theme_paths: themes.map(|dir| theme_files::discover(&dir).0).unwrap_or_default(),
            theme_key: ThemeRef::Dark,
            ui_synced_doc: 0,
            op_synced: (String::new(), 0),
            tool_synced: (String::new(), 0),
            ui_target: UiTarget::None,
            feedback: None,
            feedback_ctx: None,
            feedback_at: None,
            feedback_ghost: None,
        }
    }

    /// The refusal a pane of this scope should show inline, if the last
    /// command to run was a refusal that belongs to it.
    #[must_use]
    pub fn error_for(&self, scope: ErrorScope) -> Option<&str> {
        self.feedback
            .as_ref()
            .filter(|(s, _)| *s == scope)
            .filter(|_| self.feedback_live())
            .map(|(_, msg)| msg.as_str())
    }

    /// A blank tool page with a numbered name: New starts a real draft,
    /// not an empty field the Add button would only refuse.
    #[must_use]
    pub fn new_tool_draft(&self) -> ToolUi {
        let n = self.tool_names().len() + 1;
        let starter = catalog().remove(0);
        ToolUi::from_tool(&format!("Tool{n}"), &starter.1, false)
    }

    /// Place a fitted stock (#198 shared it between the single-margin and
    /// per-side fits): the size and XY/Z placement from the fit, the
    /// stock's own rotation kept, through the undoable `SetStock`.
    fn apply_stock_fit(&mut self, fitted: Option<(StockShape, Point3)>) {
        let Some((shape, pos)) = fitted else {
            self.refuse(
                ErrorScope::Setup,
                String::from("stock fit refused: models have no usable extent"),
            );
            return;
        };
        // Fit replaces size and XY/Z placement; a deliberately
        // oriented stock keeps its rotation.
        // The fit replaces the ACTIVE setup's stock, and keeps
        // that entity's deliberate rotation (M3.2 A: the name is
        // resolved here; the command re-resolves at construction).
        let stock_name = camcraft_app::setup::stock_entity_name(
            &self.world,
            camcraft_app::setup::active(&self.world).as_deref(),
        );
        let rot = model::find_by_name(&self.world, &stock_name)
            .and_then(|entity| model::transform_of(&self.world, &entity))
            .map_or_else(|| Location::default().rot, |loc| loc.rot);
        let location = Location { pos, rot };
        match SetStock::new(&self.world, shape, location) {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = String::from("stock fitted to models");
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("stock not fitted: {err}")),
        }
    }

    /// Record a refusal: status strip for everyone, tagged message for the
    /// pane that caused it. An operation-scope refusal is stamped with the
    /// page it was raised on, so it retires with that state.
    fn refuse(&mut self, scope: ErrorScope, msg: String) {
        self.status.clone_from(&msg);
        // The page's draft as the next frame will show it: a document
        // bump earlier in this drain re-mirrors now, not after the
        // snapshot (else the re-mirror would read as a user edit).
        if scope == ErrorScope::Operation {
            self.sync_ops_ui();
        }
        self.feedback_ctx = self.feedback_ctx_for(scope);
        self.feedback = Some((scope, msg));
        // A new refusal names no spot until its caller says so (#261).
        self.feedback_at = None;
        self.feedback_ghost = None;
    }

    /// Refetch the Setup pane's drafts from the world after any document
    /// change (Apply, Undo, Open...). Between bumps the user's in-progress
    /// typing is left alone; a refused command changes nothing, so the
    /// offending values stay visible for correction (Prime Directive).
    pub fn sync_setup_ui(&mut self) {
        if self.ui_synced_doc == self.doc_version {
            return;
        }
        self.ui_synced_doc = self.doc_version;
        // The drafts mirror the ACTIVE setup's furniture (M3.2 A): with
        // no setups declared this is the legacy `Stock`/`G54` entities,
        // byte for byte; with them, the pane edits where the user works.
        let active = camcraft_app::setup::active(&self.world);
        let stock_name = camcraft_app::setup::stock_entity_name(&self.world, active.as_deref());
        if let Some(entity) = model::find_by_name(&self.world, &stock_name)
            && model::is_stock(&self.world, &entity)
        {
            if let Some(loc) = model::transform_of(&self.world, &entity) {
                self.stock_ui.x = loc.pos.x;
                self.stock_ui.y = loc.pos.y;
                self.stock_ui.z = loc.pos.z;
                self.stock_ui.yaw_deg = quat_yaw_deg(loc.rot);
            }
            match model::stock_def_of(&self.world, &entity) {
                Some(StockShape::Box { dx, dy, dz }) => {
                    self.stock_ui.shape = 0;
                    self.stock_ui.dx = dx;
                    self.stock_ui.dy = dy;
                    self.stock_ui.dz = dz;
                }
                Some(StockShape::Cylinder { diameter, height }) => {
                    self.stock_ui.shape = 1;
                    self.stock_ui.diameter = diameter;
                    self.stock_ui.height = height;
                }
                Some(StockShape::FromModel { source }) => {
                    self.stock_ui.shape = 2;
                    self.stock_ui.source = source;
                }
                None => {}
            }
        }
        let placed =
            camcraft_app::setup::wcs_entity(&self.world, active.as_deref())
                .and_then(|entity| model::transform_of(&self.world, &entity));
        match placed {
            Some(loc) => {
                self.wcs_ui.x = loc.pos.x;
                self.wcs_ui.y = loc.pos.y;
                self.wcs_ui.z = loc.pos.z;
                let (roll, pitch, yaw) = quat_to_euler(loc.rot);
                self.wcs_ui.roll_deg = roll.to_degrees();
                self.wcs_ui.pitch_deg = pitch.to_degrees();
                self.wcs_ui.yaw_deg = yaw.to_degrees();
                self.wcs_ui.placed = true;
            }
            None => {
                // Show zero only when an origin this slot *had* disappeared
                // (undo): stale numbers for a deleted entity are a lie.
                // Typing into a never-placed slot survives other edits.
                if self.wcs_ui.placed {
                    self.wcs_ui.x = 0.0;
                    self.wcs_ui.y = 0.0;
                    self.wcs_ui.z = 0.0;
                    self.wcs_ui.roll_deg = 0.0;
                    self.wcs_ui.pitch_deg = 0.0;
                    self.wcs_ui.yaw_deg = 0.0;
                    self.wcs_ui.placed = false;
                }
            }
        }
        // The Model page's placement draft (M5.2 G) follows the same
        // law as the WCS one: whenever the document moves under the
        // pane - an undo, a sibling command - the world's numbers
        // re-enter the draft for the model actually on screen.
        if let UiTarget::Model(name) = self.pane_target()
            && let Some(entity) = model::find_by_name(&self.world, &name)
            && let Some(loc) = model::transform_of(&self.world, &entity)
        {
            self.model_ui.name = name;
            self.model_ui.x = loc.pos.x;
            self.model_ui.y = loc.pos.y;
            self.model_ui.z = loc.pos.z;
            let (roll, pitch, yaw) = quat_to_euler(loc.rot);
            self.model_ui.roll_deg = roll.to_degrees();
            self.model_ui.pitch_deg = pitch.to_degrees();
            self.model_ui.yaw_deg = yaw.to_degrees();
        }
    }

    /// What the context panes actually edit this frame: the explicit
    /// target while it resolves, else the world selection, else nothing
    /// (the creation forms). Panes and drafts route through this instead
    /// of reading the raw target - a vanished one never leaves them
    /// pointing at a ghost.
    #[must_use]
    pub fn pane_target(&self) -> UiTarget {
        if self.ui_target.resolves_in(&self.world) {
            return self.ui_target.clone();
        }
        match self.selected_operation() {
            Some(name) => UiTarget::Operation(name),
            None => UiTarget::None,
        }
    }

    /// Refetch the Properties pane's draft after any document or selection
    /// change. Since M9.1 the operation page's target is always a real
    /// operation - the "new operation" draft form died with its Type
    /// dropdown and Add footer, creation goes through the ribbon's doors
    /// or the tree - so this only ever mirrors the world into the pane.
    pub fn sync_ops_ui(&mut self) {
        let ident = match self.pane_target() {
            UiTarget::Operation(name) => name,
            _ => String::new(),
        };
        if self.op_synced == (ident.clone(), self.doc_version) {
            return;
        }
        self.op_synced = (ident, self.doc_version);
        let name = self.op_synced.0.clone();
        if name.is_empty() {
            return;
        }
        if let Some(entity) = model::find_by_name(&self.world, &name)
            && let Some(op) = model::operation_of(&self.world, &entity)
        {
            self.op_ui = OpUi::from_op(&name, &op);
        }
    }

    /// Arm the Operations page's Apply click: a draft that differs from
    /// the operation queues the edit for the next drain (validated,
    /// undoable, as always); a draft that does not differ changes nothing,
    /// so nothing goes into the history - but the click still answers
    /// through the status strip. Silence on a click reads as a dead
    /// button; this is the same ephemeral-ack pattern the feature picks
    /// use, not a command.
    pub fn apply_operation_params(&mut self, name: &str) {
        let live = model::find_by_name(&self.world, name).and_then(|e| model::operation_of(&self.world, &e));
        let unchanged = live
            .as_ref()
            .is_some_and(|op| op.params == self.op_ui.params && op.area == self.op_ui.area);
        if unchanged {
            self.status = String::from("parameters already applied - nothing changed");
        } else {
            self.push_param_edit(name, live.is_some_and(|op| op.area != self.op_ui.area));
        }
    }

    /// Queue the page draft as one edit: the plain parameter edit, or -
    /// when the Boundary group moved too - the edit that carries both.
    fn push_param_edit(&mut self, name: &str, area_dirty: bool) {
        let (name, params) = (name.to_string(), self.op_ui.params.clone());
        self.pending.push_back(if area_dirty {
            CommandKind::EditOperationBoundary { name, params, area: self.op_ui.area.clone() }
        } else {
            CommandKind::EditOperationParams { name, params }
        });
    }

    /// Arm the Generate click (the pane's button and the menu's item for
    /// the selected operation both route here): a draft that differs from
    /// the operation's committed parameters is applied first - one click,
    /// two honest commands - so the path is never generated from stale
    /// numbers while the panel shows edited ones. A draft that mirrors a
    /// different page (another op's mirror, the new-op form) never flushes
    /// into this operation; it generates the committed parameters alone.
    pub fn generate_operation(&mut self, name: &str) {
        let dirty = name == self.op_ui.name
            && model::find_by_name(&self.world, name)
                .and_then(|e| model::operation_of(&self.world, &e))
                .is_some_and(|op| op.params != self.op_ui.params || op.area != self.op_ui.area);
        let area_dirty = dirty
            && model::find_by_name(&self.world, name)
                .and_then(|e| model::operation_of(&self.world, &e))
                .is_some_and(|op| op.area != self.op_ui.area);
        // A Boundary draft the Apply door would refuse (an open chain in an
        // include / exclude, M11.5 UX) stops here, named, before anything
        // generates from the committed boundary behind the user's back.
        if area_dirty {
            let setup = model::find_by_name(&self.world, name)
                .and_then(|e| model::operation_of(&self.world, &e))
                .and_then(|op| op.setup);
            if let Some(why) =
                operation::area_pick_issue(&self.world, setup.as_deref(), &self.op_ui.area, &self.op_ui.params)
            {
                self.refuse(ErrorScope::Operation, format!("not generated: {why}"));
                return;
            }
        }
        if dirty {
            self.push_param_edit(name, area_dirty);
        }
        self.pending
            .push_back(CommandKind::GenerateOperation(name.to_string()));
    }

    /// Arm the Drill page's "Detect holes" click: recognize against the
    /// operation's source geometry *now* - one pass per click, never per
    /// paint frame - and commit the result as an undoable parameter edit.
    /// The geometry comes from the draft when this pane targets the
    /// operation (what the user sees is what gets stored, so a dirty
    /// draft's other edits ride along and the panel never shows holes the
    /// world does not have); a click aimed at another operation refetches
    /// from the world instead of silently flushing some other pane's
    /// unsaved typing.
    pub fn detect_holes(&mut self, name: &str) {
        let Some(entity) = model::find_by_name(&self.world, name) else {
            self.refuse(
                ErrorScope::Operation,
                format!("no operation named '{name}'"),
            );
            return;
        };
        let Some(op) = model::operation_of(&self.world, &entity) else {
            self.refuse(
                ErrorScope::Operation,
                format!("'{name}' is not an operation"),
            );
            return;
        };
        let draft = name == self.op_ui.name;
        let current = if draft { &self.op_ui.params } else { &op.params };
        // The rule (which list, the turret's radial half, the refusals)
        // lives in the app, shared with the MCP `detect_holes` tool
        // (#312), so a click and an agent store the same holes.
        match operation::detect_holes(&self.world, name, current, op.setup.as_deref(), &op.src) {
            Err(why) => self.refuse(ErrorScope::Operation, why),
            Ok(params) => {
                if draft {
                    self.op_ui.params = params.clone();
                }
                self.pending.push_back(CommandKind::EditOperationParams {
                    name: name.to_string(),
                    params,
                });
            }
        }
    }

    /// The document's display unit - a read of the flecs singleton, so it
    /// is always current whatever ran (Apply, Undo, Open): there is no
    /// mirror to go stale. Millimeters until `SetUnits` or a v8 file says
    /// otherwise (M1.1).
    #[must_use]
    pub fn units(&self) -> Unit {
        units::of(&self.world)
    }

    /// The Geometry section's list: one row per pick-list entry, in order,
    /// resolved against the model as it stands now. Each row *describes*
    /// its pick in machining terms - a face says where it is and how big
    /// ("Face part · z=5.00 · 100.0 mm²"), a chain says whether it closes
    /// and how long ("Chain part · open · 10.0 mm · 2 of 3") - so the
    /// list answers "what did I select?" without a hash decoder. A row
    /// whose id no longer resolves (the model was deleted, or re-imported
    /// with new geometry) reads amber and falls back to its short id with
    /// "missing" - removing it is always safe, because the reduced list
    /// only gets shorter.
    pub fn geo_chips(&self) -> Vec<GeoChip> {
        // Open runs are pieces of one loop the user watches get
        // completed: the "2 of 3" in a run's label counts chain picks
        // of the same entity, in pick order.
        let mut run_totals: HashMap<String, usize> = HashMap::new();
        for g in &self.op_ui.geo {
            if matches!(g.feature, FeatureSel::Chain(_)) {
                *run_totals.entry(g.entity.clone()).or_default() += 1;
            }
        }
        let mut run_seq: HashMap<String, usize> = HashMap::new();
        // Which stitched ring each pick took part in, from the stitcher's
        // own walk - the same one the region geometry is built through,
        // so a row's `· → ring k` cannot disagree with the preview.
        let stitched = operation::stitched_rings(&self.world, &self.op_ui.geo);
        // One lens for the whole row set: chip numbers follow the
        // document's unit (M1.1); the mm law underneath is untouched.
        let unit = units::of(&self.world);
        self.op_ui
            .geo
            .iter()
            .enumerate()
            .map(|(gi, g)| {
                let (mut label, missing) = match g.feature {
                    FeatureSel::Face(id) => {
                        let face = model::find_by_name(&self.world, &g.entity)
                            .and_then(|e| model::features_of(&self.world, &e))
                            .and_then(|f| f.faces.iter().find(|fa| fa.id == id).cloned());
                        match face {
                            Some(f) => {
                                // Which axis the face sits at, on the
                                // sign-canonical normal's own axis: the
                                // offset is signed, so top and bottom
                                // both read as plain z.
                                let (axis, v) = if f.normal[2].abs()
                                    >= f.normal[0].abs().max(f.normal[1].abs())
                                {
                                    ('z', f.offset)
                                } else if f.normal[0].abs() >= f.normal[1].abs() {
                                    ('x', f.offset)
                                } else {
                                    ('y', f.offset)
                                };
                                let mut label = format!(
                                    "Face {} · {axis}={} · {} {}",
                                    g.entity,
                                    units::len(v, unit, 2),
                                    units::area(f.area, unit),
                                    unit.area_suffix()
                                );
                                if f.loops.len() > 1 {
                                    label.push_str(&format!(" · {} loops", f.loops.len()));
                                }
                                (label, false)
                            }
                            None => (
                                format!("Face {} · {} · missing", g.entity, short_id(id)),
                                true,
                            ),
                        }
                    }
                    FeatureSel::Chain(id) => {
                        let found = model::find_by_name(&self.world, &g.entity)
                            .and_then(|e| model::features_of(&self.world, &e))
                            .map(|f| {
                                let c = feature::resolve_chain(&f, id)?;
                                let pts = feature::chain_points(&f, &c);
                                Some((c, pts))
                            })
                            .unwrap_or(None);
                        match found {
                            Some((c, pts)) => {
                                // Length along the polyline; a closed
                                // chain's points do not repeat the start
                                // vertex, so the ring closure counts too.
                                let mut len = 0.0;
                                for w in pts.windows(2) {
                                    let (dx, dy, dz) =
                                        (w[1][0] - w[0][0], w[1][1] - w[0][1], w[1][2] - w[0][2]);
                                    len += (dx * dx + dy * dy + dz * dz).sqrt();
                                }
                                if c.closed && pts.len() > 1 {
                                    let (a, b) = (&pts[0], &pts[pts.len() - 1]);
                                    let (dx, dy, dz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
                                    len += (dx * dx + dy * dy + dz * dz).sqrt();
                                }
                                let label = if c.closed {
                                    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
                                    for p in &pts {
                                        for k in 0..2 {
                                            lo[k] = lo[k].min(p[k]);
                                            hi[k] = hi[k].max(p[k]);
                                        }
                                    }
                                    format!(
                                        "Chain {} · closed · {} {} · {} × {}",
                                        g.entity,
                                        units::len(len, unit, 1),
                                        unit.suffix(),
                                        units::len(hi[0] - lo[0], unit, 1),
                                        units::len(hi[1] - lo[1], unit, 1)
                                    )
                                } else {
                                    let i = run_seq.entry(g.entity.clone()).or_default();
                                    *i += 1;
                                    format!(
                                        "Chain {} · open · {} {} · {} of {}",
                                        g.entity,
                                        units::len(len, unit, 1),
                                        unit.suffix(),
                                        i,
                                        run_totals.get(&g.entity).copied().unwrap_or(1)
                                    )
                                };
                                (label, false)
                            }
                            None => {
                                let label =
                                    format!("Chain {} · {} · missing", g.entity, short_id(id));
                                (label, true)
                            }
                        }
                    }
                    FeatureSel::Patch(id) => {
                        // Resolve everything while the features live:
                        // the rings come back as plain point runs,
                        // XY-deduped by vertex set so a closed wall's
                        // coincident rims read as the one outline they
                        // are. Display only - the generator applies the
                        // same dedupe exactly, through `push_loop`.
                        let found = model::find_by_name(&self.world, &g.entity)
                            .and_then(|e| model::features_of(&self.world, &e))
                            .and_then(|f| {
                                let strips = feature::patches(&f)
                                    .into_iter()
                                    .find(|p| p.id == id)?
                                    .faces
                                    .len();
                                let mut seen: Vec<Vec<(i64, i64)>> = Vec::new();
                                let mut closed: Vec<Vec<[f64; 3]>> = Vec::new();
                                let mut open = false;
                                for r in feature::patch_rings(&f, id)? {
                                    let pts: Vec<[f64; 3]> = r
                                        .verts
                                        .iter()
                                        .filter_map(|&v| f.verts.get(v as usize).copied())
                                        .collect();
                                    open |= !r.closed;
                                    if !r.closed || pts.len() < 3 {
                                        continue;
                                    }
                                    let key = xy_set_key(&pts);
                                    if seen.contains(&key) {
                                        continue;
                                    }
                                    seen.push(key);
                                    closed.push(pts);
                                }
                                Some((strips, closed, open))
                            });
                        match found {
                            Some((strips, closed, _open)) => {
                                let label = match closed
                                    .iter()
                                    .max_by(|a, b| span(a).total_cmp(&span(b)))
                                {
                                    Some(rim) => {
                                        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
                                        for p in rim {
                                            for k in 0..2 {
                                                lo[k] = lo[k].min(p[k]);
                                                hi[k] = hi[k].max(p[k]);
                                            }
                                        }
                                        let shape = match fit_circle(rim) {
                                            Some((c, r))
                                                if rim.iter().all(|p| {
                                                    let d = ((p[0] - c[0]).powi(2)
                                                        + (p[1] - c[1]).powi(2))
                                                    .sqrt();
                                                    (d - r).abs() <= r.max(1e-3) * 0.02
                                                }) =>
                                            {
                                                format!("bore Ø{}", units::len(2.0 * r, unit, 1))
                                            }
                                            _ => format!(
                                                "{} × {}",
                                                units::len(hi[0] - lo[0], unit, 1),
                                                units::len(hi[1] - lo[1], unit, 1)
                                            ),
                                        };
                                        format!(
                                            "Welded face {} · {shape} · {} {} · {strips} strips",
                                            g.entity,
                                            units::area(shoelace(rim), unit),
                                            unit.area_suffix()
                                        )
                                    }
                                    None => format!("Welded face {} · open band", g.entity),
                                };
                                (label, false)
                            }
                            None => {
                                let label = format!(
                                    "Welded face {} · {} · missing",
                                    g.entity,
                                    short_id(id)
                                );
                                (label, true)
                            }
                        }
                    }
                    FeatureSel::Vertex(id) => {
                        match crate::vertex_pick::vertex_world(self, &g.entity, id) {
                            Some(p) => (
                                format!(
                                    "Vertex {} · ({}, {}, {}) {}",
                                    g.entity,
                                    units::len(p[0], unit, 2),
                                    units::len(p[1], unit, 2),
                                    units::len(p[2], unit, 2),
                                    unit.suffix()
                                ),
                                false,
                            ),
                            None => (format!("Vertex {} · {} · missing", g.entity, short_id(id)), true),
                        }
                    }
                };
                if !missing && let Some(k) = stitched.get(gi).copied().flatten() {
                    label.push_str(&format!(" · → ring {}", k + 1));
                }
                GeoChip {
                    sel: g.clone(),
                    label,
                    missing,
                }
            })
            .collect()
    }

    /// Refetch the Tool Library pane's draft after document changes. The
    /// name decides the mode: it resolves to a tool entity (mirror it,
    /// `exists = true` - even after an undo brought it back), to nothing
    /// (keep the typed numbers with `exists = false`, so a deleted tool's
    /// page doubles as re-add), or is empty (the untouched New page).
    pub fn sync_tool_ui(&mut self) {
        if self.tool_synced == (self.tool_ui.name.clone(), self.doc_version) {
            return;
        }
        let ident = self.tool_ui.name.clone();
        self.tool_synced = (ident.clone(), self.doc_version);
        if ident.is_empty() {
            return;
        }
        match model::find_by_name(&self.world, &ident).and_then(|e| model::tool_of(&self.world, &e))
        {
            Some(tool) => {
                self.tool_ui = ToolUi::from_tool(&ident, &tool, true);
                // #221: the number lives beside the geometry, on ToolDef.
                self.tool_ui.number = model::find_by_name(&self.world, &ident)
                    .and_then(|e| model::tool_number_of(&self.world, &e))
                    .unwrap_or(0);
            }
            None => self.tool_ui.exists = false,
        }
    }

    /// Name of the selected entity if it is an operation (single selection
    /// in practice; the first one wins).
    #[must_use]
    pub fn selected_operation(&self) -> Option<String> {
        model::selected_names(&self.world).into_iter().find(|name| {
            model::find_by_name(&self.world, name)
                .is_some_and(|e| model::is_operation(&self.world, &e))
        })
    }

    /// Dismiss the current properties page without changing the document.
    /// Clear selection so pane_target cannot reopen a selected operation.
    pub(crate) fn close_property_editor(&mut self) {
        self.ui_target = UiTarget::None;
        self.pending.push_back(CommandKind::Select(None));
        self.op_ui.name.clear();
        self.op_synced = (String::new(), self.doc_version);
        self.boundary_pick = None;
        self.boundary_view.shown = false;
        self.rename_draft = None;
    }

    pub(crate) fn close_operation_editor(&mut self, name: &str) {
        self.close_property_editor();
        self.status = format!("closed operation {name}");
    }

    pub(crate) fn close_tool_editor_window(&mut self) {
        self.tool_editor = None;
        self.tool_synced = (String::new(), self.doc_version);
        self.tool_body_ui = ToolBodyUi::default();
        self.tool_profile_ui = ToolProfileUi::default();
        self.tool_cutter_ui = ToolCutterUi::default();
    }

    pub(crate) fn copy_operation(&mut self, name: &str) {
        let Some(entity) = model::find_by_name(&self.world, name) else {
            self.status = format!("copy failed: operation '{name}' was not found");
            return;
        };
        let Some(operation) = model::operation_of(&self.world, &entity) else {
            self.status = format!("copy failed: '{name}' is not an operation");
            return;
        };
        let label = model::operation_label(&self.world, name, &entity);
        self.operation_clipboard = Some(OperationClipboard { label: label.clone(), operation });
        self.status = format!("copied operation {label}");
    }

    pub(crate) fn paste_operation(&mut self) {
        let Some(clip) = self.operation_clipboard.clone() else {
            self.status = String::from("nothing to paste: copy an operation first");
            return;
        };
        let base = format!("{} Copy", clip.label);
        let mut suffix = 1usize;
        let name = loop {
            let candidate = if suffix == 1 { base.clone() } else { format!("{base} {suffix}") };
            let taken = model::find_by_name(&self.world, &candidate).is_some()
                || camcraft_app::setup::operation_order(&self.world)
                    .into_iter()
                    .any(|(key, entity)| model::operation_label(&self.world, &key, &entity) == candidate);
            if !taken {
                break candidate;
            }
            let Some(next) = suffix.checked_add(1) else {
                self.status = String::from("paste failed: too many copies of this operation");
                return;
            };
            suffix = next;
        };
        self.pending.push_back(CommandKind::PasteOperation { name, operation: clip.operation });
    }

    /// Names of the tool entities, in document order.
    #[must_use]
    pub fn tool_names(&self) -> Vec<String> {
        model::named_entities(&self.world)
            .into_iter()
            .filter(|(_, e)| model::is_tool(&self.world, e))
            .map(|(name, _)| name)
            .collect()
    }

    /// Flip one name's visibility in a browser eye list.
    pub(crate) fn toggle_visibility(list: &mut Vec<String>, name: &str) {
        match list.iter().position(|n| n == name) {
            Some(i) => drop(list.remove(i)),
            None => list.push(name.to_string()),
        }
    }

    #[must_use]
    pub(crate) fn hidden_in_view(&self, name: &str) -> bool {
        self.hidden_models.iter().any(|n| n == name)
            || self.hidden_stocks.iter().any(|n| n == name)
    }

    #[must_use]
    pub(crate) fn active_stock_hidden(&self) -> bool {
        let name = camcraft_app::setup::stock_entity_name(
            &self.world,
            camcraft_app::setup::active(&self.world).as_deref(),
        );
        self.hidden_stocks.iter().any(|n| n == &name)
    }

    /// Show every cached operation path, including paths hidden by their
    /// browser eyes. This only changes viewport state.
    pub(crate) fn show_all_toolpaths(&mut self) {
        self.hidden_paths.clear();
        self.show_all_paths = true;
    }

    /// Hide every operation path, including the selected operation's path.
    /// Keeping the names in the eye list makes each row reflect the result.
    pub(crate) fn hide_all_toolpaths(&mut self) {
        self.hidden_paths = model::named_entities(&self.world)
            .into_iter()
            .filter(|(_, entity)| model::is_operation(&self.world, entity))
            .map(|(name, _)| name)
            .collect();
        self.show_all_paths = false;
    }

    pub(crate) fn toggle_all_toolpaths(&mut self) {
        if self.show_all_paths {
            self.hide_all_toolpaths();
        } else {
            self.show_all_toolpaths();
        }
    }

    /// Run every queued action, then poll the import worker once. Safe to
    /// call every frame; never blocks.
    pub fn drain_actions(&mut self) {
        while let Some(action) = self.pending.pop_front() {
            self.run(action);
        }
        // #183: an agent's live calls, after the user's own clicks.
        crate::mcp_host::pump(self);
        self.poll_import();
        // A refusal that no longer describes the page retires, with its
        // status-strip mirror (crate::feedback).
        self.retire_stale_feedback();
        // #406: the incoming stock's sheet follows the open setup page.
        crate::incoming_view::refresh(self);
        // The editor window's ghost law (M1.4), mirroring `pane_target`:
        // an open editor never keeps pointing at a name the document lost
        // under it - deleted, undone, or swapped by New/Open all land here.
        // The blank form creates rather than points, so it survives.
        if let Some(name) = &self.tool_editor
            && !name.is_empty()
            && !model::find_by_name(&self.world, name)
                .is_some_and(|e| model::is_tool(&self.world, &e))
        {
            self.tool_editor = None;
        }
    }

    /// Drive the queue and the import worker to completion, blocking. For
    /// tests and headless runs only - the interactive loop never calls this.
    pub fn settle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            self.drain_actions();
            if self.pending.is_empty() && self.import_rx.is_none() && self.import_queue.is_empty() {
                return;
            }
            if Instant::now() >= deadline {
                self.abandon_imports();
                self.status = String::from("import timed out");
                diag_warn!("settle: queued work did not finish before the deadline");
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    pub(crate) fn run(&mut self, action: CommandKind) {
        // Whatever ran, the banner shows only the newest outcome: a later
        // command (even a successful one) retires an old refusal.
        self.feedback = None;
        self.feedback_ctx = None;
        self.feedback_at = None;
        self.feedback_ghost = None;
        // #107: New, Open and Quit over unsaved changes wait for the
        // prompt's answer instead of discarding the document.
        let Some(action) = self.hold_if_unsaved(action) else {
            return;
        };
        match action {
            CommandKind::OpenModelDialog => {
                if let Some(path) = pick_mesh_file() {
                    self.import_path(path);
                }
            }
            CommandKind::ImportFile(path) => self.import_path(path),
            CommandKind::NewDocument => {
                self.abandon_imports();
                self.project_path = None;
                self.hidden_stocks.clear();
                self.hidden_offsets.clear();
                model::clear_named(&self.world);
                // Units belong to the document, and clear_named cannot
                // reach a singleton: reset the lens so a fresh blank is
                // millimeters, never the last document's inches. The
                // machine (M2.3 C) is the same furniture - a blank never
                // inherits the last document's limits.
                units::set(&self.world, Unit::default());
                machine::set(&self.world, "", None);
                // The post (M3.3 A) is the same furniture and sat outside
                // this reset: a blank used to inherit the last document's
                // script. The theme is NOT reset: it is the user's
                // preference, not the document's (#877).
                posting::post_set(&self.world, &PostRef::Builtin);
                self.history.clear();
                // Frozen evidence of the old document retires with it.
                self.post_view = None;
                self.post_refusal = None;
                self.program_sim = None;
                self.field_view = None;
                self.field_play = None;
                self.stock_mesh.clear();
                self.machine_editor = None;
                self.rename_draft = None;
                self.dragging_op = None;
                self.doc_version += 1;
                self.status = String::from("new document");
            }
            CommandKind::OpenProjectDialog => {
                if let Some(path) = pick_project_file() {
                    self.open_project_path(path);
                }
            }
            CommandKind::OpenProjectFile(path) => self.open_project_path(path),
            CommandKind::SaveProjectDialog => {
                if let Some(path) = self.project_path.clone().or_else(|| save_project_dialog(None)) {
                    self.save_project_path(path);
                }
            }
            CommandKind::SaveProjectAsDialog => {
                if let Some(path) = save_project_dialog(self.project_path.as_deref()) {
                    self.save_project_path(path);
                }
            }
            CommandKind::SaveProjectFile(path) => self.save_project_path(path),
            CommandKind::OpenRecent(kind, path) => self.open_recent(kind, path),
            CommandKind::ClearRecent(kind) => self.clear_recent(kind),
            CommandKind::PostProgramDialog => self.post_program_dialog(),
            CommandKind::PostProgramFile(path) => self.post_program_path(path),
            CommandKind::PostProgramFolder(dir) => self.post_program_folder(dir),
            CommandKind::Undo => {
                self.status = if self.history.undo(&self.world) {
                    self.doc_version += 1;
                    String::from("undo")
                } else {
                    String::from("nothing to undo")
                };
            }
            CommandKind::Redo => {
                self.status = if self.history.redo(&self.world) {
                    self.doc_version += 1;
                    String::from("redo")
                } else {
                    String::from("nothing to redo")
                };
            }
            CommandKind::DeleteSelected => {
                let names = model::selected_names(&self.world);
                let mut done = 0usize;
                for name in names {
                    match DeleteEntity::new(&self.world, &name) {
                        Ok(command) => {
                            self.history.execute(&self.world, command);
                            done += 1;
                        }
                        Err(err) => self.status = format!("delete failed: {err}"),
                    }
                }
                if done > 0 {
                    self.doc_version += 1;
                    self.status = format!("deleted {done} model(s)");
                }
            }
            CommandKind::Select(name) => {
                History::execute_ephemeral(
                    &self.world,
                    SelectEntity::new(&self.world, name.as_deref()),
                );
                self.selection_version = self.selection_version.wrapping_add(1);
            }
            CommandKind::FrameAll => self.frame_all(),
            CommandKind::Quit => self.request_quit = true,
            CommandKind::ResolveUnsaved(choice) => self.resolve_unsaved(choice),
            CommandKind::DeleteByName(name) => match DeleteEntity::new(&self.world, &name) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("deleted {name}");
                }
                Err(err) => self.refuse(ErrorScope::Setup, format!("delete failed: {err}")),
            },
            CommandKind::SetStock {
                shape,
                pos,
                yaw_deg,
            } => {
                let location = Location {
                    pos: Point3::new(pos[0], pos[1], pos[2]),
                    rot: quat_from_yaw(yaw_deg.to_radians()),
                };
                match SetStock::new(&self.world, shape, location) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = String::from("stock set");
                    }
                    Err(err) => self.refuse(ErrorScope::Setup, format!("stock not set: {err}")),
                }
            }
            CommandKind::SetRawStockOnly(on) => {
                match camcraft_app::commands::SetStockSourceOnly::new(&self.world, on) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = String::from(if on {
                            "the stock's source model is raw stock only"
                        } else {
                            "the stock's source model is judged as the part"
                        });
                    }
                    Err(err) => self.refuse(ErrorScope::Setup, format!("not changed: {err}")),
                }
            }
            CommandKind::SetUnits(next) => match SetUnits::new(&self.world, next) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("units: {}", next.label());
                }
                Err(err) => self.refuse(ErrorScope::Setup, format!("units not set: {err}")),
            },
            CommandKind::LoadMachineDialog => {
                if let Some(path) = pick_machine_file() {
                    self.load_machine_path(path);
                }
            }
            CommandKind::LoadMachineFile(path) => self.load_machine_path(path),
            // #343: the machine verbs land on the active setup's own
            // machine when it has one (`setup_mount`).
            CommandKind::ClearMachine => self.mount_clear(),
            CommandKind::LoadBuiltinMachine(name) => self.mount_builtin(&name),
            CommandKind::SetupOwnMachine { setup, own } => self.own_machine(&setup, own),
            CommandKind::SetupOwnPost { setup, own } => self.own_post(&setup, own),
            CommandKind::SetupOwnPostOptions { setup, own } => self.own_post_options(&setup, own),
            CommandKind::SetSetupWorkholdingRpm { setup, max_rpm } => self.set_workholding_rpm(&setup, max_rpm),
            CommandKind::EditMachineDialog => {
                let (path, m) = machine::of(&self.world);
                match m {
                    Some(m) => {
                        self.machine_editor =
                            Some(crate::egui_app::machine_editor::MachineEditor {
                                path,
                                machine: m,
                            });
                    }
                    None => self.refuse(
                        ErrorScope::Setup,
                        String::from("no machine to edit - load one first"),
                    ),
                }
            }
            CommandKind::EditMachineApply { path, machine } => self.mount_edited(path, *machine),
            // #384: the Machines pane's "Use for this setup" - routed
            // like every machine verb (`setup_mount`).
            CommandKind::UseCatalogueMachine(source) => self.mount_catalogue(&source),
            CommandKind::SelectPostBuiltin => self.select_post(PostRef::Builtin),
            CommandKind::SelectPostDialog => {
                if let Some(path) = pick_post_file() {
                    self.select_post(PostRef::File(path.display().to_string()));
                }
            }
            CommandKind::SelectPostFile(path) => {
                self.select_post(PostRef::File(path.display().to_string()));
            }
            CommandKind::SelectPostNamed(name) => self.select_post(PostRef::Named(name)),
            // #389: routed to the active setup's own options when it has them.
            CommandKind::SetPostOptions(next) => {
                self.route_post_options(next, "post options set", "post options not set", ErrorScope::Setup);
            }
            CommandKind::SelectTheme(next) => self.select_theme(next),
            CommandKind::SelectColorVision(next) => self.select_color_vision(next),
            CommandKind::SelectThemeDialog => {
                if let Some(path) = pick_theme_file() {
                    self.select_theme(ThemeRef::File(path.display().to_string()));
                }
            }
            CommandKind::StockFit {
                margin,
                shape: radio,
            } => {
                if radio == 2 {
                    // From-model stock IS its source's shape; there is
                    // nothing to wrap. Refuse visibly, never guess.
                    self.refuse(
                        ErrorScope::Setup,
                        String::from(
                            "from-model stock follows its source: choose Box or Cylinder to fit",
                        ),
                    );
                    return;
                }
                // #326: the parts as the active setup holds them - the
                // stock is square to the setup frame.
                let fit_setup = camcraft_app::setup::active(&self.world);
                let Some(bounds) = camcraft_app::setup_align::solid_aabb_in(&self.world, fit_setup.as_deref()) else {
                    self.refuse(
                        ErrorScope::Setup,
                        String::from("nothing to fit: no models in the document"),
                    );
                    return;
                };
                let fitted = if radio == 1 {
                    // #230: snug about the part - the models' largest
                    // radius from the fit's axis plus the margin, not the
                    // footprint's diagonal (1.41 R for a round part).
                    let c = [(bounds.min.x + bounds.max.x) * 0.5, (bounds.min.y + bounds.max.y) * 0.5];
                    camcraft_app::setup_align::solid_radius_about_in(&self.world, fit_setup.as_deref(), c)
                        .and_then(|r| stock::fit_cylinder_about(&bounds, r, margin))
                } else {
                    stock::fit_box(&bounds, margin)
                };
                self.apply_stock_fit(fitted);
            }
            CommandKind::StockFitSides { sides } => {
                if !sides.iter().all(|s| s.is_finite() && *s >= 0.0) {
                    self.refuse(
                        ErrorScope::Setup,
                        String::from("stock fit refused: every side's allowance must be zero or more"),
                    );
                    return;
                }
                // #326: the parts as the active setup holds them - the
                // stock is square to the setup frame.
                let fit_setup = camcraft_app::setup::active(&self.world);
                let Some(bounds) = camcraft_app::setup_align::solid_aabb_in(&self.world, fit_setup.as_deref()) else {
                    self.refuse(
                        ErrorScope::Setup,
                        String::from("nothing to fit: no models in the document"),
                    );
                    return;
                };
                self.apply_stock_fit(stock::fit_box_sides(&bounds, sides));
            }
            CommandKind::SetWcs {
                pos,
                roll_deg,
                pitch_deg,
                yaw_deg,
            } => {
                // The axis-angle builders quietly substitute identity for
                // non-finite angles; refuse that here rather than silently
                // dropping a NaN rotation (Prime Directive).
                if !roll_deg.is_finite() || !pitch_deg.is_finite() || !yaw_deg.is_finite() {
                    self.refuse(
                        ErrorScope::Setup,
                        "WCS not set: rotation angles must be finite".to_owned(),
                    );
                    return;
                }
                let location = Location {
                    pos: Point3::new(pos[0], pos[1], pos[2]),
                    rot: quat_from_euler(
                        roll_deg.to_radians(),
                        pitch_deg.to_radians(),
                        yaw_deg.to_radians(),
                    ),
                };
                match SetWcsOrigin::new(&self.world, location) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.wcs_pick = None; // #701: a landed frame ends the role draft
                        let active = camcraft_app::setup::active(&self.world);
                        self.status =
                            format!("{} origin set", camcraft_app::setup::wcs_label(&self.world, active.as_deref()));
                    }
                    Err(err) => self.refuse(ErrorScope::Setup, format!("WCS not set: {err}")),
                }
            }
            // #828: a new tool starts with the machine's default holder,
            // named in the status line (or why it was left off).
            CommandKind::AddTool { name, tool } => match AddTool::with_holder(&self.world, &name, tool, HolderPick::MachineDefault) {
                Ok(command) => {
                    let note = command.note();
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = match note {
                        Some(n) => format!("tool {name} added - {n}"),
                        None => format!("tool {name} added"),
                    };
                }
                Err(err) => self.refuse(ErrorScope::Tool, format!("tool not added: {err}")),
            },
            CommandKind::SetToolNumber { name, number } => {
                match SetToolNumber::new(&self.world, &name, number) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = match number {
                            Some(n) => format!("tool {name} is now T{n}"),
                            None => format!("tool {name} posts as its list position"),
                        };
                    }
                    Err(err) => self.refuse(ErrorScope::Tool, format!("tool number not set: {err}")),
                }
            }
            CommandKind::SetToolBody { name, body } => self.run_set_tool_body(&name, body),
            CommandKind::SetToolProfile { name, points } => self.run_set_tool_profile(&name, points),
            CommandKind::SetToolCutter { name, points } => self.run_set_tool_cutter(&name, points),
            CommandKind::ReshapeTool { name, tool, points } => self.run_reshape_tool(&name, tool, points),
            CommandKind::SetToolHolder { name, entry, gauge, projection } => {
                self.run_set_tool_holder(&name, &entry, gauge, projection)
            }
            CommandKind::ChangeTool { name, tool } => {
                match ChangeTool::new(&self.world, &name, tool) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("tool {name} updated");
                    }
                    Err(err) => self.refuse(ErrorScope::Tool, format!("tool not updated: {err}")),
                }
            }
            CommandKind::SetWorkMaterial(m) => match SetWorkMaterial::new(&self.world, m) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("workpiece material: {}", m.map_or("not stated", |m| m.label()));
                }
                Err(err) => self.refuse(ErrorScope::Operation, format!("material not set: {err}")),
            },
            CommandKind::ApplyCuttingData(name) => match ApplyCuttingData::new(&self.world, &name) {
                Ok(command) => {
                    let s = command.suggestion();
                    let u = self.units();
                    self.status = format!(
                        "{name}: suggestion applied - {:.0} rpm, {} {}",
                        s.rpm,
                        units::feed(s.feed, u),
                        u.feed_suffix()
                    );
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                }
                Err(err) => self.refuse(ErrorScope::Operation, format!("suggestion not applied: {err}")),
            },
            CommandKind::DeleteTool(name) => match DeleteTool::new(&self.world, &name) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("deleted tool {name}");
                }
                Err(err) => self.refuse(ErrorScope::Tool, format!("tool not deleted: {err}")),
            },
            CommandKind::CreateOperation {
                name,
                tool,
                src,
                params,
            } => match CreateOperation::new(&self.world, &name, &tool, src, params) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("operation {name} created");
                }
                Err(err) => {
                    self.refuse(
                        ErrorScope::Operation,
                        format!("operation not created: {err}"),
                    );
                }
            },
            CommandKind::PasteOperation { name, operation } => {
                match PasteOperation::new(&self.world, &name, operation) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        History::execute_ephemeral(&self.world, SelectEntity::new(&self.world, Some(name.as_str())));
                        self.selection_version = self.selection_version.wrapping_add(1);
                        self.ui_target = UiTarget::Operation(name.clone());
                        self.doc_version += 1;
                        self.status = format!("pasted operation {name}; generate its toolpath when ready");
                    }
                    Err(err) => self.refuse(ErrorScope::Operation, format!("paste failed: {err}")),
                }
            }
            CommandKind::EditOperationParams { name, params } => {
                match EditOperationParams::new(&self.world, &name, params) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("parameters updated for {name}");
                    }
                    Err(err) => {
                        self.refuse(
                            ErrorScope::Operation,
                            format!("parameters not updated: {err}"),
                        );
                    }
                }
            }
            CommandKind::EditOperationBoundary { name, params, area } => {
                match EditOperationParams::with_area(&self.world, &name, params, area) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("parameters and boundary updated for {name}");
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Operation, format!("parameters not updated: {err}"));
                    }
                }
            }
            CommandKind::EditOperationCoolant { name, coolant } => {
                match EditOperationParams::with_coolant(&self.world, &name, coolant) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("coolant set to {} for {name}", coolant.label());
                    }
                    Err(err) => self.refuse(ErrorScope::Operation, format!("coolant not changed: {err}")),
                }
            }
            CommandKind::EditOperationAfter { name, after } => {
                match EditOperationParams::with_after(&self.world, &name, after) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("after {name}: {}", after.label());
                    }
                    Err(err) => self.refuse(ErrorScope::Operation, format!("after-action not changed: {err}")),
                }
            }
            CommandKind::SuppressOperation { name, suppressed } => {
                match EditOperationParams::with_suppressed(&self.world, &name, suppressed) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = if suppressed {
                            format!("{name} suppressed: left out of the posted program")
                        } else {
                            format!("{name} back in the posted program")
                        };
                    }
                    Err(err) => self.refuse(ErrorScope::Operation, format!("suppression not changed: {err}")),
                }
            }
            CommandKind::SetPerOpOutput(per_op) => self.set_per_op_output(per_op),
            CommandKind::ProgramEntry(verb) => self.program_entry_verb(verb),
            CommandKind::SetSkipStops(skip) => {
                let next = camcraft_app::schema::PostOptions { skip_stops: skip, ..posting::post_options(&self.world) };
                let done = if skip { "post skips every pause and home" } else { "post writes the operations' pauses and homes" };
                self.route_post_options(next, done, "post options not changed", ErrorScope::Operation);
            }
            CommandKind::ChangeOperationKind { name, kind } => {
                match ChangeOperationKind::new(&self.world, &name, kind) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("operation {name} retyped");
                    }
                    Err(err) => {
                        self.refuse(
                            ErrorScope::Operation,
                            format!("operation kind not changed: {err}"),
                        );
                    }
                }
            }
            CommandKind::EditOperationLinks { name, tool, src } => {
                let links = EditOperationLinks::new(&self.world, &name, tool.as_deref(), src);
                match links {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("links updated for {name}");
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Operation, format!("links not updated: {err}"));
                    }
                }
            }
            CommandKind::SetOperationGeometry { name, items } => {
                match SetOperationGeometry::new(&self.world, &name, items) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("geometry selection updated for {name}");
                    }
                    Err(err) => {
                        self.refuse(
                            ErrorScope::Operation,
                            format!("geometry selection not updated: {err}"),
                        );
                    }
                }
            }
            CommandKind::DeleteOperation(name) => match DeleteOperation::new(&self.world, &name) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("deleted operation {name}");
                }
                Err(err) => {
                    self.refuse(
                        ErrorScope::Operation,
                        format!("operation not deleted: {err}"),
                    );
                }
            },
            CommandKind::RenameOperation { name, label } => match RenameOperation::new(&self.world, &name, &label) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("operation renamed to {label}");
                }
                Err(err) => self.refuse(ErrorScope::Operation, format!("rename failed: {err}")),
            },
            CommandKind::ReorderOperation { name, target, after } => match ReorderOperation::new(&self.world, &name, &target, after) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("operation {name} moved");
                }
                Err(err) => self.refuse(ErrorScope::Operation, format!("reorder failed: {err}")),
            },
            CommandKind::GenerateOperation(name) => {
                // Generation happens in the constructor (a refusal writes
                // nothing); the status line reports what was produced.
                match GenerateOperation::new(&self.world, &name) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        let cache = model::find_by_name(&self.world, &name)
                            .and_then(|e| model::toolpath_of(&self.world, &e));
                        // The feed in the summary reads in the document's
                        // unit (M1.1); the path itself is mm law, always.
                        let unit = self.units();
                        self.status = match cache {
                            Some(tp) => tp.path.status_summary(&name, tp.warnings.len(), unit),
                            None => format!("generated {name}"),
                        };
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Operation, format!("generate failed: {err}"));
                        // #261 part 2: where the generator refused, if it said.
                        self.feedback_at = err.location().map(|p| [p.x, p.y, p.z]);
                        // Part 3: the moves it had built, drawn as a ghost.
                        self.feedback_ghost = err.ghost().cloned();
                    }
                }
            }
            CommandKind::ClearToolpath(name) => match ClearToolpath::new(&self.world, &name) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("cleared the toolpath of {name} - Ctrl+Z restores it");
                }
                Err(err) => {
                    self.refuse(ErrorScope::Operation, format!("toolpath not cleared: {err}"))
                }
            },
            CommandKind::RegenerateAll => {
                // Construction decides everything (validate early: each
                // stale op's generator runs before anything is written),
                // so an empty batch touches no history at all.
                let command = RegenerateAll::new(&self.world);
                let skipped: Vec<String> = command
                    .skipped()
                    .iter()
                    .map(|(name, err)| format!("{name}: {err}"))
                    .collect();
                if command.is_empty() {
                    match command.skipped().first() {
                        Some((name, err)) => self.refuse(
                            ErrorScope::Operation,
                            format!("nothing regenerated - {name}: {err}"),
                        ),
                        None => self.status = "nothing is stale".to_owned(),
                    }
                } else {
                    let n = command.regenerated();
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = if skipped.is_empty() {
                        format!("regenerated {n} operation(s)")
                    } else {
                        format!(
                            "regenerated {n} operation(s); {} skipped - {}",
                            skipped.len(),
                            skipped.join("; ")
                        )
                    };
                }
            }
            CommandKind::CreateSetup { name, datum_from } => {
                match CreateSetup::new(&self.world, name.as_deref(), datum_from.as_deref()) {
                    Ok(command) => {
                        let made = command.name().to_owned();
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("setup {made} created and active");
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("setup not created: {err}"));
                    }
                }
            }
            CommandKind::DeleteSetup(name) => match DeleteSetup::new(&self.world, &name) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("deleted setup {name}");
                }
                Err(err) => {
                    self.refuse(ErrorScope::Setup, format!("setup not deleted: {err}"));
                }
            },
            CommandKind::ActivateSetup(name) => {
                match ActivateSetup::new(&self.world, name.as_deref()) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = match &name {
                            Some(n) => format!("setup {n} active"),
                            None => "active-setup cursor cleared".to_owned(),
                        };
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("setup not activated: {err}"));
                    }
                }
            }
            CommandKind::SetSetupWorkOffset { setup, slot } => {
                match SetSetupWorkOffset::new(&self.world, &setup, slot) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        let word = WCS_SLOTS.get(slot.unwrap_or(0)).copied().unwrap_or("G54");
                        self.status = format!("{setup} WCS posts as {word}");
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("work offset not changed: {err}"));
                    }
                }
            }
            CommandKind::AlignPart { setup, angles, pivot } => {
                match AlignPartToSetup::turn(&self.world, &setup, angles, pivot) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!(
                            "{setup} part alignment: {}",
                            crate::egui_app::align_text(camcraft_app::setup_align::stored(&self.world, &setup))
                        );
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("part alignment not changed: {err}"));
                    }
                }
            }
            CommandKind::SetSetupRelative { setup, to, angles } => {
                let built = match &to {
                    Some(parent) => SetSetupRelative::link(&self.world, &setup, parent, angles),
                    None => SetSetupRelative::unlink(&self.world, &setup),
                };
                match built {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = match camcraft_app::setup_relative::describe(&self.world, &setup) {
                            Some(words) => format!("{setup}: {words}"),
                            None => format!("{setup} is no longer placed relative to another setup"),
                        };
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("setup not re-placed: {err}"));
                    }
                }
            }
            CommandKind::SetSetupStockSource { setup, source } => {
                match SetSetupStockSource::new(&self.world, &setup, source) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        let words = camcraft_app::setup_stock::describe(&self.world, &setup).unwrap_or_default();
                        self.status = format!("{setup} stock: {words}");
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("stock source not changed: {err}"));
                    }
                }
            }
            CommandKind::LayPartOnPlane { setup, picks, lay } => {
                match AlignPartToSetup::lay_on_plane(&self.world, &setup, &picks, lay) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.plane_pick = None;
                        self.status = format!(
                            "{setup} part laid on {}: {}",
                            camcraft_app::pick::plane_label(lay.plane),
                            crate::egui_app::align_text(camcraft_app::setup_align::stored(&self.world, &setup))
                        );
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("part not laid on the plane: {err}"));
                    }
                }
            }
            CommandKind::LayPartOnPoints { setup, picks, lay } => {
                match AlignPartToSetup::lay_on_points(&self.world, &setup, &picks, lay) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.points_pick = None;
                        self.status = format!(
                            "{setup} part laid on {} through three points: {}",
                            camcraft_app::pick::plane_label(lay.lay.plane),
                            crate::egui_app::align_text(camcraft_app::setup_align::stored(&self.world, &setup))
                        );
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("part not laid on the three points: {err}"));
                    }
                }
            }
            CommandKind::TurnPartInPlane { setup, axes } => {
                match AlignPartToSetup::turn_in_plane(&self.world, &setup, &axes) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        // The picks stay (disarmed) so Flip re-applies
                        // onto what the setup now holds (#557 hand check).
                        if let Some(d) = self.axis_pick.as_mut().filter(|d| d.setup == setup) {
                            d.picking = None;
                            d.applied = true;
                        }
                        self.status = format!(
                            "{setup} part turned in the plane: {}",
                            crate::egui_app::align_text(camcraft_app::setup_align::stored(&self.world, &setup))
                        );
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("part not turned in the plane: {err}"));
                    }
                }
            }
            CommandKind::SetOperationSetup { name, setup } => {
                match SetOperationSetup::new(&self.world, &name, setup.as_deref()) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!(
                            "moved {name} to {}",
                            setup.as_deref().map_or_else(
                                || "the first setup".to_owned(),
                                |s| format!("'{s}'"),
                            )
                        );
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Operation, format!("setup not moved: {err}"));
                    }
                }
            }
            CommandKind::TagFixture { name, setup } => {
                match TagFixture::new(&self.world, &name, setup.as_deref()) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = match &setup {
                            Some(s) => format!("marked {name} as {s}'s fixture"),
                            None => format!("marked {name} as a fixture"),
                        };
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("fixture not marked: {err}"));
                    }
                }
            }
            CommandKind::UntagFixture(name) => match UntagFixture::new(&self.world, &name) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("unmarked {name} - the solid is a plain model again");
                }
                Err(err) => {
                    self.refuse(ErrorScope::Setup, format!("fixture not unmarked: {err}"));
                }
            },
            CommandKind::SetFixtureFixed { name, fixed } => {
                match SetFixtureFixed::new(&self.world, &name, fixed) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = if fixed {
                            format!("{name} is fixed - it does not turn with the part")
                        } else {
                            format!("{name} turns with the part")
                        };
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("fixture not changed: {err}"));
                    }
                }
            }
            CommandKind::MoveFixture { name, pos, yaw_deg } => {
                let location = Location {
                    pos: Point3::new(pos[0], pos[1], pos[2]),
                    rot: quat_from_yaw(yaw_deg.to_radians()),
                };
                match MoveFixture::new(&self.world, &name, location) {
                    Ok(command) => {
                        let warning = command.warning().map(str::to_owned);
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = with_warning(format!("moved fixture {name}"), warning);
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("fixture not moved: {err}"));
                    }
                }
            }
            CommandKind::Locate { spec, mover } => self.dispatch_locate(&spec, mover),
            CommandKind::DeleteFixture(name) => match DeleteFixture::new(&self.world, &name) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("deleted fixture {name}");
                }
                Err(err) => {
                    self.refuse(ErrorScope::Setup, format!("fixture not deleted: {err}"));
                }
            },
            CommandKind::MoveModel {
                name,
                pos,
                roll_deg,
                pitch_deg,
                yaw_deg,
            } => {
                // The `SetWcs` guard: the axis-angle builders quietly
                // substitute identity for non-finite angles, so refuse
                // here rather than silently drop a NaN rotation.
                if !roll_deg.is_finite() || !pitch_deg.is_finite() || !yaw_deg.is_finite() {
                    self.refuse(
                        ErrorScope::Setup,
                        "part not moved: rotation angles must be finite".to_string(),
                    );
                    return;
                }
                let location = Location {
                    pos: Point3::new(pos[0], pos[1], pos[2]),
                    rot: quat_from_euler(
                        roll_deg.to_radians(),
                        pitch_deg.to_radians(),
                        yaw_deg.to_radians(),
                    ),
                };
                match MoveModel::new(&self.world, &name, location) {
                    Ok(command) => {
                        let warning = command.warning().map(str::to_owned);
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.model_ui = ModelUi {
                            name: name.clone(),
                            x: pos[0],
                            y: pos[1],
                            z: pos[2],
                            roll_deg,
                            pitch_deg,
                            yaw_deg,
                        };
                        self.status = with_warning(format!("moved {name}"), warning);
                    }
                    Err(err) => self.refuse(ErrorScope::Setup, format!("part not moved: {err}")),
                }
            }
            CommandKind::RepairModel { name, weld_tol, max_hole_edges } => {
                match RepairModel::new(&self.world, &name, weld_tol, max_hole_edges) {
                    Ok(command) => {
                        let summary = command.summary();
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.repair_ui.last = Some((name.clone(), summary.clone()));
                        self.status = format!("repaired {name}: {summary}");
                    }
                    Err(err) => self.refuse(ErrorScope::Setup, format!("{name} not repaired: {err}")),
                }
            }
            CommandKind::ImportFixtureDialog => {
                if let Some(path) = pick_mesh_file() {
                    // Bind to the context the page shows: the active
                    // setup, or the implicit-first `None` of a document
                    // with no setups at all.
                    self.import_as_fixture = Some(camcraft_app::setup::active(&self.world));
                    self.import_path(path);
                }
            }
            CommandKind::FixtureLibrarySaveDialog(setup) => self.open_fixture_library_save(setup),
            CommandKind::FixtureLibraryInsertDialog(setup) => self.open_fixture_library_insert(setup),
            CommandKind::InsertFixtureFromLibrary { entry, setup, pos, rpy_deg } => {
                self.insert_fixture_from_library(&entry, setup, pos, rpy_deg);
            }
            CommandKind::BuiltinViseDialog(setup) => self.open_vise_dialog(setup),
            CommandKind::AccessoryDialog(setup) => self.open_accessory_dialog(setup),
            CommandKind::InsertAccessory { setup, params, placement } => {
                self.insert_accessory(setup, &params, placement);
            }
            CommandKind::SetAccessory { name, params } => self.set_accessory(&name, params),
            CommandKind::InsertBuiltinVise { setup, params, fit_stock, pos, yaw_deg } => {
                self.insert_builtin_vise(setup, &params, fit_stock, pos, yaw_deg);
            }
            CommandKind::SetVise { name, params, from_stock } => self.set_builtin_vise(&name, params, from_stock),
            CommandKind::FixturePlateDialog(setup) => self.open_plate_dialog(setup),
            CommandKind::InsertFixturePlate { setup, params, on_table, pos, yaw_deg } => {
                self.insert_fixture_plate(setup, &params, on_table, pos, yaw_deg);
            }
            CommandKind::PlaceOnPlate { fixture, plate, at, yaw_deg } => self.place_fixture_on_plate(&fixture, &plate, at, yaw_deg),
            CommandKind::DetectPlateGrid { name, clear } => self.detect_plate_grid(&name, clear),
            CommandKind::PlaceModVisePair { plate, fixed, adjustable, at, yaw_deg } => {
                self.place_mod_vise_pair(&plate, &fixed, &adjustable, &at, yaw_deg);
            }
            CommandKind::SetModJaw { name, params } => self.set_mod_jaw(&name, params),
            CommandKind::RefitModVise { adjustable } => self.refit_mod_vise(&adjustable),
            CommandKind::SetupSheetDialog => {
                if let Some(path) = save_sheet_dialog() {
                    self.setup_sheet_path(path);
                }
            }
            CommandKind::SetupSheetFile(path) => self.setup_sheet_path(path),
            CommandKind::ToolListExportDialog(setup) => self.tool_list_export_dialog(setup),
            CommandKind::ToolListExportFile { setup, path } => self.tool_list_export_path(setup.as_deref(), &path),
            CommandKind::ExportStockDialog => {
                if let Some(path) = save_stl_dialog() {
                    self.export_stock_path(path);
                }
            }
            CommandKind::ExportStockFile(path) => self.export_stock_path(path),
        }
    }

    #[must_use]
    pub fn has_selection(&self) -> bool {
        !model::selected_names(&self.world).is_empty()
    }

    /// Is there anything to simulate: an operation carrying a cached
    /// path. The walk mirrors the post gate's exactly
    /// (`posting::stream_records`: `named_entities` filtered by
    /// `is_operation`) - and display-hidden paths are included, by
    /// the same law that keeps them posted: hidden is display, not
    /// suppression, and the simulation rehearses the machine's
    /// program, not the picture.
    #[must_use]
    pub fn has_toolpaths(&self) -> bool {
        model::named_entities(&self.world)
            .into_iter()
            .filter(|(_, e)| model::is_operation(&self.world, e))
            .any(|(_, e)| model::has_toolpath(&self.world, &e))
    }

    /// Center the orbit camera on the document's bounding sphere.
    fn frame_all(&mut self) {
        let mut min = [f64::MAX; 3];
        let mut max = [f64::MIN; 3];
        let mut any = false;
        for (_, entity) in model::named_entities(&self.world) {
            let Some(tris) = model::facets_of(&self.world, &entity) else {
                continue;
            };
            let loc = camcraft_app::setup_align::view_pose(&self.world, &entity).unwrap_or_default(); // #326
            for t in tris.iter() {
                for p in [t.a, t.b, t.c] {
                    let p = transform_point(p, loc.pos, loc.rot);
                    for axis in 0..3 {
                        let v = [p.x, p.y, p.z][axis];
                        if v.is_finite() {
                            min[axis] = min[axis].min(v);
                            max[axis] = max[axis].max(v);
                            any = true;
                        }
                    }
                }
            }
        }
        // A work offset parked far from the parts is still "all": frame
        // their origins even though they carry no geometry.
        for (_, entity) in model::named_entities(&self.world) {
            if !model::is_wcs(&self.world, &entity) {
                continue;
            }
            let Some(loc) = model::transform_of(&self.world, &entity) else {
                continue;
            };
            for axis in 0..3 {
                let v = [loc.pos.x, loc.pos.y, loc.pos.z][axis];
                if v.is_finite() {
                    min[axis] = min[axis].min(v);
                    max[axis] = max[axis].max(v);
                    any = true;
                }
            }
        }
        if !any {
            self.cam = Camera {
                projection: self.cam.projection,
                ..Camera::default()
            };
            self.status = String::from("nothing to frame");
            return;
        }
        let center = std::array::from_fn(|i| (min[i] + max[i]) * 0.5);
        let half: [f64; 3] = std::array::from_fn(|i| (max[i] - min[i]) * 0.5);
        let radius = half
            .iter()
            .fold(0.0f64, |acc, r| acc.max(*r))
            .max(f64::EPSILON)
            * 3.0f64.sqrt();
        self.cam = Camera {
            projection: self.cam.projection,
            ..Camera::frame(center, radius)
        };
    }

    fn import_path(&mut self, path: PathBuf) {
        // Parsing happens at command construction, like everything else -
        // it just happens to be off-thread now, so a heavy STEP tessellates
        // while the window keeps painting.
        if self.import_rx.is_some() {
            self.status = format!("queued {} for import", file_label(&path));
            self.import_queue.push_back(path);
            return;
        }
        self.begin_import(path);
    }

    fn begin_import(&mut self, path: PathBuf) {
        self.status = format!("importing {}…", file_label(&path));
        self.import_file = Some(path.clone());
        self.import_started = Some(Instant::now());
        let (tx, rx) = mpsc::channel();
        let worker_path = path.clone();
        let budget = self.step_budget_for(&path);
        let spawned = std::thread::Builder::new()
            .name("camcraft-import".to_string())
            .spawn(move || {
                // No World here: pure file + geometry work, Send across.
                let result = ImportModel::within(&worker_path, budget);
                // A dropped rx means the document changed or we are closing;
                // the command is simply never applied.
                if tx.send(result).is_err() {
                    diag_warn!("import: result discarded, no receiver");
                }
            });
        match spawned {
            Ok(_) => self.import_rx = Some(rx),
            Err(err) => {
                // Thread exhaustion must not lose the import: run inline.
                diag_warn!("import: worker spawn failed ({err}); running inline");
                let result = ImportModel::within(&path, budget);
                self.finish_import(result);
            }
        }
    }

    /// Called on this thread when a worker result arrives (or inline
    /// fallback completes). Execution stays a normal history command.
    fn finish_import(&mut self, result: Result<ImportModel, CommandError>) {
        // #662: the file a landed import came from joins the recent models.
        let file = self.import_file.take();
        let secs = self
            .import_started
            .take()
            .map_or(0.0, |t| t.elapsed().as_secs_f64());
        match result {
            Ok(command) => {
                let name = command.name.clone();
                let timed = |verb: &str, note: &str| {
                    if secs >= 2.0 {
                        format!("{verb} {name}{note} in {secs:.1}s")
                    } else {
                        format!("{verb} {name}{note}")
                    }
                };
                // M3.1: a name that already carries a model is an UPDATE,
                // not a silently-dropped re-import. The decision lives here,
                // on the main thread, because only the world can say whether
                // the name exists; the worker stayed world-blind on purpose.
                // UpdateModel::new classifies further: setup furniture
                // under this name is refused as Invalid, never overwritten.
                // #339 slice 4: an import-as-fixture lands mesh and
                // marker as ONE undo unit (`ImportFixture`) - one Ctrl+Z
                // takes the whole import away.
                if let Some(binding) = self.import_as_fixture.take() {
                    match ImportFixture::new(&self.world, command, binding.as_deref()) {
                        Ok(cmd) => {
                            let verb = match cmd.update_summary() {
                                Some(summary) => timed("updated", &format!(" ({summary})")),
                                None => timed("imported", ""),
                            };
                            self.history.execute(&self.world, cmd);
                            self.doc_version += 1;
                            self.note_recent_model(file.as_deref());
                            self.status = match &binding {
                                Some(s) => format!("{verb}; marked as {s}'s fixture"),
                                None => format!("{verb}; marked as a fixture"),
                            };
                        }
                        Err(err) => self.refuse(ErrorScope::Setup, format!("fixture not imported: {err}")),
                    }
                    return;
                }
                if model::find_by_name(&self.world, &name).is_some() {
                    let (coarse, budget) = (command.coarse_mm, command.facet_budget);
                    match UpdateModel::new(&self.world, &name, command.mesh).map(|u| u.with_fidelity(coarse).with_budget(budget)) {
                        Ok(upd) => {
                            let summary = upd.changes().describe();
                            self.history.execute(&self.world, upd);
                            self.doc_version += 1;
                            self.note_recent_model(file.as_deref());
                            self.status = timed("updated", &format!(" ({summary})"));
                        }
                        Err(err) => self.status = format!("import refused: {err}"),
                    }
                } else {
                    // Fresh model: the spawn path, unchanged.
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.note_recent_model(file.as_deref());
                    self.status = timed("imported", "");
                }
                // #864: an import kept coarser than promised is said, not
                // only logged.
                if let Some(chord) = model::find_by_name(&self.world, &name)
                    .and_then(|e| camcraft_app::model_fidelity::chord_of(&self.world, &e))
                {
                    let said = camcraft_app::model_fidelity::summary(&self.world, chord);
                    self.status = format!("{} - {said}", self.status);
                }
            }
            Err(err) => {
                // A refused import-as-fixture must not tag the NEXT import.
                self.import_as_fixture = None;
                self.status = format!("import refused: {err}");
            }
        }
    }

    /// Non-blocking worker check; call once per frame. Also starts the next
    /// queued import when the worker is free.
    fn poll_import(&mut self) {
        // take() first so no borrow of self straddles the mutation arms.
        let outcome = match &self.import_rx {
            Some(rx) => match rx.try_recv() {
                Ok(result) => Some(Ok(result)),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(())),
            },
            None => None,
        };
        match outcome {
            Some(Ok(result)) => {
                self.import_rx = None;
                self.finish_import(result);
            }
            Some(Err(())) => {
                // Thread died without sending (a panic in the parser): the
                // Prime Directive says surface it, never hang waiting.
                self.import_rx = None;
                self.import_file = None;
                self.import_started = None;
                self.status = String::from("import failed: worker lost");
                diag_warn!("import: worker thread exited without a result");
            }
            None => {}
        }
        // Still running: keep the elapsed readout honest in the status bar.
        if self.import_rx.is_some()
            && let Some(t0) = self.import_started
        {
            let secs = t0.elapsed().as_secs_f64();
            if secs >= 1.0 {
                let label = self
                    .import_file
                    .as_deref()
                    .map_or_else(|| "model".to_string(), file_label);
                self.status = format!("importing {label}… {secs:.0}s");
            }
        }
        if self.import_rx.is_none()
            && let Some(next) = self.import_queue.pop_front()
        {
            self.begin_import(next);
        }
    }

    /// The shell polls a running import on a timer while otherwise idle.
    pub(crate) fn import_in_progress(&self) -> bool {
        self.import_rx.is_some() || !self.import_queue.is_empty()
    }

    /// Drop any live/queued imports because the document is being replaced.
    /// A worker still running finishes in the background and discards itself.
    fn abandon_imports(&mut self) {
        if self.import_rx.take().is_some() {
            diag_warn!("import: document changed; pending import discarded");
        }
        self.import_queue.clear();
        self.import_file = None;
        self.import_started = None;
        self.import_as_fixture = None;
    }

    /// Mount a `.mch` machine definition on the document (M2.3 C). The
    /// parse lives in `LoadMachine::from_file` - command-construction
    /// validation, so a bad file is a visible refusal, never a
    /// half-written setup.
    fn load_machine_path(&mut self, path: PathBuf) {
        self.mount_file(&path);
    }

    /// Choose the post script (M3.3 A) - routed to the active setup's
    /// own post when it has one (#343, `setup_mount`).
    fn select_post(&mut self, next: PostRef) {
        self.choose_post(next);
    }

    /// Install the user's theme (M6.1 C, #877): the world singleton is
    /// the truth, and this frame's job is to say whether it changed
    /// since the last - a pick or the startup preferences read can
    /// move it between frames, and none of them reaches into
    /// egui from app (layering; the command-law's other half). A
    /// refused reference - the file vanished, a corrupt parse, a
    /// stranger name - names itself in the status line ONCE per change
    /// and leaves the live tokens untouched: the screen never loses
    /// its look to a missing accessory mid-session, and never
    /// half-applies.
    pub fn sync_theme(&mut self, ctx: &egui::Context) {
        let next = theme_of(&self.world);
        if next == self.theme_key {
            return;
        }
        self.theme_key = next.clone();
        match theme_files::resolve(&mut self.theme_cache, &next) {
            Ok((tokens, notes)) => {
                theme::set_current(&tokens);
                theme::apply(ctx);
                if !notes.is_empty() {
                    // Reported, never silently dropped (B's law, kept
                    // through C's swap path): the first note rides the
                    // status line beside the theme's name.
                    self.status = format!("theme: {} - {}", theme_label(&self.world), notes[0]);
                }
            }
            Err(why) => {
                self.status = format!("theme refused: {why} (keeping the current look)");
            }
        }
    }

    /// The switcher's rows (M6.1 C): the two built-ins, the discovered
    /// files (label: the descriptor name when it parses, the file stem
    /// when it does not - a listed file is pickable enough to name why
    /// not), and Browse for anywhere else. Labels are screen furniture;
    /// what the document stores is the reference each row queues.
    #[must_use]
    pub fn theme_options(&self) -> Vec<(String, CommandKind)> {
        let mut rows: Vec<(String, CommandKind)> = vec![
            ("Dark".into(), CommandKind::SelectTheme(ThemeRef::Dark)),
            (
                "Light".into(),
                CommandKind::SelectTheme(ThemeRef::Named("light".into())),
            ),
        ];
        for path in &self.theme_paths {
            let label = match self.theme_cache.peek_label(path) {
                Some(name) => name,
                None => path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_owned(),
            };
            rows.push((
                label,
                CommandKind::SelectTheme(ThemeRef::File(path.display().to_string())),
            ));
        }
        rows.push(("Browse...".into(), CommandKind::SelectThemeDialog));
        rows
    }

    /// The switcher's selected text: which row the document matches.
    /// A file not in the discovered list still shows its path - the
    /// `post_label` law, one truth for what the screen wears.
    #[must_use]
    pub fn theme_selection_text(&self) -> String {
        match theme_of(&self.world) {
            ThemeRef::Dark => "Dark".to_owned(),
            ThemeRef::Named(name) => titled(&name),
            ThemeRef::File(path) => {
                for p in &self.theme_paths {
                    if p.display().to_string() == path {
                        return self.theme_cache.peek_label(p).unwrap_or_default();
                    }
                }
                path
            }
        }
    }

    fn open_project_path(&mut self, path: PathBuf) {
        self.abandon_imports();
        self.project_path = None;
        self.hidden_stocks.clear();
        self.hidden_offsets.clear();
        model::clear_named(&self.world);
        self.history.clear();
        self.post_view = None; // the old document's program is not this one's
        self.post_refusal = None; // nor its refused post
        self.program_sim = None; // and neither is its stock verdict
        self.field_view = None; // nor its traced stock
        self.field_play = None; // nor its playback
        self.stock_mesh.clear(); // nor the mesh extracted from it
        self.machine_editor = None; // nor a draft over its machine's numbers
        self.rename_draft = None;
        self.dragging_op = None;
        match project::load_noted(&self.world, &path) {
            Ok(notes) => {
                self.project_path = Some(path.clone());
                self.note_recent(camcraft_app::recent::RecentKind::Project, &path);
                // The loaded document is the new clean point (#107).
                self.history.mark_clean();
                self.doc_version += 1;
                // #342: what a migration changed is said, never silent.
                self.status = if notes.is_empty() {
                    format!("opened {}", path.display())
                } else {
                    format!("opened {} - {}", path.display(), notes.join("; "))
                };
            }
            Err(err) => self.status = format!("open failed: {err}"),
        }
    }

    pub(crate) fn save_project_path(&mut self, path: PathBuf) {
        match project::save(&self.world, &path) {
            Ok(()) => {
                // What is on disk now matches the history position (#107).
                self.history.mark_clean();
                self.status = format!("saved {}", path.display());
                self.note_recent(camcraft_app::recent::RecentKind::Project, &path);
                self.project_path = Some(path);
            }
            Err(err) => self.status = format!("save failed: {err}"),
        }
    }

    #[must_use]
    pub fn window_title(&self) -> String {
        let filename = self.project_path.as_ref().and_then(|path| path.file_name());
        // A leading '*' while the document has unsaved changes (#107).
        let mark = if self.history.is_dirty() { "*" } else { "" };
        match filename {
            Some(name) => format!("{mark}CamCraft - {}", name.to_string_lossy()),
            None => format!("{mark}CamCraft - Untitled"),
        }
    }

    /// The file law's third effect (M1.7b): post, or be refused by name.
    /// Like save/open, the outcome is a status line, not a pane error -
    /// and the gate writes nothing on refusal (posting's own law), so a
    /// refused post leaves both the document and any previous program on
    /// disk exactly as they were.
    fn post_program_path(&mut self, path: PathBuf) {
        // #388: setups on different machines or posts write one file each.
        match posting::post_mounts(&self.world, &path) {
            Ok(files) if files.len() > 1 => {
                // A program went out: an earlier refusal no longer
                // describes the document's post (#265).
                self.post_refusal = None;
                self.show_posted_mounts(files);
            }
            Ok(files) => {
                let Some(first) = files.into_iter().next() else { return };
                let (path, posted) = (first.path, first.posted);
                // A program went out: an earlier refusal no longer
                // describes the document's post (#265).
                self.post_refusal = None;
                self.status = format!(
                    "posted {} op(s), {} lines to {}",
                    posted.ops,
                    posted.lines,
                    path.display()
                );
                // A program the material sim refused to judge posts
                // only on its generator-time verdicts - and says so on
                // the status line, never silently.
                if let Some(line) = &posted.not_judged {
                    self.status = format!("{} - WARNING: {line}", self.status);
                }
                // The viewer shows what the gate just wrote - text and
                // positions from the same engine run, so highlighting a
                // line highlights the move that line posts.
                self.post_view = Some(PostView {
                    path,
                    program: posted.program,
                    line_pos: posted.line_pos,
                    open: true,
                    line: 0,
                    blocks: posted.blocks,
                    trace: false,
                    not_judged: posted.not_judged,
                });
            }
            // A refused post leaves any previous program (file and viewer)
            // exactly as it was - the gate writes nothing on refusal. The
            // reason also goes to its own window (#265), because the
            // status line is overwritten by the next click or command.
            Err(err) => {
                let reason = err.to_string();
                self.status = format!("post refused: {reason}");
                self.post_refusal = Some(crate::egui_app::post_refusal::PostRefusal {
                    path,
                    reason,
                    doc_version: self.doc_version,
                });
            }
        }
    }

    /// M7.2 D: extract the machined stock and write it as STL. The view
    /// the viewport traces is reused when it is the whole program (the
    /// mesh cache keys on it); otherwise - transport up, or no GPU eye
    /// asked yet - the whole program is walked here. A refusal (no
    /// stock, an open extraction, a write error) is the status line.
    fn export_stock_path(&mut self, path: PathBuf) {
        let fv = match &self.field_view {
            Some(Ok(fv)) if self.sim.is_none() => std::sync::Arc::clone(fv),
            _ => match camcraft_app::stock_view::field_view(&self.world) {
                Ok(fv) => std::sync::Arc::new(fv),
                Err(err) => {
                    self.status = format!("stock export refused: {err}");
                    return;
                }
            },
        };
        let mesh = match self.stock_mesh.get(&fv) {
            Ok(m) => m,
            Err(err) => {
                self.status = format!("stock export refused: {err}");
                return;
            }
        };
        match std::fs::write(&path, camcraft_app::stock_export::stl_bytes(&mesh)) {
            Ok(()) => self.status = format!("machined stock: {} facets written to {}", mesh.len(), path.display()),
            Err(err) => self.status = format!("stock export write failed: {err}"),
        }
    }

    /// Setup sheets (M3.2 stage D): write every setup's sheet as one
    /// text file, form feeds between pages so any print path paginates
    /// the same way the screen does. A pure read like posting - the
    /// sheet is derived from the document, nothing about it goes in
    /// the history, and the status line is the only effect.
    fn setup_sheet_path(&mut self, path: PathBuf) {
        let sheets = camcraft_app::setup_sheet::render_all(&self.world);
        let mut out = String::new();
        for (i, (_title, sheet)) in sheets.iter().enumerate() {
            if i > 0 {
                out.push('\u{0c}');
                out.push('\n');
            }
            out.push_str(sheet);
        }
        match std::fs::write(&path, out) {
            Ok(()) => {
                self.status = format!(
                    "{} setup sheet(s) written to {}",
                    sheets.len(),
                    path.display()
                )
            }
            Err(err) => self.status = format!("sheet write failed: {err}"),
        }
    }
}

/// Short human name for a file path, for status text.
fn file_label(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model")
        .to_string()
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

fn pick_mesh_file() -> Option<PathBuf> {
    // STEP arrives tessellated through truck (mesh, not B-rep).
    crate::file_dialog::FileDialog::new()
        .add_filter("Mesh", &["stl", "obj", "step", "stp"])
        .pick_file()
}

fn pick_project_file() -> Option<PathBuf> {
    crate::file_dialog::FileDialog::new()
        .add_filter("CamCraft Project", &["camproj"])
        .pick_file()
}

fn pick_machine_file() -> Option<PathBuf> {
    // The `.mch` layout is the machine-definition JSON format we reuse
    // by user instruction (M2.3 C); the filter just names it.
    crate::file_dialog::FileDialog::new()
        .add_filter("Machine Definition", &["mch"])
        .pick_file()
}

fn pick_post_file() -> Option<PathBuf> {
    // The `.cps` script format is the one we reuse by user instruction
    // (M3.3 A) - plain JavaScript, the user's own; the filter names it.
    crate::file_dialog::FileDialog::new()
        .add_filter("Post Script", &["cps"])
        .pick_file()
}

/// What the status line calls the document's theme: built-ins by name,
/// a file by path - the `post_label` law, one truth for what the screen
/// wears. (The switcher's row labels may read richer for a discovered
/// file; the reference is the law, the label is furniture.)
#[must_use]
pub fn theme_label(world: &World) -> String {
    match theme_of(world) {
        ThemeRef::Dark => "Dark".to_owned(),
        ThemeRef::Named(name) => titled(&name),
        ThemeRef::File(path) => path,
    }
}

fn titled(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn pick_theme_file() -> Option<PathBuf> {
    // `.json` is OUR OWN `camcraft-theme/1` format (M6.1 B) - the
    // filter names it; the selection-time parse decides it. The dialog
    // opens on the discovered folder (`theme_dir`, M13.3) when there is
    // one; `State::new` already said so on the status line when not.
    let mut dialog = crate::file_dialog::FileDialog::new().add_filter("CamCraft Theme", &["json"]);
    if let Ok(dir) = crate::theme_dir::from_process() {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_file()
}

pub(crate) fn save_project_dialog(current: Option<&Path>) -> Option<PathBuf> {
    let mut dialog = crate::file_dialog::FileDialog::new().add_filter("CamCraft Project", &["camproj"]);
    if let Some(parent) = current.and_then(Path::parent) {
        dialog = dialog.set_directory(parent);
    }
    let filename = current
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("untitled.camproj");
    dialog.set_file_name(filename).save_file()
}

/// The Post dialog, opened where `crate::post_file::post_target` says
/// (#258: the last post's folder and name, else the project's).
fn post_program_dialog(target: &crate::post_file::PostTarget) -> Option<PathBuf> {
    // The extension law lives with the script (fanuc.cps's `extension`
    // is "nc"); the dialog just offers it.
    let mut dialog = crate::file_dialog::FileDialog::new().add_filter("NC Program", &["nc"]);
    if let Some(dir) = &target.dir {
        dialog = dialog.set_directory(dir);
    }
    dialog.set_file_name(&target.file_name).save_file()
}

fn save_stl_dialog() -> Option<PathBuf> {
    crate::file_dialog::FileDialog::new()
        .add_filter("STL", &["stl"])
        .set_file_name("machined-stock.stl")
        .save_file()
}

fn save_sheet_dialog() -> Option<PathBuf> {
    // Plain text on purpose: the sheet is meant to be read on paper
    // next to the machine, so the format is whatever Notepad opens.
    crate::file_dialog::FileDialog::new()
        .add_filter("Text", &["txt"])
        .set_file_name("setup-sheets.txt")
        .save_file()
}
