//! The `.mch` part tree kept whole (#2583): each part of
//! `kinematics.default.parts` with its parent, so that a machine model
//! can be built from the file. `parse_mch` flattens the same tree into
//! `Machine.axes`; this reader keeps the chain and changes nothing there.
//!
//! Provenance: our own reader of our own machine-file grammar
//! (`assets/machines/AGENTS.md`); a serial chain of table, slides, rotaries
//! and head is general machine-tool practice (ISO 841 axis nomenclature).

use serde_json::Value;

use super::direction::AxisDirection;
use super::parse::{mm_factor, rate, scale};

/// What a part is: the `type` word of the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind {
    Table,
    Head,
    Linear,
    Rotary,
}

impl PartKind {
    fn from_word(w: &str) -> Option<Self> {
        match w {
            "table" => Some(Self::Table),
            "head" => Some(Self::Head),
            "linear" => Some(Self::Linear),
            "rotary" => Some(Self::Rotary),
            _ => None,
        }
    }
}

/// Which side of the machine an axis moves: the tool (the axis is on
/// the path from a root to a `head` part) or the table (all others).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovingSide {
    Tool,
    Table,
}

/// The spindle body a head states, in mm. An unstated number is `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpindleBody {
    pub diameter: Option<f64>,
    pub length: Option<f64>,
}

/// The tool station limits a head states, in mm.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToolStation {
    pub max_tool_length: Option<f64>,
    pub max_tool_diameter: Option<f64>,
}

/// One part of the tree with the facts the file states. Lengths are in
/// mm; angles are in degrees.
#[derive(Clone, Debug, PartialEq)]
pub struct TreePart {
    pub id: String,
    /// The `name` of the part; a part that states none takes its `id`.
    pub name: String,
    pub kind: PartKind,
    /// The index of the parent in [`MachineTree::parts`]; `None` on a root.
    pub parent: Option<usize>,
    /// `travel` of a linear or rotary part: `(min, max)`.
    pub travel: Option<(f64, f64)>,
    /// `axis`: the direction vector of the part.
    pub axis: Option<[f64; 3]>,
    /// `pivot` of a rotary part, in machine coordinates.
    pub pivot: Option<[f64; 3]>,
    /// `offset` of a child rotary from its parent rotary.
    pub offset: Option<[f64; 3]>,
    /// `direction` of a linear part (#373).
    pub direction: AxisDirection,
    /// `spindle` body of a head part; `None` where no `spindle` is stated.
    pub spindle_body: Option<SpindleBody>,
    /// `tool_station` of a head part; `None` where none is stated.
    pub tool_station: Option<ToolStation>,
    /// The side a linear or rotary part moves. `None` on a table or head
    /// part, and on each part of a tree that states no head.
    pub moving_side: Option<MovingSide>,
}

/// The part tree of a machine file, depth-first: a parent is always
/// ahead of its children, and siblings keep the order of the file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MachineTree {
    pub parts: Vec<TreePart>,
}

impl MachineTree {
    /// The index of the part with this `id`.
    #[must_use]
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.parts.iter().position(|p| p.id == id)
    }

    /// The part with this `id`.
    #[must_use]
    pub fn part(&self, id: &str) -> Option<&TreePart> {
        self.parts.iter().find(|p| p.id == id)
    }

    /// The children of part `index`, in the order of the file.
    pub fn children(&self, index: usize) -> impl Iterator<Item = &TreePart> {
        self.parts.iter().filter(move |p| p.parent == Some(index))
    }

    /// The parts from the root down to part `index`, the part itself
    /// last. Empty for an index that is not in the tree.
    #[must_use]
    pub fn chain(&self, index: usize) -> Vec<&TreePart> {
        let mut out = Vec::new();
        let mut at = Some(index);
        // A parent index is always lower than its child (`walk`), so the
        // climb ends; the length bound guards a hand-built tree.
        while let Some(part) = at.and_then(|i| self.parts.get(i)) {
            if out.len() > self.parts.len() {
                break;
            }
            out.push(part);
            at = part.parent;
        }
        out.reverse();
        out
    }

    /// The first `head` part, depth-first (the one `parse_mch` reads).
    #[must_use]
    pub fn head(&self) -> Option<&TreePart> {
        self.parts.iter().find(|p| p.kind == PartKind::Head)
    }
}

