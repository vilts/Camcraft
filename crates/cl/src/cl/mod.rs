//! Cutter-location (CL) data: a linked sequence of machine moves.
//!
//! Provenance: our own minimal CL model. Move kinds map to the G0/G1
//! vocabulary of ISO 6983 / RS-274 (public standards); the rapid/feed/
//! plunge split is general machining practice. Arcs, cycles, paths and
//! poses each live behind their own door (M5.0.1's discipline, the
//! pose added with M5.1 stage A).
//!
//! A move may state a TOOL POSE: the unit axis [`Move::axis`], the
//! same vector the RS-274 letter angles command, converted in exactly
//! one place - [`pose`] - and read by the sim's swept-band laws, the
//! checker's refusal law, and the post's stream dump. `None` is no
//! angle stated: prismatic, which remains the ordinary case for every
//! path this app generates today.
//!
//! A 3+2 op adds the third companion: [`TiltedFrame`], the tilted work
//! plane the pose belongs to (M5.1 stage B). Generators run in the
//! frame's local coordinates and cross to the world through exactly
//! one door, [`TiltedFrame::world_path`].
//!
//! Invariant: every move's `from` equals the previous move's `to` (the
//! path is a chain, no gaps); generators build through [`PathBuilder`],
//! which maintains it by construction.

mod comp;
mod cycle;
mod edge_break;
mod edge_volume;
mod frame;
mod jet;
mod moves;
mod path;
mod plane;
mod pose;
mod rotary;
mod rotary_unwind;
pub mod text;
pub mod transform;
mod thread;
mod torch;
mod touch;

pub use comp::{CompSide, CompSpan};
pub use cycle::{Cycle, CycleKind};
pub use edge_break::EdgeBreak;
pub use edge_volume::{BreakFrame, BreakProfile, EdgeBreakVolume};
pub use frame::TiltedFrame;
pub use jet::{parse_header as parse_waterjet_header, parse_off as parse_jet_off, parse_on as parse_jet_on, JetSpan, WaterjetCut};
pub use moves::{arc_chords, lattice, ArcMove, Move, MoveKind, MAX_ARC_CHORDS};
pub use path::{PathBuilder, Toolpath};
pub use plane::ArcPlane;
pub use thread::{ThreadCycle, ThreadHand};
pub use torch::{parse_header as parse_plasma_header, parse_off as parse_torch_off, parse_on as parse_torch_on, ContourRole, PlasmaCut, TorchSpan};
pub use touch::ProbeTouch;
pub use rotary::{stamp_c, unwrap_near};
pub use rotary_unwind::unwind_c;
pub use pose::{angles_to_axis, axis_to_angles, is_vertical, normalize_axis};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod plane_tests;
