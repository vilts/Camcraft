//! Dexel (Z-map) stock simulation for 3-D roughing (M2.1b).
//!
//! Provenance: the dexel/Z-map material model is classic published CAM
//! simulation literature (a column-wise height field of remaining
//! material, subtracted per swept tool disc); the region algebra is our
//! Clipper2 wrapper. Provenance also by omission: this grid is the
//! VERIFICATION layer, never the source of truth - operation geometry
//! and tolerance laws keep evaluating against the mesh itself (the
//! ROADMAP's standing rule for any discretized stock).
//!
//! ## The laws here
//!
//! - **Column model.** Every cell holds one number: the Z of the top of
//!   the material standing at that XY. This is exact for everything
//!   this codebase cuts - a Z-plunge world whose tools never machine
//!   under an overhang - and it is the same discretization that makes
//!   `machining3d::material_above` answerable per plane. Undercuts
//!   would need a voxel octree; they need it nowhere yet.
//! - **Subtraction is conservative.** A move removes, per cell, the
//!   LOWEST Z the tool bottom reaches while its disc touches the cell
//!   center - computed from the exact interval of the move that is in
//!   reach, not from the endpoints. XY arcs sweep the TRUE arc
//!   (`dexel_arc`, #402): inscribed chords stand a sagitta nearer the
//!   arc's centre, which over-cuts toward a convex corner (a gouge the
//!   tool never committed, refusing a correct post) and under-cuts
//!   away from it (a concave wall's real gouge hidden) - no chord
//!   placement is honest on both sides at once. XZ / YZ arcs still
//!   ride their in-plane chords.
//! - **Rapids touch nothing** (RS-274 G0 practice: no material contacts
//!   a tool moving at rapid). Feeds and plunges cut.
//! - **The ball atom is exact, not sampled** (`cut_ball`): a sphere's
//!   bottom above a cell is a convex function of the chord parameter,
//!   so its minimum is at a window end or at one stationary point that
//!   a sign-guarded quadratic finds in closed form. Endpoints alone
//!   would over-report rest (and could hide a gouge); the full law is
//!   cheap and never guesses.
//! - **Part surface is read from the mesh**, facet-plane interpolation
//!   at cell centers over each facet's XY bounding box: the highest
//!   surface covering a cell is that column's part top (`-inf` when no
//!   facet covers it - air everywhere along the column). Rest material
//!   and gouges are both measured against this, so the sim's verdict
//!   uses the model, not a second hand-entered surface.
//! - **The program sim chains ops, never re-plans them** (M2.3): the
//!   same conservative atoms, fed the cached paths in program order,
//!   so the grid answers "what does this whole NC program leave
//!   standing" - the stock-comparison deviation and its color map read
//!   that answer, and the post gate judges it once at the end.
//! - **Deviation is located, not just counted** (M2.3): the program
//!   report carries the XY and the sim's own Z of its worst rest and
//!   worst gouge cells, because "the toolpath is trusted before the
//!   machine sees it" means a human can point at where the program is
//!   trusted to be right, or wrong. Columns that answer no verdict
//!   (air in the stock, or air in the model) carry NaN, never a zero
//!   dressed up as a clean bill of health.
//! - **A gouge is judged with `tol` sideways too** (#299, `wall_slack`):
//!   against the lowest part top within `tol` of the column centre, so
//!   a centre microns inside a finished wall is not charged the wall.