/// Read the part tree of a `.mch` machine file. A file that states no
/// `kinematics.default.parts` (the mirror form) gives an empty tree. A
/// malformed tree refuses, and the message names the part id and the key.
pub fn parse_mch_tree(text: &str) -> Result<MachineTree, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("machine file: {e}"))?;
    if !v.is_object() {
        return Err("machine file: expected a machine object".to_owned());
    }
    let mut tree = MachineTree::default();
    let list = match v.pointer("/kinematics/default/parts") {
        None | Some(Value::Null) => return Ok(tree),
        Some(Value::Array(list)) => list,
        Some(other) => return Err(format!("machine file: kinematics.default.parts must be a list, not {other}")),
    };
    let f = mm_factor(v.pointer("/kinematics/default/units")).unwrap_or(1.0);
    walk(list, None, "kinematics.default.parts", f, &mut tree.parts)?;
    mark_moving_sides(&mut tree.parts);
    Ok(tree)
}

/// Read `list` into `out`, depth-first. `at` names the list in a refusal.
/// serde_json bounds the nesting depth of the text, so the recursion ends.
fn walk(list: &[Value], parent: Option<usize>, at: &str, f: f64, out: &mut Vec<TreePart>) -> Result<(), String> {
    for (n, p) in list.iter().enumerate() {
        if !p.is_object() {
            return Err(format!("machine file: {at}[{n}] must be a part object, not {p}"));
        }
        let id = part_id(p, at, n)?;
        if out.iter().any(|seen| seen.id == id) {
            return Err(format!("machine file: part {id}: id is stated by two parts"));
        }
        let ty = p.get("type").unwrap_or(&Value::Null);
        let kind = ty.as_str().and_then(PartKind::from_word).ok_or_else(|| {
            format!("machine file: part {id}: type must be \"table\", \"head\", \"linear\" or \"rotary\", not {ty}")
        })?;
        let name = match p.get("name") {
            None | Some(Value::Null) => id.clone(),
            Some(Value::String(s)) if !s.is_empty() => s.clone(),
            Some(other) => return Err(format!("machine file: part {id}: name must be a word, not {other}")),
        };
        let rotary = kind == PartKind::Rotary;
        // The trunnion reader owns `pivot` and `offset` (#1962): it scales
        // them and refuses them on a part that is not a rotary.
        let tr = super::trunnion::parse_axis_fields(&id, p, rotary, f)
            .map_err(|(key, why)| format!("machine file: part {key}: {why}"))?;
        let index = out.len();
        out.push(TreePart {
            travel: travel(p, if rotary { 1.0 } else { f }),
            axis: axis(&id, p, rotary)?,
            pivot: tr.pivot,
            offset: tr.offset,
            direction: direction(&id, p, rotary)?,
            spindle_body: head_object(&id, p, "spindle")?
                .map(|s| {
                    Ok::<_, String>(SpindleBody {
                        diameter: head_length(&id, s, "body_diameter", f)?,
                        length: head_length(&id, s, "body_length", f)?,
                    })
                })
                .transpose()?,
            tool_station: head_object(&id, p, "tool_station")?
                .map(|s| {
                    Ok::<_, String>(ToolStation {
                        max_tool_length: head_length(&id, s, "max_tool_length", f)?,
                        max_tool_diameter: head_length(&id, s, "max_tool_diameter", f)?,
                    })
                })
                .transpose()?,
            moving_side: None,
            id: id.clone(),
            name,
            kind,
            parent,
        });
        match p.get("parts") {
            None | Some(Value::Null) => {}
            Some(Value::Array(kids)) => walk(kids, Some(index), &format!("part {id}: parts"), f, out)?,
            Some(other) => return Err(format!("machine file: part {id}: parts must be a list, not {other}")),
        }
    }
    Ok(())
}

/// Preserve unstated limits, but refuse a length that overflows in mm.
fn head_length(id: &str, object: &Value, key: &str, f: f64) -> Result<Option<f64>, String> {
    let length = scale(rate(object.get(key)), f);
    if length.is_some_and(|value| !value.is_finite()) {
        return Err(format!(
            "machine file: part {id}: {key} must be finite after conversion to mm"
        ));
    }
    Ok(length)
}

