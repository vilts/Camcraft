//! Static verification of generated toolpaths: warnings a path can carry
//! before anything is posted.
//!
//! Provenance: segment intersection by the orientation predicate is
//! textbook computational geometry (O'Rourke, "Computational Geometry in
//! C", public material); the tool-reach checks formalize the shop
//! practice of comparing cutter reach against wall height (machine-tool
//! handbooks). The rest-material model below is our own conservative
//! approximation, stated in full where it is used. No vendor source was
//! consulted.
//!
//! Eight checks, each with the exact condition that raises it:
//!
//! - [`Warning::Crossing`] - two Feed moves whose Z ranges overlap cross
//!   in XY. Proper crossings only: touching at a shared vertex (a closed
//!   trace loop, a serpentine join) is not a crossing. Grid-bucketed
//!   (`crossing`, M11.4 C round 4: the all-pairs pair set exactly,
//!   [`crossed_pairs_all`] its reference). `Mode::Envelope`
//!   stands it down - crossed raster families overlap by design, and the
//!   chord that matters there is measured by law (f) instead;
//!   `Mode::Revolve` and `Mode::Radial` stand it down because the XY
//!   reading of a posed path says nothing true about paths that share
//!   an axis line (meridian plunges at the spindle axis, cross-drilled
//!   holes at their intersection - both are the machined part).
//! - [`Warning::Gouge`] - a cutter center closer to a region boundary
//!   than `r - leave - tol`, i.e. the disc bites past the finish surface
//!   it is supposed to touch. Engrave paths (trace) deliberately sit on
//!   boundaries, so `Mode::Engrave` skips it; Envelope and Revolve skip
//!   it because tangency IS their geometry (rings tangent to the model,
//!   lathes tracing the meridian); `Mode::Radial` skips it because a
//!   plunge chord crosses the silhouette wall head-on - d = 0 is the
//!   hole's mouth, not a gouge (M5.2 F2); `Mode::Cone` skips it because
//!   the generatrix ride puts the axis ON the edge and the virtual tip
//!   inside the corner the chamfer makes - both are the promise, and
//!   unlike Engrave the shank law keeps speaking (M2.5 E B).
//! - [`Warning::ShankHit`] - uncut material inside the shank or holder
//!   annulus above the current cut. Rest material: at a boundary sample
//!   q (region and stock loop vertices and edge midpoints, nudged a hair
//!   inside so membership is unambiguous), material survives above height h iff
//!   `h < m(q)`, where `m(q)` is the stock top when q is part (never cut)
//!   or, when q is removable stock, the deepest cut level whose path
//!   swept q. A band bottomed at `cut_z + flute_len` (shank) or
//!   `cut_z + flute_len + shank_len` (holder) with radius above the
//!   flute's to q, with material still above it, collides. Sampling at
//!   vertices and edge midpoints is conservative, not exact. Engrave
//!   never poses and `Mode::Radial` stands down (M5.2 F2): the column
//!   enclosure of a fully horizontal band is a vertical slab over the
//!   part's own walls - the kernel's door margins are the shank law
//!   for a live tool. The fixture form of this law (c2) is pose-aware
//!   3-D and speaks every mode.
//! - [`Warning::SweepHit`] - the assembly meets material DURING a pose
//!   change (M5.1 stage G, law (c3) in [`sweep`]): a coordinated block
//!   drives table and head at once, so the reachable set between the
//!   move's commanded letters includes every lerp of them at every
//!   point along (and down) the path. Both endpoints pass (c); the
//!   grid in between may not. Area/Boundary paths only; moves (c)
//!   already convicted are skipped; advisory by nature - the same
//!   angles the replay timeline turns on screen.
//! - [`Warning::RapidInStock`] - a lateral rapid below the clearance
//!   plane whose XY path still has material above it, or (#96) a
//!   vertical rapid DOWN whose end lies below the surface standing at
//!   its column: the stock top, lowered by the cuts before it in this
//!   path (a peck's re-entry above its own earlier plunge is clean)
//!   and - read by the operation layer through [`acquit_by_prior`] -
//!   by what the setup's earlier operations cut. A vertical retract
//!   UP rides the tool's own kerf and stays exempt.
//!   `Mode::Revolve` stands this law down too (M5.2 D): a lathe insert
//!   is a point in XY and its whole form lives in the meridian, so the
//!   column law misreads every threading pull-out - which slides along
//!   the flanks of its own cut V - as a plunge through solid. The
//!   turning rapids' safety rides the kernel's clearance radius and
//!   the meridian scan; fixture steel (d2) still speaks every mode.
//!   `Mode::Radial` leaves the column on the same reading inverted
//!   (M5.2 F2): a horizontal bore is a tunnel the height field cannot
//!   see, so the retract sliding out along its own kerf reads as
//!   lateral travel under solid stock. Its rapids answer to a solid
//!   reading instead (#79, `rapids_radial`): the flute body, posed,
//!   against the stock prism (footprint under the top) minus the kerf
//!   the cuts before it swept - a station change through uncut stock
//!   convicts, a retract up its own hole does not.
//! - [`Warning::ShankHit`] again, from the cycle list: a hole whose
//!   entry-to-bottom span exceeds flute + shank + holder pushes the
//!   assembly below the entry surface no matter what the path looks
//!   like. The move laws cannot see this at a matched drill - the
//!   plunge sweeps exactly its own disc, so the hole's rim samples
//!   fall inside the flute radius where law (c)'s annulus is empty -
//!   which is why the reach law reads the cycles, not the moves.
//! - [`Warning::FixtureHit`] - the tool assembly passes through a
//!   fixture box (M3.2 stage B). Workholding steel is never cut and
//!   never gouge-relevant; it joins exactly two laws: (c2), every
//!   cutting move's swept cylinder stack - flute column, shank band,
//!   holder band - against every box, where a jaw in the path is a
//!   crash like uncut material is, and (d2), the tool body along a
//!   rapid - with no clearance or kerf exemptions, because steel is
//!   never removed material. A POSED move (held or stated pose off
//!   the upright) is also swept with its real pose - the whole stack
//!   as capsules along the lerped pose ([`posed_steel`], M13.8): a
//!   radial index under a bar's equator carries its holder toward the
//!   chuck, and the column enclosures never looked there. Z contact within
//!   [`crate::fixture::FIXTURE_HIT_TOL`]
//!   of a face is silent (mating surfaces touch by design); everything
//!   else errs toward HIT.
//! - [`Warning::Envelope`] - the measured verdict of a 3-D finish
//!   (`Mode::Envelope`, law (f) below): the swept ball envelope cuts
//!   below, or leaves above, the model surface by more than
//!   `finish_tol` plus the scan's quantization floor. It replaces
//!   [`Warning::Gouge`] for ball paths, whose tangency the distance
//!   law would misjudge. Its posed half, law (f2) in [`contact`]:
//!   in `Mode::Envelope` every cutting move holding a LEANING pose
//!   claims contact (the lean is a function of the contact normal),
//!   so its ball centre - tip plus `r` along the pose - must stand
//!   within `r + finish_tol + leave` (plus the stamp's 0.01 contact
//!   lattice) of the mesh, or the verdict reports the gap as rest.
//!   Shank escapes in air are excused exactly where the upright band
//!   stack at the same centre would enter the solid. This is what
//!   convicts a pose turned about the tip instead of the ball centre
//!   (the ridge rest law only judges points two passes came near, so
//!   an all-air pass never reaches it).
//!
//! Beside the warnings, one positive verdict (M13.8): an upright
//! program whose every cut stands above all material provably cuts
//! nothing ([`cuts_no_material`]). The laws above are silent on it -
//! no ball meets the part - so the operation layer names it with a
//! note instead of letting that silence pass as a judged finish.
//!
//! With [`VerifyContext::model`] set (the 3-D pocket, M2.1a) the
//! boundary is depth-dependent: the gouge law adds the model's
//! cross-section strictly above each move's deepest plane, the reach
//! laws sample those sections' walls like any other island, and rest
//! material counts from the deepest plane whose section does not hold
//! the column - cutting around an island removes nothing inside it.
//! Sections only see what a plane cuts, so the model also answers the
//! solid law (b2, [`solid_hits`]): the whole assembly at every sampled
//! tip along every move, against the mesh's solid - the blind floor no
//! level slices, the slab hanging over a ring, an undercut lip.
//!
//! Conservative by design: it may warn where nothing is wrong (a wall
//! thinner than the sampling, an exotic holder shape), it must never
//! stay silent about the seven classes above. The hard post-gate that
//! blocks on these lands with the post processor (M2.3); until then the
//! operation layer attaches them to the operation for display.
//!
//! Arc moves are flattened to chords before the laws run
//! ([`Move::chords`]) at `gouge_tol`: a chord sits within that
//! tolerance of the arc it samples, which is exactly the slack the
//! radial laws already grant - so no violation beyond tolerance hides
//! in an arc. Warnings keep indexing *moves*; an arc is one move, so
//! swapping chords for arcs never shifts an index.


