//! The drilling intent records: what a hole's cycle is, standing beside
//! the expanded move chain.
//!
//! Provenance: the names carry the RS-274 canned cycles they post to
//! (public standard, general machining practice); the expanded move
//! chain remains the truth for verification, display and simulation,
//! so the two must agree - that agreement is a law with a test, not a
//! hope.

use crate::geo2d::Point2;

/// What a drilled hole's cycle is, in the vocabulary the post needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CycleKind {
    /// Shallow conical entry dimple (G81 with dwell at a spot depth).
    Spot,
    /// One straight plunge to depth, rapid retract (G81).
    Drill,
    /// Chip-breaking pecks, each retracted a hair to snap the chip
    /// before continuing (G73).
    Peck,
    /// Deep-hole pecks: every peck fully retracts to R, dwells at the
    /// bottom (G83).
    Deep,
    /// Rigid tapping: feed down and back at pitch·rpm (G84).
    Tap,

    /// Boring: feed in, rapid out (G86's mill-side shape; spindle
    /// orientation is the machine's, M1.7's post word).
    Bore,
    /// Back-boring: feed in AND feed out — the retract must not drag
    /// the already-cut back surface, so no rapid on the way home.
    BackBore,
    /// Tapping with an explicitly selected floating tap holder.
    FloatingTap,
    /// Reaming (#1445): feed in to the bottom and feed back out to R at
    /// the same feed, spindle running, no dwell - RS-274/NGC's G85
    /// (NIST IR 6556, The NIST RS274NGC Interpreter v3, its G85 cycle;
    /// LinuxCNC's G-code reference, G85). A rapid out would score the finished
    /// wall, so the retract is a feed move in the chain too.
    Ream,
}

/// One hole's machining intent, standing beside the expanded chain:
/// the center, the feed plane it enters at, its bottom, and the cycle's
/// numbers. A post emits one canned block per cycle; re-deriving the
/// cycle from the move chain is exactly what this replaces. Zs are
/// world coordinates, on the same lattice the moves keep.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cycle {
    pub kind: CycleKind,
    pub xy: Point2,
    /// Retract/feed plane: rapids above it, the cycle's feed moves start there.
    pub r_z: f64,
    /// The bottom the cycle cuts to (through holes clear the far wall).
    pub bottom_z: f64,
    /// The material's entry surface at the hole: the plane the tool's
    /// total length may never sink below whole (the reach law's datum).
    pub entry_z: f64,
    /// Depth per peck; 0.0 outside peck cycles.
    pub peck: f64,
    /// Seconds at the bottom; 0.0 means no dwell word.
    pub dwell: f64,
    /// This hole's first move in the chain. Cycle groups are contiguous
    /// in hole order, so `first_move..next.first_move` bounds the moves
    /// that carry the cycle - what the agreement law pins, and what the
    /// reach law's warning points a panel at.
    pub first_move: usize,
}