/// The `id` of a part: a word that is not empty. A part with no id is
/// named by its `name`, or by its place in the list.
fn part_id(p: &Value, at: &str, n: usize) -> Result<String, String> {
    match p.get("id") {
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.clone()),
        other => {
            let who = match p.get("name").and_then(Value::as_str) {
                Some(name) if !name.is_empty() => format!("part {name}"),
                _ => format!("{at}[{n}]"),
            };
            let got = other.map_or_else(|| "nothing".to_owned(), Value::to_string);
            Err(format!("machine file: {who}: id must be a word, not {got}"))
        }
    }
}

/// `travel` by the unstated-limit law of `parse_mch`: two finite numbers
/// with `max > min`, or the field reads as not stated.
fn travel(p: &Value, f: f64) -> Option<(f64, f64)> {
    let t = p.get("travel")?;
    let lo = t.get("min")?.as_f64()? * f;
    let hi = t.get("max")?.as_f64()? * f;
    (lo.is_finite() && hi.is_finite() && hi > lo).then_some((lo, hi))
}

/// `axis`: three finite numbers, and not the zero vector on a rotary.
fn axis(id: &str, p: &Value, rotary: bool) -> Result<Option<[f64; 3]>, String> {
    let v = match p.get("axis") {
        None | Some(Value::Null) => return Ok(None),
        Some(v) => v,
    };
    let num = |i: usize| v.get(i).and_then(Value::as_f64).filter(|x| x.is_finite());
    let three = v.as_array().is_some_and(|a| a.len() == 3);
    match (three, num(0), num(1), num(2)) {
        (true, Some(x), Some(y), Some(z)) if rotary && x == 0.0 && y == 0.0 && z == 0.0 => {
            Err(format!("machine file: part {id}: axis states the zero direction"))
        }
        (true, Some(x), Some(y), Some(z)) => Ok(Some([x, y, z])),
        _ => Err(format!("machine file: part {id}: axis must be three finite numbers, not {v}")),
    }
}

/// `direction` by the law of `parse_mch` (#373): a linear part states
/// `1` or `-1`; a rotary part that states `-1` refuses.
fn direction(id: &str, p: &Value, rotary: bool) -> Result<AxisDirection, String> {
    let v = match p.get("direction") {
        None | Some(Value::Null) => return Ok(AxisDirection::Standard),
        Some(v) => v,
    };
    let sign = v.as_f64().and_then(AxisDirection::from_sign);
    if rotary {
        return match sign {
            Some(AxisDirection::Inverted) => Err(format!(
                "machine file: part {id}: direction -1 on a rotary part is not supported"
            )),
            _ => Ok(AxisDirection::Standard),
        };
    }
    sign.ok_or_else(|| format!("machine file: part {id}: direction must be +1 or -1, not {v}"))
}

/// The `spindle` or `tool_station` object of a head part. `None` where
/// the key is absent or null, and on a part that is not a head.
fn head_object<'a>(id: &str, p: &'a Value, key: &str) -> Result<Option<&'a Value>, String> {
    if p.get("type").and_then(Value::as_str) != Some("head") {
        return Ok(None);
    }
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) if v.is_object() => Ok(Some(v)),
        Some(other) => Err(format!("machine file: part {id}: {key} must be an object, not {other}")),
    }
}

/// Set the moving side of each linear and rotary part: the tool side for
/// an ancestor of a head, the table side for the others. A tree with no
/// head states no side.
fn mark_moving_sides(parts: &mut [TreePart]) {
    let mut carries_head = vec![false; parts.len()];
    let mut any_head = false;
    for i in 0..parts.len() {
        if parts[i].kind != PartKind::Head {
            continue;
        }
        any_head = true;
        let mut at = parts[i].parent;
        // A parent index is lower than its child, so the climb ends.
        while let Some(up) = at.filter(|up| *up < parts.len()) {
            carries_head[up] = true;
            at = parts[up].parent.filter(|next| *next < up);
        }
    }
    if !any_head {
        return;
    }
    for (part, carries) in parts.iter_mut().zip(carries_head) {
        if matches!(part.kind, PartKind::Linear | PartKind::Rotary) {
            part.moving_side = Some(if carries { MovingSide::Tool } else { MovingSide::Table });
        }
    }
}
