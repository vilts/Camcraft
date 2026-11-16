//! The signature of an operation's cached path (issue #109 review B1):
//! a hash of every field a move carries, computed ONCE when a cache is
//! built ([`crate::schema::Toolpath::new`]) so the per-frame staleness
//! question reads a stored word instead of cloning and hashing every
//! predecessor's path.
//!
//! Every float is hashed on the 1 um lattice ([`crate::cl::lattice`]),
//! the form the project file stores a move in (`project::entity`'s
//! `MoveDto`): a generated path and the same path saved and loaded
//! then sign alike, so a dependent op's rest digest survives a reopen
//! (#1351). A change of half a micron or more still moves the sign.
//!
//! Provenance: our own design (a content hash memoised at the write, the
//! same idea as M5.3 A's geometry signature); no vendor material.

use crate::cl::{lattice, Move};

/// One float as the project file stores it: snapped to the lattice.
/// Values already on the lattice hash exactly as they always did.
fn put(h: &mut impl std::hash::Hasher, v: f64) {
    h.write(&lattice(v).to_bits().to_le_bytes());
}

/// Identity of a cached path: every field a move carries, on the
/// lattice the file keeps. The generators are deterministic functions
/// of what the stamp digests, so equal paths here means the rest mask
/// builds identically.
pub fn of(moves: &[Move]) -> u64 {
    use std::hash::Hasher;
    let mut h = std::hash::DefaultHasher::new();
    h.write(b"camcraft-rest-path-v1");
    h.write_u64(moves.len() as u64);
    for m in moves {
        h.write_u8(match m.kind {
            crate::cl::MoveKind::Rapid => 0,
            crate::cl::MoveKind::Feed => 1,
            crate::cl::MoveKind::Plunge => 2,
        });
        for p in [m.from, m.to] {
            put(&mut h, p.x);
            put(&mut h, p.y);
            put(&mut h, p.z);
        }
        match m.feed {
            Some(f) => {
                h.write_u8(1);
                put(&mut h, f);
            }
            None => h.write_u8(0),
        }
        match &m.arc {
            Some(a) => {
                h.write_u8(1);
                put(&mut h, a.center.x);
                put(&mut h, a.center.y);
                h.write_u8(u8::from(a.ccw));
                // M11.2 A: only a vertical plane adds a byte, so every
                // XY arc hashes exactly as it always did.
                if !a.plane.is_xy() {
                    h.write_u8(a.plane.g_code());
                }
            }
            None => h.write_u8(0),
        }
    }
    h.finish()
}
