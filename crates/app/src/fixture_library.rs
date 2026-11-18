//! The fixture library file (#339 slice 6): user-saved workholding
//! solids (vises, chuck jaws, plates), each a named mesh with the datum
//! it is placed by, inside a small `camcraft-fixtures/1` envelope - the
//! tool library's `camcraft-tools/1` shape (#300): a versioned JSON
//! envelope, a strict reader that refuses the whole file by entry name,
//! and a writer whose save -> load -> save is byte-stable.
//!
//! An entry's facets are the fixture's mesh in its OWN model frame,
//! exactly the bytes its `Facets` carry; `datum` is a frame inside that
//! model frame - the point and orientation that land on the picked
//! placement when the entry is inserted (`commands::InsertFixture`).
//! The default datum is the model origin (identity), so a fixture saved
//! and re-inserted at its old pose stands exactly where it stood.
//!
//! Provenance: our own format, built from our own model components; the
//! shapes in a library are whatever the user saved - no vendor fixture
//! library or asset was read, consulted or shipped (#339 triage).

use serde_json::{Value, json};

use crate::geometry::{IDENTITY_Q, Point3, Triangle, normalize_quat};
use crate::model;
use crate::schema::Location;

mod store;

pub use store::{
    ENV_VAR, LIBRARY_FILE, LibraryError, add_entries, delete_entry, library_path, load, save, user_dir,
    user_dir_for,
};

/// The envelope's `format` tag.
pub const FORMAT: &str = "camcraft-fixtures";
/// The envelope's `version`; a reader refuses any other.
pub const VERSION: u64 = 1;
/// The extension of a library file.
pub const EXTENSION: &str = "camfixtures";
/// A sanity cap on one entry's facets (a library entry is a vise, not
/// a scanned building); larger is refused by name, never truncated.
pub const MAX_FACETS: usize = 2_000_000;

/// One named fixture of a library file.
#[derive(Clone, Debug, PartialEq)]
pub struct FixtureEntry {
    pub name: String,
    /// Free text the user saved with it (`None` writes no key).
    pub note: Option<String>,
    /// The placement datum, in the mesh's own frame (a rotation,
    /// normalized when placed).
    pub datum: Location,
    /// The mesh in the fixture's own model frame.
    pub facets: Vec<Triangle>,
    /// Raw STEP source text when saved from an imported STEP fixture (#744).
    pub step_source: Option<String>,
    /// Editable spun/extruded profile when saved from a profile fixture (#744).
    pub profile: Option<crate::fixture_profile::FixtureProfile>,
}

/// Which frame of the fixture a saved entry is placed by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DatumChoice {
    /// The model's own origin and axes (the default).
    #[default]
    ModelOrigin,
    /// The centre of the mesh's bounding box bottom, model axes - the
    /// face a vise or plate sits on.
    BottomCentre,
}

/// The bottom-centre point of a mesh's bounds (finite vertices only).
fn bottom_centre(tris: &[Triangle]) -> Option<Point3> {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    let mut any = false;
    for p in tris.iter().flat_map(|t| [t.a, t.b, t.c]) {
        if p.is_finite() {
            any = true;
            for (k, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                lo[k] = lo[k].min(v);
                hi[k] = hi[k].max(v);
            }
        }
    }
    any.then(|| Point3::new(0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1]), lo[2]))
}

/// The library entry for model `name` of this document: its facets in
/// its own frame and the chosen datum. Refused by name when the model
/// has no geometry, or a non-finite vertex (the library never carries
/// a mesh it would refuse to read back).
pub fn entry_from_model(
    world: &flecs_ecs::prelude::World,
    name: &str,
    datum: DatumChoice,
    note: Option<&str>,
) -> Result<FixtureEntry, String> {
    let e = model::find_by_name(world, name)
        .filter(|e| model::is_model(world, e))
        .ok_or_else(|| format!("no model named '{name}'"))?;
    let facets: Vec<Triangle> = model::facets_of(world, &e).map(|f| f.to_vec()).unwrap_or_default();
    let origin = match datum {
        DatumChoice::ModelOrigin => Point3::ZERO,
        DatumChoice::BottomCentre => bottom_centre(&facets).unwrap_or(Point3::ZERO),
    };
    let step_source = crate::step_source::of(world, &e);
    let profile = crate::fixture::def_of(world, &e).and_then(|d| d.profile);
    let entry = FixtureEntry {
        name: name.trim().to_owned(),
        note: note.map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned),
        datum: Location { pos: origin, rot: IDENTITY_Q },
        facets,
        step_source,
        profile,
    };
    check_entry(&entry)?;
    Ok(entry)
}

