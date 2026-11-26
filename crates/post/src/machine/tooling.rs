//! `tooling.spindle_mount` and `tooling.default_holder` (#828): the
//! holder system a machine's spindle takes, and the built-in library
//! holder a NEW tool on that machine starts with. Both optional - a file
//! without them reads as before. The holder library is not this crate's:
//! the caller passes its lookup in ([`HolderLookup`], the app's
//! `machine::holder_lookup`), so the post crate stays on cl and geometry
//! (maintainer, PR #880: no crate edge for a couple of items).
//!
//! Provenance: our own keys and rules, specced from the issue and the
//! manufacturers' public pages for which spindle a machine has (the
//! Tormach mills: https://www.tormach.com/tormach_mills/ - MX machines
//! BT30, PCNC and M machines an R8 spindle with TTS holders). One law at
//! two doors, as the plasma rules: the reader refuses the first broken
//! key, the Machines pane's validator lists them all, each BY NAME - an
//! unknown or mismatched holder is never read as "no default".

use serde_json::Value;

/// What the mount rule needs to know of a library holder: its own
/// (canonical) name and the taper it sits in (`None`: a straight-shank
/// chuck, no spindle mount of its own).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HolderInfo {
    pub name: String,
    pub taper: Option<String>,
}

/// The caller's holder library: a holder by name, or why there is none
/// (the refusal text, naming the holder).
pub type HolderLookup<'a> = &'a dyn Fn(&str) -> Result<HolderInfo, String>;

/// The mounts a machine file may state: the holder library's tapers.
/// `TTS` names the tooling system an R8 spindle takes on a Tormach
/// PCNC or M mill (the R8 collet grips the TTS shank).
pub const SPINDLE_MOUNTS: [&str; 4] = ["BT30", "BT40", "CAT40", "TTS"];

const MOUNT_KEY: &str = "tooling.spindle_mount";
const HOLDER_KEY: &str = "tooling.default_holder";

/// A stated value as a non-empty word: absent or null is unstated.
fn word(v: Option<&Value>) -> Result<Option<&str>, String> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(Some(s.trim())),
        Some(other) => Err(format!("{other} must be a name (leave the key out when unstated)")),
    }
}

fn mount(w: &str) -> Result<&'static str, String> {
    SPINDLE_MOUNTS
        .iter()
        .find(|m| m.eq_ignore_ascii_case(w))
        .copied()
        .ok_or_else(|| format!("\"{w}\" is not a spindle mount the holder library knows ({})", SPINDLE_MOUNTS.join(", ")))
}

/// A stated holder as the caller's library reads it; `Unresolved` when
/// the reader was given no library (a post-only reader that never lands
/// a holder keeps the word, and no mount rule can judge it).
enum Holder {
    Known(HolderInfo),
    Unresolved(String),
}

impl Holder {
    fn name(self) -> String {
        match self {
            Self::Known(h) => h.name,
            Self::Unresolved(w) => w,
        }
    }
}

fn holder(w: &str, lookup: Option<HolderLookup>) -> Result<Holder, String> {
    match lookup {
        Some(find) => find(w).map(Holder::Known),
        None => Ok(Holder::Unresolved(w.to_owned())),
    }
}

/// A stated holder must sit in the stated mount: its taper is the
/// mount, and a straight-shank chuck (no taper of its own) names none.
fn matches(mount: &str, h: &Holder) -> Result<(), String> {
    let Holder::Known(h) = h else { return Ok(()) };
    match h.taper.as_deref() {
        Some(t) if t == mount => Ok(()),
        Some(t) => Err(format!("\"{}\" is a {t} holder - it does not fit the stated {mount} spindle mount", h.name)),
        None => Err(format!("\"{}\" is a straight-shank chuck - it names no spindle mount, and the file states {mount}", h.name)),
    }
}

/// The file's two keys, each read on its own.
type Read = (Result<Option<&'static str>, String>, Result<Option<Holder>, String>);

fn read(v: &Value, lookup: Option<HolderLookup>) -> Read {
    let t = v.get("tooling");
    let m = word(t.and_then(|t| t.get("spindle_mount"))).and_then(|w| w.map(mount).transpose());
    let h = word(t.and_then(|t| t.get("default_holder"))).and_then(|w| w.map(|w| holder(w, lookup)).transpose());
    (m, h)
}

/// Every refusal the two keys earn, as (key, rule) - the validator's
/// list; the reader refuses on the first. `lookup` is the caller's
/// holder library; `None` judges the words only.
#[must_use]
pub fn tooling_refusals(v: &Value, lookup: Option<HolderLookup>) -> Vec<(String, String)> {
    let (m, h) = read(v, lookup);
    let mut out = Vec::new();
    if let Err(rule) = super::reservations::parse(v) {
        out.push(("tooling.reserved".to_owned(), rule));
    }
    if let Err(rule) = &m {
        out.push((MOUNT_KEY.to_owned(), rule.clone()));
    }
    if let Err(rule) = &h {
        out.push((HOLDER_KEY.to_owned(), rule.clone()));
    }
    if let (Ok(Some(m)), Ok(Some(h))) = (&m, &h)
        && let Err(rule) = matches(m, h)
    {
        out.push((HOLDER_KEY.to_owned(), rule));
    }
    out
}

/// The canonical (mount, holder name) a file states, or its first
/// refusal as `"key: rule"`.
pub(super) fn parse_tooling(v: &Value, lookup: Option<HolderLookup>) -> Result<(Option<String>, Option<String>), String> {
    if let Some((key, rule)) = tooling_refusals(v, lookup).into_iter().next() {
        return Err(format!("{key}: {rule}"));
    }
    let (m, h) = read(v, lookup);
    Ok((m.ok().flatten().map(str::to_owned), h.ok().flatten().map(Holder::name)))
}

/// Re-judge stored values (a project file's machine,
/// `Machine::tooling_refusal`): the same rules as a `.mch`'s keys.
pub fn mount_check(spindle_mount: Option<&str>, default_holder: Option<&str>, lookup: Option<HolderLookup>) -> Result<(), String> {
    let mut t = serde_json::Map::new();
    if let Some(m) = spindle_mount {
        t.insert("spindle_mount".into(), Value::String(m.to_owned()));
    }
    if let Some(h) = default_holder {
        t.insert("default_holder".into(), Value::String(h.to_owned()));
    }
    let v = serde_json::json!({ "tooling": Value::Object(t) });
    match tooling_refusals(&v, lookup).into_iter().next() {
        Some((key, rule)) => Err(format!("{key}: {rule}")),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "tooling_tests.rs"]
mod tests;
