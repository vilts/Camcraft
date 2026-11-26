//! Provenance: our own file format; no vendor material.
//!
//! The operation-level file mirror of [`crate::path_picks::PathPicks`]
//! (#3339): `picks: { start: [[x, y, z?], ...], exit: [...] }`, written
//! only when a role list is not empty so files without picks stay unchanged.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::geo2d::Point2;
use crate::path_picks::{PathPick, PathPicks};

/// One picked coordinate in project JSON: `[x, y]` or `[x, y, z]`.
#[derive(Clone, Debug, PartialEq)]
pub struct PathPickDto(pub PathPick);

impl Serialize for PathPickDto {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.0.z {
            Some(z) => [self.0.xy.x, self.0.xy.y, z].serialize(s),
            None => [self.0.xy.x, self.0.xy.y].serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for PathPickDto {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Xy([f64; 2]),
            Xyz([f64; 3]),
        }
        let pick = match Repr::deserialize(d)? {
            Repr::Xy([x, y]) => PathPick::xy(Point2::new(x, y)),
            Repr::Xyz([x, y, z]) => PathPick { xy: Point2::new(x, y), z: Some(z) },
        };
        Ok(Self(pick))
    }
}

/// Operation-level `picks` object in project JSON.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PathPicksDto {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub start: Vec<PathPickDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exit: Vec<PathPickDto>,
}

impl PathPicksDto {
    /// True when neither role holds a pick; keeps `picks` off the wire.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start.is_empty() && self.exit.is_empty()
    }

    #[must_use]
    pub fn from_picks(picks: &PathPicks) -> Self {
        Self {
            start: picks.start.iter().copied().map(PathPickDto).collect(),
            exit: picks.exit.iter().copied().map(PathPickDto).collect(),
        }
    }

    #[must_use]
    pub fn to_picks(&self) -> PathPicks {
        PathPicks {
            start: self.start.iter().map(|d| d.0).collect(),
            exit: self.exit.iter().map(|d| d.0).collect(),
        }
    }

    /// Convert to runtime `PathPicks`, filling any empty role from pre-#3339
    /// legacy keys (`links.approach.start_point`/`exit_point` or wire `start`).
    #[must_use]
    pub fn with_legacy(&self, start: Option<[f64; 2]>, exit: Option<[f64; 2]>) -> PathPicks {
        let mut out = self.to_picks();
        if out.start.is_empty() && let Some([x, y]) = start {
            out.start.push(PathPick::xy(Point2::new(x, y)));
        }
        if out.exit.is_empty() && let Some([x, y]) = exit {
            out.exit.push(PathPick::xy(Point2::new(x, y)));
        }
        out
    }
}

/// Migrate pre-#3339 legacy keys (`start` on wire_contour, `start_point` /
/// `exit_point` under `links.approach`) into `picks` in an MCP or script
/// patch map so existing callers continue to work (#3339).
pub fn migrate_legacy_patch(key: &str, patch: &mut serde_json::Map<String, serde_json::Value>) {
    let mut legacy_start = None;
    let mut legacy_exit = None;
    if key == "wire_contour" && patch.contains_key("start") {
        legacy_start = patch.remove("start");
    }
    let mut empty_links = false;
    if let Some(links) = patch.get_mut("links").and_then(serde_json::Value::as_object_mut) {
        if let Some(approach) = links.get_mut("approach").and_then(serde_json::Value::as_object_mut) {
            if approach.contains_key("start_point") {
                legacy_start = approach.remove("start_point");
            }
            if approach.contains_key("exit_point") {
                legacy_exit = approach.remove("exit_point");
            }
        }
        if links.get("approach").is_some_and(|a| a.as_object().is_some_and(|m| m.is_empty())) {
            links.remove("approach");
        }
        if links.is_empty() {
            empty_links = true;
        }
    }
    if empty_links {
        patch.remove("links");
    }
    let mut empty_approach = false;
    if let Some(approach) = patch.get_mut("approach").and_then(serde_json::Value::as_object_mut) {
        if approach.contains_key("start_point") {
            legacy_start = approach.remove("start_point");
        }
        if approach.contains_key("exit_point") {
            legacy_exit = approach.remove("exit_point");
        }
        if approach.is_empty() {
            empty_approach = true;
        }
    }
    if empty_approach {
        patch.remove("approach");
    }
    if let Some(val) = legacy_start {
        if val.is_null() {
            let picks = patch.entry(String::from("picks")).or_insert_with(|| serde_json::json!({})).as_object_mut();
            if let Some(picks) = picks {
                picks.insert("start".into(), serde_json::json!([]));
            }
        } else if let Some(arr) = val.as_array() {
            let picks = patch.entry(String::from("picks")).or_insert_with(|| serde_json::json!({})).as_object_mut();
            if let Some(picks) = picks
                && !picks.contains_key("start")
            {
                picks.insert("start".into(), serde_json::json!([arr]));
            }
        } else {
            patch.insert(if key == "wire_contour" { "start".into() } else { "start_point".into() }, val);
        }
    }
    if let Some(val) = legacy_exit {
        if val.is_null() {
            let picks = patch.entry(String::from("picks")).or_insert_with(|| serde_json::json!({})).as_object_mut();
            if let Some(picks) = picks {
                picks.insert("exit".into(), serde_json::json!([]));
            }
        } else if let Some(arr) = val.as_array() {
            let picks = patch.entry(String::from("picks")).or_insert_with(|| serde_json::json!({})).as_object_mut();
            if let Some(picks) = picks
                && !picks.contains_key("exit")
            {
                picks.insert("exit".into(), serde_json::json!([arr]));
            }
        } else {
            patch.insert("exit_point".into(), val);
        }
    }
}