/// The per-entry law the reader and every writer share.
pub fn check_entry(e: &FixtureEntry) -> Result<(), String> {
    let name = &e.name;
    if name.trim().is_empty() {
        return Err("a fixture has no name".into());
    }
    if name.trim() != name {
        return Err(format!("fixture '{name}': the name has leading or trailing spaces"));
    }
    if name == crate::fixture::MACHINE_TABLE {
        return Err(format!("fixture '{name}': that name is the derived machine table"));
    }
    if e.facets.is_empty() {
        return Err(format!("fixture '{name}' has no facets"));
    }
    if e.facets.len() > MAX_FACETS {
        return Err(format!("fixture '{name}' has {} facets (at most {MAX_FACETS})", e.facets.len()));
    }
    if e.facets.iter().any(|t| !(t.a.is_finite() && t.b.is_finite() && t.c.is_finite())) {
        return Err(format!("fixture '{name}' has a non-finite vertex"));
    }
    if !e.datum.pos.is_finite() || normalize_quat(e.datum.rot).is_none() {
        return Err(format!("fixture '{name}': the datum must be a finite point and a rotation"));
    }
    if e.step_source.as_ref().is_some_and(|s| s.trim().is_empty()) {
        return Err(format!("fixture '{name}': step_source cannot be blank"));
    }
    if let Some(prof) = &e.profile {
        prof.check().map_err(|why| format!("fixture '{name}': invalid profile ({why})"))?;
    }
    Ok(())
}

fn num3(p: Point3) -> Value {
    json!([p.x, p.y, p.z])
}

/// Library text for `entries`, sorted by name: an indented envelope a
/// person can read, one facet per line so a large mesh stays a list of
/// lines rather than one number per line. Lengths are millimetres.
/// Refuses an entry the reader would refuse, so a written file always
/// reads back.
pub fn to_text(entries: &[FixtureEntry]) -> Result<String, String> {
    let mut sorted: Vec<&FixtureEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    for w in sorted.windows(2) {
        if let [a, b] = w
            && a.name == b.name
        {
            return Err(format!("fixture '{}' appears twice in the library", a.name));
        }
    }
    let s = |v: &Value| serde_json::to_string(v).map_err(|e| format!("fixture library not written: {e}"));
    let mut out = String::new();
    out.push_str(&format!(
        "{{\n  \"format\": {},\n  \"version\": {VERSION},\n  \"units\": \"mm\",\n  \"fixtures\": [",
        s(&json!(FORMAT))?
    ));
    for (i, e) in sorted.iter().enumerate() {
        check_entry(e)?;
        out.push_str(if i == 0 { "\n" } else { ",\n" });
        out.push_str(&format!("    {{\n      \"name\": {},\n", s(&json!(e.name))?));
        if let Some(note) = &e.note {
            out.push_str(&format!("      \"note\": {},\n", s(&json!(note))?));
        }
        if let Some(step) = &e.step_source {
            out.push_str(&format!("      \"step_source\": {},\n", s(&json!(step))?));
        }
        if let Some(prof) = &e.profile {
            let v = serde_json::to_value(prof).map_err(|err| format!("fixture library not written: {err}"))?;
            out.push_str(&format!("      \"profile\": {},\n", s(&v)?));
        }
        let datum = json!({ "origin": num3(e.datum.pos), "rot": e.datum.rot });
        out.push_str(&format!("      \"datum\": {},\n      \"facets\": [\n", s(&datum)?));
        for (k, t) in e.facets.iter().enumerate() {
            let tri = json!([num3(t.a), num3(t.b), num3(t.c)]);
            out.push_str(&format!("        {}{}\n", s(&tri)?, if k + 1 < e.facets.len() { "," } else { "" }));
        }
        out.push_str("      ]\n    }");
    }
    out.push_str(if sorted.is_empty() { "]\n}\n" } else { "\n  ]\n}\n" });
    Ok(out)
}

