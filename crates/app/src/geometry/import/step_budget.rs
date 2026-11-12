//! The STEP facet budget as a setting carried with an import (#1054): the
//! default (today's cap), the bounds a user-set budget must stay inside,
//! and the one refusal every door (GUI, script verb, MCP, project file)
//! says when it does not.
//!
//! Provenance: our own design (a bounded memory budget for a tessellator);
//! no vendor material consulted.

use super::MAX_FACETS;

/// The budget an import gets when none is set: the per-model facet cap
/// every STEP import ran under before #1054, so no default import changes.
pub const DEFAULT: u64 = MAX_FACETS as u64;
/// The smallest budget a user may set. Below a thousand facets a real part
/// cannot hold its own faces; the refinement schedule then only coarsens.
pub const MIN: u64 = 1_000;
/// The largest budget a user may set: twice the default, ~4.3 GiB of
/// 72-byte facets at its peak. Past it a typo (an extra zero) would let one
/// import exhaust the machine's memory instead of being refused.
pub const CEILING: u64 = 2 * DEFAULT;

/// What the setting is called, in every refusal and every label.
pub const NAME: &str = "STEP facet budget";

/// `budget` as the importer's facet cap, or the refusal by name when it is
/// outside [`MIN`]..=[`CEILING`].
pub fn check(budget: u64) -> Result<usize, String> {
    if !(MIN..=CEILING).contains(&budget) {
        return Err(format!("{NAME} {budget} is outside {MIN}..={CEILING} facets"));
    }
    usize::try_from(budget).map_err(|_| format!("{NAME} {budget} does not fit this machine's address size"))
}

/// An optional budget argument from the script verb or MCP: absent or null
/// is the default (`None`); anything but a whole number in range is refused
/// by name - a fraction, a negative or a string never rounds to a budget.
pub fn from_json(v: Option<&serde_json::Value>) -> Result<Option<u64>, String> {
    match v {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(n)) => match n.as_u64() {
            Some(b) => check(b).map(|_| Some(b)),
            None => Err(format!("{NAME} must be a whole number of facets in {MIN}..={CEILING}, got {n}")),
        },
        Some(other) => Err(format!("{NAME} must be a whole number of facets, got {other}")),
    }
}

/// The budget as it is recorded with a model: `None` for the default, so a
/// default import stores nothing and a project file saves as it always did.
#[must_use]
pub fn recorded(budget: Option<u64>) -> Option<u64> {
    budget.filter(|b| *b != DEFAULT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bounds_are_inclusive_and_the_refusal_names_the_setting() {
        assert_eq!(check(MIN), Ok(MIN as usize));
        assert_eq!(check(CEILING), Ok(CEILING as usize));
        for bad in [0, MIN - 1, CEILING + 1, u64::MAX] {
            let err = check(bad).expect_err("out of range");
            assert!(err.starts_with("STEP facet budget "), "{err}");
        }
    }

    #[test]
    fn json_takes_whole_numbers_only() {
        assert_eq!(from_json(None), Ok(None));
        assert_eq!(from_json(Some(&json!(null))), Ok(None));
        assert_eq!(from_json(Some(&json!(5000))), Ok(Some(5000)));
        for bad in [json!(5000.5), json!(-1), json!("5000"), json!(1e12), json!(10)] {
            assert!(from_json(Some(&bad)).expect_err("refused").contains(NAME), "{bad}");
        }
    }

    #[test]
    fn the_default_records_nothing() {
        assert_eq!(recorded(Some(DEFAULT)), None);
        assert_eq!(recorded(None), None);
        assert_eq!(recorded(Some(5000)), Some(5000));
    }
}
