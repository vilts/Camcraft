//! The geometry fingerprint a raw-stock-only mark is checked by (#1094).
//! What is hashed: the model's facet count, every vertex coordinate of its
//! LOCAL facets (`f64::to_bits`, in facet order, a b c, x y z), then its
//! placement (`Location`: position x y z and rotation x y z w, as bits) -
//! the geometry the program verdict reads (`stock_sim::part_facets` places
//! the local facets by that `Location`). Two independent 64-bit hashes
//! (FNV-1a over the bytes, and a splitmix64 fold over the words) give 128
//! bits. The same mesh at the same place gives the same print across save
//! and load (the project file round-trips f64 exactly) and undo/redo.
//! The facet hashes are memoised per revision (`schema::GeomPrintMemo`).
//!
//! Provenance: FNV-1a (Fowler/Noll/Vo, public domain) and splitmix64
//! (Steele, Lea, Flood, "Fast splittable pseudorandom number generators",
//! OOPSLA 2014) - textbook non-cryptographic hashes; our own wiring.

use flecs_ecs::prelude::*;

use crate::geometry::Triangle;
use crate::model;
use crate::schema::{GeomPrint, GeomPrintMemo, Location};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const MIX_SEED: u64 = 0x9e37_79b9_7f4a_7c15;

fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Fold 64-bit words into the two lanes.
fn fold(mut h: [u64; 2], words: impl Iterator<Item = u64>) -> [u64; 2] {
    for w in words {
        for b in w.to_le_bytes() {
            h[0] = (h[0] ^ u64::from(b)).wrapping_mul(FNV_PRIME);
        }
        h[1] = splitmix(h[1] ^ w);
    }
    h
}

fn facet_hash(tris: &[Triangle]) -> [u64; 2] {
    let words = tris
        .iter()
        .flat_map(|t| [t.a, t.b, t.c])
        .flat_map(|p| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]);
    fold([FNV_OFFSET, MIX_SEED], words)
}

/// The fingerprint of the model `entity` as it stands, or `None` when it
/// has no facets (nothing to mark). Facet hashes come from the memo when
/// the revision and the mounted facets are the ones it describes.
#[must_use]
pub fn print_of(world: &World, entity: &Entity) -> Option<GeomPrint> {
    let tris = model::facets_of(world, entity).filter(|f| !f.is_empty())?;
    let rev = model::rev_of(world, entity);
    let ptr = tris.as_ptr() as usize;
    let len = tris.len();
    let view = entity.entity_view(world);
    let memo = view
        .try_get::<&GeomPrintMemo>(|m| (m.rev == rev && m.ptr == ptr && m.len == len).then_some(m.hash))
        .flatten();
    let hash = match memo {
        Some(h) => h,
        None => {
            let h = facet_hash(&tris);
            view.set(GeomPrintMemo { rev, ptr, len, hash: h });
            h
        }
    };
    let Location { pos, rot } = model::transform_of(world, entity).unwrap_or_default();
    let place = [pos.x, pos.y, pos.z, rot[0], rot[1], rot[2], rot[3]].map(f64::to_bits);
    let len64 = u64::try_from(len).unwrap_or(u64::MAX);
    Some(GeomPrint { facets: len64, hash: fold(hash, place.into_iter().chain([len64])) })
}

/// The print as the project file writes it: `"<facets>:<hash0><hash1>"`
/// (hex, 16 digits each).
#[must_use]
pub fn to_text(p: &GeomPrint) -> String {
    format!("{}:{:016x}{:016x}", p.facets, p.hash[0], p.hash[1])
}

/// [`to_text`]'s inverse; `None` for anything else.
#[must_use]
pub fn from_text(s: &str) -> Option<GeomPrint> {
    let (n, h) = s.split_once(':')?;
    if h.len() != 32 {
        return None;
    }
    let facets = n.parse().ok()?;
    let a = u64::from_str_radix(h.get(..16)?, 16).ok()?;
    let b = u64::from_str_radix(h.get(16..)?, 16).ok()?;
    Some(GeomPrint { facets, hash: [a, b] })
}
