//! Provenance: canonical order and identity of a sketch chain. A closed chain is turned
//! counterclockwise by its signed area (Green's theorem) and started at its lowest joint; an
//! open chain starts at its lower end. The identity is the 128-bit FNV-1a hash (Fowler, Noll
//! and Vo; IETF draft-eastlake-fnv, "The FNV Non-Cryptographic Hash Algorithm") of the layer
//! name and the quantized curves in that order. Original implementation.

use super::ChainId;
use super::connect::Walk;
use super::curve::{Key, Piece, Shape, cross, dist, key, quantum};

const FNV_OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
const FNV_PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
/// Mixed in first; raise the version when the fed bytes change, so old ids never alias new ones.
const DOMAIN: &[u8] = b"camcraft-sketch-chain-v1";

struct Fnv(u128);

impl Fnv {
    fn bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u128::from(byte)).wrapping_mul(FNV_PRIME);
        }
    }

    fn int(&mut self, v: i64) {
        self.bytes(&v.to_le_bytes());
    }

    fn key(&mut self, k: Key) {
        self.int(k.0);
        self.int(k.1);
    }
}

/// Signed area of a closed walk, its joint gaps bridged by straight lines.
pub(super) fn signed_area(pieces: &[(usize, Piece)]) -> f64 {
    let mut area = 0.0;
    for (i, (_, piece)) in pieces.iter().enumerate() {
        area += piece.area_term();
        if let Some((_, next)) = pieces.get((i + 1) % pieces.len()) {
            area += 0.5 * cross(piece.b, next.a);
        }
    }
    area
}

/// The widest joint gap the walk bridges (a closed walk includes its closing joint).
pub(super) fn max_gap(walk: &Walk) -> f64 {
    let n = walk.pieces.len();
    let joints = if walk.closed { n } else { n.saturating_sub(1) };
    let mut gap = 0.0f64;
    for i in 0..joints {
        if let (Some((_, from)), Some((_, to))) = (walk.pieces.get(i), walk.pieces.get((i + 1) % n))
        {
            gap = gap.max(dist(from.b, to.a));
        }
    }
    gap
}

fn reverse(walk: &mut Walk) {
    walk.pieces.reverse();
    for (_, piece) in &mut walk.pieces {
        *piece = piece.flipped();
    }
}

/// Put the walk in its canonical direction and start. Returns the area of a closed walk
/// (positive after the turn; zero for an open one).
pub(super) fn canonicalize(walk: &mut Walk) -> f64 {
    if walk.closed {
        let mut area = signed_area(&walk.pieces);
        if area < 0.0 {
            reverse(walk);
            area = -area;
        }
        // Joints are farther apart than the tolerance, which is above the quantum, so the
        // lowest key is unique and the start does not depend on the input order.
        let lowest = walk
            .pieces
            .iter()
            .enumerate()
            .min_by_key(|(_, (_, piece))| key(piece.a));
        if let Some((at, _)) = lowest {
            walk.pieces.rotate_left(at);
        }
        return area;
    }
    let first = walk.pieces.first().map(|(_, piece)| key(piece.a));
    let last = walk.pieces.last().map(|(_, piece)| key(piece.b));
    if last < first {
        reverse(walk);
    }
    0.0
}

/// The key the chains of a layer are sorted by: where the canonical walk starts.
pub(super) fn start_key(walk: &Walk) -> Key {
    walk.pieces
        .first()
        .map_or((0, 0), |(_, piece)| key(piece.a))
}

pub(super) fn chain_id(layer: &str, walk: &Walk) -> ChainId {
    let mut hash = Fnv(FNV_OFFSET);
    hash.bytes(DOMAIN);
    hash.int(layer.len() as i64);
    hash.bytes(layer.as_bytes());
    hash.bytes(&[u8::from(walk.closed)]);
    hash.int(walk.pieces.len() as i64);
    for (_, piece) in &walk.pieces {
        match piece.shape {
            Shape::Line => {
                hash.bytes(&[1]);
                hash.key(key(piece.a));
                hash.key(key(piece.b));
            }
            Shape::Arc { center, radius, .. } => {
                hash.bytes(&[2, u8::from(piece.travel_sweep() > 0.0)]);
                hash.key(key(piece.a));
                hash.key(key(piece.b));
                hash.key(key(center));
                hash.int(quantum(radius));
            }
            Shape::Circle { center, radius } => {
                hash.bytes(&[3]);
                hash.key(key(center));
                hash.int(quantum(radius));
            }
        }
    }
    ChainId(hash.0)
}
