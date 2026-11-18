//! Each setup's post options (#389): skip pauses and homes, one file per
//! operation and the thread output, owned by a setup or followed from the
//! document's - the same own / follow-the-document law as the setup's
//! post (#343, `crate::setup_mount`). A setup that owns none (`None`,
//! every earlier file) reads the document's `PostOptions` singleton, so
//! an untouched document posts byte for byte as before.
//!
//! ## Which options a program posts with
//!
//! - "Skip pauses and homes" is an operation-level effect (it mutes each
//!   operation's after-action), so each operation reads its OWN setup's
//!   choice ([`of`]) - a setup that skips posts without its stops while
//!   the next one keeps them.
//! - The Output (one file per operation), the thread output and the End
//!   units (the unit word the program ends in, #596) shape the whole
//!   program, so the program posts with the ACTIVE setup's options
//!   ([`in_force`]) - the one-mount law of the post itself - and a program
//!   holding a setup that disagrees on any of them is refused by name
//!   ([`program_problems`]), never posted half one way (#1540). The
//!   program's own unit is not a post option at all: it is the
//!   document's one mm / inch setting (#902, `crate::posting_units`).
//!
//! Provenance: our own design (the #343 mount law restated for the post
//! options); no vendor implementation inspected.

use std::collections::{BTreeMap, BTreeSet};

use camcraft_post::post::options::{self, OptionValue};
use flecs_ecs::core::WorldGet;
use flecs_ecs::prelude::World;

use crate::model;
use crate::posting;
use crate::schema::{BetweenSetups, PostOptions, PostRef, PostScriptOptions};
use crate::setup;
use crate::setup_mount;

/// The document's post options as stored (the singleton), what setups
/// that own none follow. The default when none was ever set.
#[must_use]
pub fn document(world: &World) -> PostOptions {
    world.try_get::<&PostOptions>(|p| *p).unwrap_or_default()
}

/// The setup's own options, `None` when it follows the document. `None`
/// as the declared context resolves to the first setup (implicit-setup
/// law); a document without setups owns none.
#[must_use]
pub fn own(world: &World, declared: Option<&str>) -> Option<PostOptions> {
    let name = setup::effective(world, declared)?;
    setup::def_of(world, &name)?.post_options
}

/// The options a declared setup context posts with: its own, else the
/// document's.
#[must_use]
pub fn of(world: &World, declared: Option<&str>) -> PostOptions {
    own(world, declared).unwrap_or_else(|| document(world))
}

/// The options the program posts with: the active setup's.
#[must_use]
pub fn in_force(world: &World) -> PostOptions {
    of(world, setup::active(world).as_deref())
}

/// The program-level clause: every unsuppressed operation's setup must
/// Canonical map key for per-post script options (#258).
///
/// Builtin Fanuc whether referenced as [`PostRef::Builtin`] or
/// [`PostRef::Named("Fanuc")`] shares the same `"builtin:Fanuc"` key so
/// stored user preferences never split across equivalent refs.
#[must_use]
pub fn post_key(post: &PostRef) -> String {
    match post {
        PostRef::Builtin => {
            let name = posting::post_name(post).unwrap_or("Fanuc");
            format!("builtin:{name}")
        }
        PostRef::Named(name) => format!("builtin:{name}"),
        PostRef::File(path) => format!("file:{path}"),
    }
}

/// Document-level per-post script option overrides (#258).
#[must_use]
pub fn script_document(world: &World) -> PostScriptOptions {
    world
        .try_get::<&PostScriptOptions>(|p| p.clone())
        .unwrap_or_default()
}

/// Replace the document-level per-post script option overrides (#258).
pub fn script_document_set(world: &World, mut next: PostScriptOptions) {
    next.normalize();
    world.set(next);
}

/// The named setup's own script option overrides, or `None` when it
/// inherits from the document (#258).
#[must_use]
pub fn script_own(world: &World, declared: Option<&str>) -> Option<PostScriptOptions> {
    let name = declared.map(str::to_owned).or_else(|| setup::first(world))?;
    setup::def_of(world, &name).and_then(|d| d.post_properties)
}