fn point(v: Option<&Value>) -> Option<Point3> {
    let a = v?.as_array()?;
    match a.as_slice() {
        [x, y, z] => Some(Point3::new(x.as_f64()?, y.as_f64()?, z.as_f64()?)),
        _ => None,
    }
}

fn read_entry(row: &Value, n: usize) -> Result<FixtureEntry, String> {
    let name = row
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("fixture #{n} has no name"))?
        .to_owned();
    let note = match row.get("note") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_str().ok_or_else(|| format!("fixture '{name}': the note is not text"))?.to_owned()),
    };
    let d = row.get("datum").ok_or_else(|| format!("fixture '{name}' has no datum"))?;
    let pos = point(d.get("origin")).ok_or_else(|| format!("fixture '{name}': the datum origin is not [x, y, z]"))?;
    let rot: Vec<f64> = d
        .get("rot")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    let rot: [f64; 4] = rot
        .try_into()
        .map_err(|_| format!("fixture '{name}': the datum rot is not [x, y, z, w]"))?;
    // Kept as written (the insert normalizes at its chokepoint), so a
    // read -> write round trip is byte-stable; only a non-rotation fails.
    if normalize_quat(rot).is_none() {
        return Err(format!("fixture '{name}': the datum rot is not a rotation"));
    }
    let rows = row
        .get("facets")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("fixture '{name}' has no \"facets\" array"))?;
    let mut facets = Vec::with_capacity(rows.len().min(MAX_FACETS));
    for (k, t) in rows.iter().enumerate() {
        let tri = t.as_array().and_then(|v| match v.as_slice() {
            [a, b, c] => Some(Triangle::new(point(Some(a))?, point(Some(b))?, point(Some(c))?)),
            _ => None,
        });
        let tri = tri.ok_or_else(|| format!("fixture '{name}': facet #{} is not three [x, y, z] points", k + 1))?;
        facets.push(tri);
    }
    let step_source = match row.get("step_source") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_str().ok_or_else(|| format!("fixture '{name}': step_source is not text"))?.to_owned()),
    };
    let profile = match row.get("profile") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            serde_json::from_value::<crate::fixture_profile::FixtureProfile>(v.clone())
                .map_err(|e| format!("fixture '{name}': invalid profile: {e}"))?,
        ),
    };
    let entry = FixtureEntry { name, note, datum: Location { pos, rot }, facets, step_source, profile };
    check_entry(&entry)?;
    Ok(entry)
}

/// Read a library file's text. The whole file is checked before anything
/// is returned: malformed JSON, a wrong format tag, version or units, an
/// entry without a name, datum or facets, a name twice, an empty mesh or
/// a non-finite number each refuse the whole file, naming the entry.
/// An empty `fixtures` array is a valid (empty) library.
pub fn parse(text: &str) -> Result<Vec<FixtureEntry>, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| format!("not a fixture library (bad JSON: {e})"))?;
    let o = root.as_object().ok_or("not a fixture library: expected an object")?;
    match o.get("format").and_then(Value::as_str) {
        Some(FORMAT) => {}
        Some(other) => return Err(format!("not a fixture library: format is '{other}', not '{FORMAT}'")),
        None => return Err(format!("not a fixture library: no \"format\": \"{FORMAT}\"")),
    }
    match o.get("version").and_then(Value::as_u64) {
        Some(VERSION) => {}
        Some(v) => return Err(format!("fixture library version {v} is not supported (this build reads {VERSION})")),
        None => return Err("not a fixture library: no \"version\"".into()),
    }
    if let Some(u) = o.get("units").and_then(Value::as_str)
        && u != "mm"
    {
        return Err(format!("fixture library units '{u}' are not supported (lengths are mm)"));
    }
    let rows = o
        .get("fixtures")
        .and_then(Value::as_array)
        .ok_or("not a fixture library: no \"fixtures\" array")?;
    let mut out: Vec<FixtureEntry> = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let entry = read_entry(row, i.saturating_add(1))?;
        if out.iter().any(|e| e.name == entry.name) {
            return Err(format!("fixture '{}' appears twice in the library", entry.name));
        }
        out.push(entry);
    }
    Ok(out)
}
