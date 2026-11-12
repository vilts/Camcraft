//! #661: the structural pins of the icon tables, split out of
//! structural.rs (which sits at the 400-line law): every operation kind
//! and every tool kind wears its own SVG-mastered glyph, and the tree rows,
//! status badges and actions are SVG-mastered too.
//! Provenance: our own test; no HSMWorks material involved.

use super::*;

/// #661: every operation kind wears its own glyph - an `op-*` PNG drawn
/// from an SVG master in assets/ui/svg/ - and no two kinds share one (the
/// toolbar's Create cells and the tree's op rows tell kinds apart by it).
/// The pixels themselves are pinned against their masters by xtask's
/// `committed_icon_pngs_match_their_svg_masters`.
#[test]
fn every_op_kind_has_its_own_svg_mastered_icon() {
    use std::collections::HashSet;
    let svg = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/ui/svg");
    let mut seen = HashSet::new();
    for k in camcraft_app::operation::KindInfo::ALL {
        let icon = crate::egui_app::op_icons::op_icon(k.ty);
        assert!(seen.insert(icon), "{:?} shares {icon:?} with another kind", k.ty);
        let file = icon.file();
        assert!(file.starts_with("op-"), "{:?} wears the stand-in {file}.png", k.ty);
        assert!(svg.join(format!("{file}.svg")).is_file(), "{file}.png has no SVG master");
    }
    // No two slots of the table load the same PNG either.
    let files: HashSet<&str> = Icon::ALL.iter().map(|i| i.file()).collect();
    assert_eq!(files.len(), Icon::ALL.len(), "two icon slots share a PNG");
}

/// Every tool kind the editor can set, read from the editor's own kind
/// numbering (`ToolUi::tool_kind`, exhaustive in `from_tool`) - not a hand
/// list: a kind the numbering gains is enumerated here with it.
fn every_tool_kind() -> Vec<camcraft_app::tools::ToolKind> {
    let mut ui = crate::state::ToolUi::default();
    let mut kinds = Vec::new();
    for i in 0..64 {
        ui.kind = i;
        let k = ui.tool_kind();
        if !kinds.contains(&k) {
            kinds.push(k);
        }
    }
    kinds
}

/// #661 section 4: every tool kind wears its own `tool-*` glyph with an SVG
/// master, and no two kinds share one (the tool library rows and the
/// picker tell kinds apart by it).
#[test]
fn every_tool_kind_has_its_own_svg_mastered_icon() {
    use std::collections::HashSet;
    let svg = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/ui/svg");
    let kinds = every_tool_kind();
    assert!(kinds.len() >= 13, "the editor numbers {} tool kinds", kinds.len());
    let mut seen = HashSet::new();
    for k in kinds {
        let icon = crate::egui_app::tool_icons::tool_icon(k);
        assert!(seen.insert(icon), "{k:?} shares {icon:?} with another kind");
        let file = icon.file();
        assert!(file.starts_with("tool-"), "{k:?} wears the stand-in {file}.png");
        assert!(svg.join(format!("{file}.svg")).is_file(), "{file}.png has no SVG master");
        // Never an operation's glyph: Drill the tool is not Drill the op.
        assert!(
            camcraft_app::operation::KindInfo::ALL.iter().all(|o| crate::egui_app::op_icons::op_icon(o.ty) != icon),
            "{k:?} wears an operation's glyph"
        );
    }
}

/// #661 sections 6-8: the tree rows, badges and CamCraft actions are our
/// own SVG-mastered glyphs, no longer the stand-ins they borrowed.
#[test]
fn tree_rows_badges_and_actions_are_svg_mastered() {
    let svg = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/ui/svg");
    let ours = [
        (Icon::Setup, "tree-"), (Icon::Model, "tree-"), (Icon::Stock, "tree-"), (Icon::WorkOffset, "tree-"),
        (Icon::Fixture, "tree-"), (Icon::Machine, "tree-"), (Icon::OpGroup, "tree-"),
        (Icon::BadgeNever, "badge-"), (Icon::BadgeOk, "badge-"), (Icon::BadgeStale, "badge-"), (Icon::BadgeWarning, "badge-"),
        (Icon::Generate, "act-"), (Icon::Post, "act-"), (Icon::Simulate, "act-"), (Icon::Toolpaths, "act-"),
        (Icon::EyeOff, "act-"), (Icon::FrameAll, "act-"), (Icon::DebugInfo, "act-"),
    ];
    for (icon, prefix) in ours {
        let file = icon.file();
        assert!(file.starts_with(prefix), "{icon:?} loads {file}.png, not a {prefix}* glyph");
        assert!(svg.join(format!("{file}.svg")).is_file(), "{file}.png has no SVG master");
    }
}

/// #661 section 7: the badge says what the row's text says - refused or
/// warned wears the warning, otherwise the cache status; no new state.
#[test]
fn status_badge_follows_the_row_status() {
    use camcraft_app::operation::OpStatus;
    use crate::egui_app::tree_badges::status_badge;
    assert_eq!(status_badge(OpStatus::Never, false, 0), Icon::BadgeNever);
    assert_eq!(status_badge(OpStatus::Ok, false, 0), Icon::BadgeOk);
    assert_eq!(status_badge(OpStatus::Stale, false, 0), Icon::BadgeStale);
    assert_eq!(status_badge(OpStatus::Ok, false, 2), Icon::BadgeWarning);
    assert_eq!(status_badge(OpStatus::Stale, false, 2), Icon::BadgeStale, "a stale path's warnings are old news");
    for s in [OpStatus::Never, OpStatus::Ok, OpStatus::Stale] {
        assert_eq!(status_badge(s, true, 0), Icon::BadgeWarning, "a BLOCKED row is refused");
    }
}