/// Effective script option overrides for `declared` setup (own, or
/// falling back to the document) (#258).
#[must_use]
pub fn script_of(world: &World, declared: Option<&str>) -> PostScriptOptions {
    script_own(world, declared).unwrap_or_else(|| script_document(world))
}

/// True when two [`PostRef`] values address the same post script (#2190).
///
/// Builtin Fanuc matches `Named("Fanuc")` via [`post_key`]; two `File` refs
/// also match when both paths canonicalise to the same file on disk.
#[must_use]
pub fn same_post(a: &PostRef, b: &PostRef) -> bool {
    if post_key(a) == post_key(b) {
        return true;
    }
    if let (PostRef::File(pa), PostRef::File(pb)) = (a, b)
        && let (Ok(ca), Ok(cb)) = (std::fs::canonicalize(pa), std::fs::canonicalize(pb))
    {
        return ca == cb;
    }
    false
}

/// Effective overrides for a specific `post` under `declared` setup (#258).
#[must_use]
pub fn script_for_post(
    world: &World,
    declared: Option<&str>,
    post: &PostRef,
) -> BTreeMap<String, OptionValue> {
    let all = script_of(world, declared);
    let exact = all.for_post(&post_key(post));
    if !exact.is_empty() {
        return exact.clone();
    }
    if let PostRef::File(path) = post
        && let Ok(canon) = std::fs::canonicalize(path)
    {
        for (k, v) in &all.by_post {
            if let Some(other) = k.strip_prefix("file:")
                && std::fs::canonicalize(other).is_ok_and(|c| c == canon)
            {
                return v.clone();
            }
        }
    }
    BTreeMap::new()
}

struct Unapplied<'a> {
    setup: Option<&'a str>,
    from_doc: bool,
    post: PostRef,
    keys: Vec<String>,
    combined_into: Option<&'a str>,
}

/// Warning when posting through `chosen` left stored `script_options` of the
/// document or a setup's own post unapplied (#2190). Returns `None` when every
/// configured post's stored options applied.
#[must_use]
pub fn unapplied_script_warning(
    world: &World,
    mount_setups: &[Vec<String>],
    chosen: &PostRef,
) -> Option<String> {
    let total_setups: usize = mount_setups.iter().map(Vec::len).sum();
    let mut entries: Vec<Unapplied<'_>> = Vec::new();
    for group in mount_setups {
        let mount_setup = group.first().map(String::as_str);
        let applied = script_for_post(world, mount_setup, chosen);
        for s in group {
            let declared = (!s.is_empty()).then_some(s.as_str());
            let own_post = setup_mount::post_of(world, declared);
            let own_stored = script_for_post(world, declared, &own_post);
            if own_stored.is_empty() {
                continue;
            }
            let same = same_post(&own_post, chosen);
            let (keys, combined_into) = if !same {
                (own_stored.keys().cloned().collect(), None)
            } else if mount_setup == Some(s.as_str()) {
                continue;
            } else {
                let diff: Vec<String> = own_stored
                    .iter()
                    .filter(|(k, v)| applied.get(*k) != Some(*v))
                    .map(|(k, _)| k.clone())
                    .collect();
                if diff.is_empty() {
                    continue;
                }
                (diff, mount_setup.filter(|n| !n.is_empty()))
            };
            let from_doc =
                setup_mount::own_post(world, declared).is_none() && script_own(world, declared).is_none();
            entries.push(Unapplied {
                setup: declared,
                from_doc,
                post: own_post,
                keys,
                combined_into,
            });
        }
    }
    if entries.is_empty() {
        return None;
    }
    let chosen_label = posting::post_ref_label(chosen);
    let format_entry = |e: &Unapplied<'_>, with_setup: bool| -> String {
        let own_label = posting::post_ref_label(&e.post);
        let prefix = match e.setup {
            Some(name) if with_setup => format!("setup '{name}': "),
            _ => String::new(),
        };
        let reason = match e.combined_into {
            Some(first) => format!("because the program posted one file using setup '{first}'s options"),
            None => format!("because the program posted with {chosen_label}"),
        };
        format!(
            "{prefix}stored script_options for {own_label} ({}) were not applied {reason}",
            e.keys.join(", ")
        )
    };
    let all_same_doc = entries.len() == total_setups
        && entries.iter().all(|e| {
            e.from_doc
                && e.combined_into.is_none()
                && same_post(&e.post, &entries[0].post)
                && e.keys == entries[0].keys
        });
    if all_same_doc || total_setups <= 1 {
        return Some(format_entry(&entries[0], false));
    }
    let lines: Vec<String> = entries.iter().map(|e| format_entry(e, true)).collect();
    Some(lines.join("; "))
}

