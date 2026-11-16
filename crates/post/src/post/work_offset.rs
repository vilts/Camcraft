//! Work offsets as an index (#931): the stream's `PARAM WorkOffset` is
//! a 1-based number, and the post supplies the control's word for it.
//!
//! Provenance: our own design (issue #931), on the manifest-flag pattern
//! of `coolantModes` (#150); the words `G54`..`G59` for offsets 1-6 are
//! the six work coordinate systems of RS-274 / ISO 6983.
//!
//! A script that declares `workOffsetIndex = true` receives the number
//! and maps it to its own word (`workOffsetName(n)`), up to the maximum
//! it declares (`workOffsetMax`, 6 when it declares none). A script
//! without the flag receives the word `G54`..`G59` for offsets 1-6
//! exactly as before, and is refused by setup name for any offset above
//! 6 - never handed a word it would misread, never posted at offset 1.

use super::js::{is_true, Script};
use super::PostError;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The words an unflagged script reads, offsets 1..=6 (index 1 = G54).
pub const LEGACY_WORDS: [&str; 6] = ["G54", "G55", "G56", "G57", "G58", "G59"];

/// The maximum of a script that declares none: the six legacy words.
pub const DEFAULT_MAX: u32 = 6;

/// The sanity bound on any offset index or declared maximum (the app
/// keeps a setup's offset in a `u8`).
pub const MAX_BOUND: u32 = 255;

/// The 1-based offset index a `PARAM WorkOffset` value names: a whole
/// number `1..=MAX_BOUND`, or - so streams written before #931 still
/// read - one of the legacy words `G54`..`G59`. Anything else is None.
#[must_use]
pub fn index_of(value: &Value) -> Option<u32> {
    let n = match value {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => {
            let t = s.trim();
            if let Some(i) = LEGACY_WORDS.iter().position(|w| w.eq_ignore_ascii_case(t)) {
                return u32::try_from(i + 1).ok();
            }
            t.parse::<f64>().ok()?
        }
        _ => return None,
    };
    let whole = n.is_finite() && n.fract() == 0.0 && n >= 1.0 && n <= f64::from(MAX_BOUND);
    // A whole number in 1..=255 converts exactly.
    whole.then_some(n as u32)
}

/// The legacy word of offset `index` (1 = G54), for 1..=6 only.
#[must_use]
pub fn legacy_word(index: u32) -> Option<&'static str> {
    let i = usize::try_from(index.checked_sub(1)?).ok()?;
    LEGACY_WORDS.get(i).copied()
}

/// The work-offset half of a script's manifest, read once at load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// `workOffsetIndex = true`: the script reads the 1-based number.
    pub indexed: bool,
    /// The highest offset the script can express (`workOffsetMax`).
    pub max: u32,
}

impl Manifest {
    pub(super) fn read(s: &mut Script) -> Self {
        let indexed = s
            .eval_src("typeof workOffsetIndex !== \"undefined\" && workOffsetIndex === true")
            .map(|v| is_true(&v))
            .unwrap_or(false);
        let declared = s
            .eval_src("(typeof workOffsetMax === \"number\") ? workOffsetMax : null")
            .ok()
            .and_then(|v| v.as_number())
            .and_then(|n| index_of(&json!(n)));
        // An unflagged script is held to the six words it understands,
        // whatever maximum it states.
        let max = if indexed { declared.unwrap_or(DEFAULT_MAX) } else { DEFAULT_MAX };
        Self { indexed, max }
    }

    /// What the script receives for one `PARAM WorkOffset`, or the
    /// refusal (a counted warning, naming the setup) when the offset is
    /// no index or one this script cannot express.
    pub(super) fn translate(&self, value: &Value, base: &BTreeMap<String, Value>) -> Result<Value, String> {
        let who = who(base);
        let Some(n) = index_of(value) else {
            let shown = value.as_str().map_or_else(|| value.to_string(), str::to_owned);
            return Err(format!(
                "{who}work offset {shown} is not a work offset number - PARAM WorkOffset is 1-based (1 is the first offset)"
            ));
        };
        if !self.indexed {
            return legacy_word(n).map(|w| json!(w)).ok_or_else(|| {
                format!(
                    "{who}uses work offset {n}, but this post script does not declare workOffsetIndex, so it can only post offsets 1-6 - pick offset 1-6 or use a post that reads the offset number"
                )
            });
        }
        if n > self.max {
            return Err(format!(
                "{who}uses work offset {n}, but this post expresses work offsets 1-{} only - pick one of those or use a post for a control with more offsets",
                self.max
            ));
        }
        Ok(json!(n))
    }
}

fn who(base: &BTreeMap<String, Value>) -> String {
    let named = |k: &str| base.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned);
    if let Some(s) = named("SetupName") {
        format!("setup {s}: ")
    } else if let Some(op) = named("OperationName") {
        format!("operation {op}: ")
    } else {
        String::new()
    }
}

/// A script's work offsets, for display (the setup combo, the sheet):
/// the highest offset it can post and what the control calls each.
/// Display only - it never feeds the stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkOffsets {
    pub indexed: bool,
    pub max: u32,
    /// `names[i]` names offset `i + 1`; an empty name means the script
    /// gives none (shown as "Work offset n").
    pub names: Vec<String>,
}

/// Load `cps` and read its work-offset manifest and names. An
/// unflagged script's names are the legacy words it reads.
pub fn work_offsets(cps: &str) -> Result<WorkOffsets, PostError> {
    let mut s = Script::load(cps)?;
    let m = Manifest::read(&mut s);
    let has_names = s.eval_src("typeof workOffsetName === \"function\"").map(|v| is_true(&v)).unwrap_or(false);
    let mut names = Vec::new();
    for n in 1..=m.max {
        let name = if !m.indexed {
            legacy_word(n).unwrap_or_default().to_owned()
        } else if has_names {
            let v = s.eval_src(&format!("String(workOffsetName({n}) || \"\")"))?;
            v.as_string().map(|t| t.to_std_string_escaped()).unwrap_or_default()
        } else {
            String::new()
        };
        names.push(name);
    }
    Ok(WorkOffsets { indexed: m.indexed, max: m.max, names })
}
