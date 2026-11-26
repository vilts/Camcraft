//! `controller.post` (#2374): the registry post a machine file names as
//! the one that writes its programs. Optional - a file without it reads
//! as before. The reader checks only its shape (a non-empty name); the
//! registry is the app's, so whether the name is a built-in post and
//! serves the machine's kind is the app validator's rule, and the Setup
//! page only ever offers the post - a mount never changes it.
//!
//! Provenance: our own key and rule, specced from the issue.

use serde_json::Value;

/// The post name `controller.post` states, trimmed. Absent or `null`
/// reads `None`; anything but a non-empty string is refused by the
/// key's name, as the reader refuses every other malformed key.
pub fn parse_post_name(v: &Value) -> Result<Option<String>, String> {
    match v.pointer("/controller/post") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(Some(s.trim().to_owned())),
        Some(x) => Err(post_name_rule(x)),
    }
}

/// The rule a malformed `controller.post` breaks, worded for both doors
/// (the reader prefixes the key; the validator shows it at the key).
#[must_use]
pub fn post_name_rule(x: &Value) -> String {
    format!("{x} must be the name of a post")
}

/// The validator's rule for a stated post, or `None` when it stands.
/// The registry is the caller's (the app's): `known(name)` is `None`
/// for a name it does not hold, else whether that post serves the
/// machine's kind - the holder lookup's shape (#828), no crate edge.
pub fn post_name_refusal(v: &Value, kind_word: &str, known: impl Fn(&str) -> Option<bool>) -> Option<String> {
    let name = match parse_post_name(v) {
        Ok(Some(name)) => name,
        Ok(None) => return None,
        Err(rule) => return Some(rule),
    };
    match known(&name) {
        None => Some(format!("{name} is not a built-in post")),
        Some(false) => Some(format!("{name} does not serve a {kind_word} machine")),
        Some(true) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_stated_post_is_read_trimmed() {
        let v = json!({ "controller": { "post": " grblHAL " } });
        assert_eq!(parse_post_name(&v), Ok(Some("grblHAL".to_owned())));
    }

    #[test]
    fn an_unstated_post_reads_none() {
        assert_eq!(parse_post_name(&json!({})), Ok(None));
        assert_eq!(parse_post_name(&json!({ "controller": {} })), Ok(None));
        assert_eq!(parse_post_name(&json!({ "controller": { "post": null } })), Ok(None));
    }

    #[test]
    fn the_reader_carries_the_post_and_refuses_a_bad_one_by_name() {
        let base = include_str!("../../../../assets/machines/vmc.mch");
        let plain = crate::machine::parse_mch(base).expect("the shipped file parses");
        assert_eq!(plain.post, None, "a file without the key states no post");
        let named = base.replacen("\"controller\": {", "\"controller\": { \"post\": \"grblHAL\",", 1);
        assert_eq!(crate::machine::parse_mch(&named).expect("parses").post.as_deref(), Some("grblHAL"));
        let bad = base.replacen("\"controller\": {", "\"controller\": { \"post\": 7,", 1);
        let err = crate::machine::parse_mch(&bad).expect_err("a number is not a post name");
        assert!(err.contains("controller.post"), "{err}");
    }

    #[test]
    fn the_validator_rule_asks_the_callers_registry() {
        let v = |p: &str| json!({ "controller": { "post": p } });
        let registry = |n: &str| match n {
            "Mill post" => Some(true),
            "Lathe post" => Some(false),
            _ => None,
        };
        assert_eq!(post_name_refusal(&v("Mill post"), "mill", registry), None);
        assert_eq!(post_name_refusal(&json!({}), "mill", registry), None);
        let lathe = post_name_refusal(&v("Lathe post"), "mill", registry);
        assert_eq!(lathe.as_deref(), Some("Lathe post does not serve a mill machine"));
        let stranger = post_name_refusal(&v("Other"), "mill", registry);
        assert_eq!(stranger.as_deref(), Some("Other is not a built-in post"));
        assert!(post_name_refusal(&v(""), "mill", registry).is_some_and(|r| r.contains("name of a post")));
    }

    #[test]
    fn a_post_that_is_not_a_name_is_refused() {
        for bad in [json!(""), json!("  "), json!(3), json!(true), json!(["grblHAL"])] {
            let v = json!({ "controller": { "post": bad } });
            let err = parse_post_name(&v).expect_err("not a name");
            assert!(err.contains("must be the name of a post"), "{err}");
        }
    }
}