/// Effective overrides in force for the active setup and its post (#258).
#[must_use]
pub fn script_in_force(world: &World) -> BTreeMap<String, OptionValue> {
    let active = setup::active(world);
    let post = setup_mount::post_of(world, active.as_deref());
    script_for_post(world, active.as_deref(), &post)
}

fn differing_script_keys(
    post: &PostRef,
    a: &BTreeMap<String, OptionValue>,
    b: &BTreeMap<String, OptionValue>,
) -> Vec<String> {
    if let Ok((_, src)) = posting::script_of(post)
        && let Ok(schema) = options::declared_options(&src)
        && let (Ok(ra), Ok(rh)) = (
            options::resolve_options(&schema, a),
            options::resolve_options(&schema, b),
        )
    {
        return schema
            .iter()
            .filter(|decl| ra.get(&decl.key) != rh.get(&decl.key))
            .map(|decl| decl.key.clone())
            .collect();
    }
    let keys: BTreeSet<&str> = a.keys().chain(b.keys()).map(String::as_str).collect();
    keys.into_iter()
        .filter(|k| a.get(*k) != b.get(*k))
        .map(str::to_owned)
        .collect()
}

fn script_disagreement_problem(
    world: &World,
    active: &str,
    want: &PostOptions,
    here: &str,
) -> Option<String> {
    if want.per_op || want.between_setups == BetweenSetups::FilePerSetup {
        return None;
    }
    let active_post = setup_mount::post_of(world, Some(active));
    let here_post = setup_mount::post_of(world, Some(here));
    if post_key(&active_post) != post_key(&here_post) {
        return None;
    }
    let active_map = script_for_post(world, Some(active), &active_post);
    let here_map = script_for_post(world, Some(here), &here_post);
    let differ = differing_script_keys(&active_post, &active_map, &here_map);
    if differ.is_empty() {
        return None;
    }
    Some(format!(
        "setup '{here}' posts with other post script options ({}) than the active setup '{active}' - post the setups one at a time",
        differ.join(", ")
    ))
}

/// Every non-active setup that disagrees with the active setup. Each
/// disagreeing setup is named once. Empty whenever no setup owns its options.
#[must_use]
pub fn program_problems(world: &World) -> Vec<String> {
    let Some(active) = setup::active(world) else {
        return Vec::new();
    };
    let want = of(world, Some(&active));
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (_, e) in setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        if op.suppressed {
            continue;
        }
        let Some(here) = setup::effective(world, op.setup.as_deref()) else {
            continue;
        };
        if seen.contains(&here) {
            continue;
        }
        let has = of(world, Some(&here));
        let differ = differing(&has, &want);
        if !differ.is_empty() {
            out.push(format!(
                "setup '{here}' posts with other post options ({}) than the active setup '{active}' - post the setups one at a time",
                differ.join(", ")
            ));
        }
        if let Some(msg) = script_disagreement_problem(world, &active, &want, &here) {
            out.push(msg);
        }
        seen.push(here);
    }
    out
}

/// The program-wide options on which `has` differs from `want`, by the
/// names the post-options page shows: the Output, the thread output and
/// the End units (#1540 - the one End-units word of the program, never
/// whichever setup happens to be active).
fn differing(has: &PostOptions, want: &PostOptions) -> Vec<&'static str> {
    let mut out = Vec::new();
    if has.per_op != want.per_op {
        out.push("output");
    }
    if has.threading != want.threading {
        out.push("threading");
    }
    if has.end_units != want.end_units {
        out.push("end units");
    }
    if has.between_setups != want.between_setups {
        out.push("between setups");
    }
    out
}