mod backend;
mod backend_build;
mod backend_edit;
mod carried;
mod carried_boundary_scan;
mod carried_read;
mod carry;
mod carry_boundary;
mod carry_boundary_prune;
mod carry_verdict;
mod carry_exact;
mod carry_history;
mod carry_profile;
mod carry_capsule;
// #874: the side-on quarter turns.
mod carry_side;
mod columns;
mod cut;
mod dexel;
mod laid_cylinder;
mod dexel_ball;
mod dexel_arc;
// #541: the sinker electrode's box atom.
mod dexel_box;
mod dexel_bull;
// #1152: the sinker body electrode's outline atom.
mod dexel_outline;
mod deviate;
mod edge_break;
mod corner_relief;
mod wall_anchor;
// #1595: the mark an Engrave or Trace declared.
mod mark;
mod display;
mod dual;
// #2626: the f32 twin of the GPU cell cut and its brick work list.
mod cut_twin;
mod cut_twin_work;
// #2629 (GPU stock sim S3b): the playback layer of the twin-able ops.
mod layer_play;
mod field;
mod field_arc;
mod field_batch;
mod field_box;
mod field_carry;
mod field_carry_cut;
mod field_body;
mod field_floor;
mod field_blade;
mod field_bull;
mod field_cut;
mod field_gen;
mod field_lathe;
mod field_ray;
mod field_rotary;
mod field_shell;
mod field_solid;
mod field_sweep;
mod field_turn;
// #1444: the undercut cutter's head and neck solids.
mod field_undercut;
mod gouge;
mod hermite;
mod kernel;
mod march;
mod march_up;
mod march_verdicts;
mod partial;
mod qef;
pub mod iso;
pub mod preview;
mod ridge;
mod ridge_scan;
mod scan;
mod scan_arc;
mod scan_hidden;
mod scan_near;
mod scan_turned;
mod tiles;
mod wall_slack;

pub use backend::{Backend, Stock, FIELD_BAND, FIELD_VERDICT_EPS, LATHE_COLUMN_REFUSAL};
pub use backend_edit::SET_TOPS_FIELD_REFUSAL;
pub use carried::{clip_columns_under, lattice_of_grid, CarriedStock, FieldBody};
pub use carry::{Body, LatticeTurn, CARRY_TURN_REFUSAL};
pub use carry_boundary::RegionStock;
pub use carried_boundary_scan::{boundary_box_penetration, boundary_mesh_penetration};
pub use carry_side::{CarryTurn, SideTurn, FIELD_SIDE_CARRY_REFUSAL};
pub use columns::Columns;
pub use partial::{clamp_frac, partial_move, PartialCut};
pub use cut::{auto_cell, first_posed, simulate, SimOp, POSED_REFUSAL};
pub use deviate::{color_classes, deviate, Deviation, DEV_TOL, ProgramSim, SimSurface};
pub use dexel::{reach, Dexel};
pub use dexel_box::{simulate_box, BOX_ARC_REFUSAL};
pub use dexel_outline::{simulate_outline, OUTLINE_ARC_REFUSAL};
pub use edge_break::credit_breaks;
pub use corner_relief::credit_corner_reliefs;
pub use wall_anchor::{models_a_bevel, on_walls};
pub use mark::{credit_marks, Mark};
// The floor a mark must leave on the part's underside (#1595).
pub use crate::verify::MARK_FLOOR;
pub use cut_twin::{cell_cut, from_ordered_bits, ordered_bits, TileChord, TwinChord, TwinTool, TWIN_ULPS};
pub use cut_twin_work::{
    run_twin, run_twin_items_onto, twin_cut, twin_items, twin_refusal, twin_work, TwinItem, TwinWork, WorkItem,
    TWIN_MAX_CELLS_PER_CHORD,
};
pub use layer_play::{
    cpu_layer_cutter, layer_tool, merge_max, LayerCutter, LayerKey, LayerPlay, LayerSpan, LAYER_KEYS, LAYER_KEY_BUDGET,
};
pub use field::SdfField;
pub use field_body::Lattice;
pub use field_carry::{box_signed, CarriedField, Clip, ClipRefusal};
pub use field_floor::{ColumnSpan, FloorRead};
pub use field_batch::CutStats;
pub use field_blade::{LatheProfile, CROWN_SAG_TOL};
pub use field_cut::SdfTool;
pub use field_lathe::{LatheAxis, MERIDIAN_TOL};
pub use field_rotary::{rotary_chords, rotary_chords_law, ChordLaw, RotaryChordStats, MAX_SUBCHORDS, ROTARY_COLUMN_REFUSAL};
pub use gouge::dist_seg_tri;
pub use kernel::{measure, part_surface, StockError, StockReport};
pub use preview::{preview, Preview, PreviewCamera};
pub use scan::{envelope_scan, envelope_scan_centers, envelope_scan_turned, envelope_scan_within, EnvelopeReport};
pub use scan_hidden::{HiddenRest, ScanBudget};