pub use crate::tools::ToolGeom;


mod air;
mod arc_bulge;
mod arc_wall;
mod arc_wall_plane;
mod c_steel;
mod contact;
mod crossing;
mod cut_index;
mod engine;
mod excess;
mod finish;
mod hidden;
mod hole_wall;
mod laws;
mod mesh_posed;
mod mesh_steel;
mod midpose;
// #672: the plasma through-cut laws (cut order, kerf crossing, tip-up).
pub mod plasma;
mod posed_steel;
mod prior;
mod body_bands;
mod probe;
mod rapids;
mod rapids_posed;
mod rapids_radial;
mod reach;
mod rest;
mod rotary;
mod rotary_geom;
mod section;
mod solid;
mod solid_index;
mod stack_span;
mod stock_top;
mod stock_top_shell;
mod sweep;
mod swept;
mod types;
mod workholding;

pub use excess::excess_floor;
pub use types::{FixturePart, Mode, VerifyContext, Warning};
#[doc(hidden)]
pub use crossing::{crossed_pairs, crossed_pairs_all};
pub use engine::{verify, verify_bodies};
pub use air::cuts_no_material;
pub use prior::{acquit_by_prior, prior_surface};
pub use stock_top::acquit_by_stock_top;
pub use laws::{
    bands_vs_fixtures, bands_vs_fixtures_held, rapids_vs_fixtures, rapids_vs_fixtures_held,
    rapids_vs_fixtures_spindle,
};
pub use workholding::{
    bands_vs_workholding, bands_vs_workholding_held, rapids_vs_workholding,
    rapids_vs_workholding_held, Workholding,
};
pub use body_bands::{BodyBands, NeckBands, BODY_BANDS_CAP};
pub use probe::Cutter;
pub use rotary::{rotary_hits, RotaryAxis, RotaryLaw};
pub use hole_wall::{HoleAxis, WALL_SAG};
pub use solid::{SolidLaw, solid_hits, solid_hits_walled};
pub use mesh_steel::{mesh_refined, steel_stack, MeshSteel, SteelBand};
#[doc(hidden)]
pub use contact::posed_contact;
