//! Wavefront OBJ reader (subset: `v` and `f` geometry; material/group lines
//! parsed-and-ignored). Polygon faces are fan-triangulated. Vertex dedup is
//! by exact bit pattern (f64), so coincident vertices are shared, not
//! duplicated; face references still count the `v` records of the file,
//! repeated ones included (#3298).
//!
//! Provenance: the OBJ line grammar is public; implemented from that spec.

use std::collections::HashMap;

use super::{ImportError, MAX_FACETS, MAX_VERTICES};
use crate::geometry::{Mesh, Point3, Triangle};

pub fn parse(data: &[u8]) -> Result<Mesh, ImportError> {
    let text = std::str::from_utf8(data)
        .map_err(|_| ImportError::Obj("not valid UTF-8".into()))?;

    let mut verts: Vec<Point3> = Vec::new();
    // #3298: one entry per `v` record in file order, holding its index in
    // `verts`. Face references count records, not unique positions. It grows
    // linearly with the input bytes (each record is at least 7 bytes).
    let mut records: Vec<u32> = Vec::new();
    let mut seen: HashMap<(u64, u64, u64), u32> = HashMap::new();
    let mut facets: Vec<Triangle> = Vec::new();

    for (ln, raw) in text.lines().enumerate() {
        let line = ln + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut tok = trimmed.split_whitespace();
        let head = match tok.next() {
            Some(h) => h,
            None => continue,
        };
        match head {
            "v" => {
                let p = read_point(&mut tok, line)?;
                if verts.len() >= MAX_VERTICES {
                    return Err(ImportError::TooLarge);
                }
                let key = key_of(p);
                let unique = match seen.entry(key) {
                    std::collections::hash_map::Entry::Vacant(vacant) => {
                        let index = verts.len() as u32;
                        vacant.insert(index);
                        verts.push(p);
                        index
                    }
                    std::collections::hash_map::Entry::Occupied(occupied) => *occupied.get(),
                };
                records.push(unique);
            }
            "f" => {
                let mut refs: Vec<u32> = Vec::new();
                for word in tok {
                    let record = resolve_ref(word, records.len(), line)?;
                    // resolve_ref bounds `record` by records.len()
                    let unique = records.get(record).copied().ok_or_else(|| {
                        ImportError::Obj(format!("line {line}: vertex reference '{word}' out of range"))
                    })?;
                    refs.push(unique);
                }
                if refs.len() < 3 {
                    return Err(ImportError::Obj(format!(
                        "line {line}: face has {} vertices",
                        refs.len()
                    )));
                }
                let v0 = refs[0];
                // fan from v0 across the REST of the loop, in pairs
                for pair in refs[1..].windows(2) {
                    let t = Triangle::new(verts[v0 as usize], verts[pair[0] as usize], verts[pair[1] as usize]);
                    let n = t.normal();
                    if n.x == 0.0 && n.y == 0.0 && n.z == 0.0 {
                        return Err(ImportError::Obj(format!(
                            "line {line}: degenerate triangle"
                        )));
                    }
                    if facets.len() >= MAX_FACETS {
                        return Err(ImportError::TooLarge);
                    }
                    facets.push(t);
                }
            }
            // Non-geometric or unsupported geometry directives.
            "vt" | "vn" | "vp" | "s" | "o" | "g" | "usemtl" | "mtllib" => {
                for word in tok {
                    if head == "vt" || head == "vn" || head == "vp" {
                        // values must at least parse as numbers, else it is
                        // not an OBJ we can trust
                        if word.parse::<f64>().is_err() {
                            return Err(ImportError::Obj(format!(
                                "line {line}: bad '{head}' value '{word}'"
                            )));
                        }
                    }
                }
            }
            "p" | "l" => {
                return Err(ImportError::Obj(format!(
                    "line {line}: unsupported directive '{head}'"
                )));
            }
            other => {
                return Err(ImportError::Obj(format!(
                    "line {line}: unknown directive '{other}'"
                )));
            }
        }
    }

    if facets.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(Mesh::from_facets(facets))
}

fn read_point<'a>(tok: &mut impl Iterator<Item = &'a str>, line: usize) -> Result<Point3, ImportError> {
    let mut xyz = [0.0f64; 3];
    for slot in xyz.iter_mut() {
        let word = tok
            .next()
            .ok_or_else(|| ImportError::Obj(format!("line {line}: short vertex")))?;
        *slot = word
            .parse::<f64>()
            .map_err(|_| ImportError::Obj(format!("line {line}: bad vertex value '{word}'")))?;
    }
    let p = Point3::new(xyz[0], xyz[1], xyz[2]);
    if !p.is_finite() {
        return Err(ImportError::Obj(format!(
            "line {line}: non-finite vertex coordinate"
        )));
    }
    Ok(p)
}

/// `1`, `1/2`, `1//3`, `1/2/3`, and negative (relative) references.
fn resolve_ref(word: &str, nverts: usize, line: usize) -> Result<usize, ImportError> {
    let first = word.split('/').next().unwrap_or(word); // split always yields >=1 item
    let idx: i64 = first
        .parse()
        .map_err(|_| ImportError::Obj(format!("line {line}: bad vertex reference '{word}'")))?;
    let resolved = if idx < 0 {
        nverts as i64 + idx // negative indices count back from the end
    } else {
        idx - 1 // OBJ is 1-based
    };
    if resolved < 0 || resolved as usize >= nverts {
        return Err(ImportError::Obj(format!(
            "line {line}: vertex reference '{word}' out of range"
        )));
    }
    Ok(resolved as usize)
}

/// Exact-bit key with -0.0 folded to +0.0 (NaN is impossible: finite-checked).
fn key_of(p: Point3) -> (u64, u64, u64) {
    fn norm(v: f64) -> u64 {
        if v == 0.0 {
            0.0f64.to_bits()
        } else {
            v.to_bits()
        }
    }
    (norm(p.x), norm(p.y), norm(p.z))
}
