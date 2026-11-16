//! The Edge-mode pick outside the boundary picker (user hand test
//! 2026-11-06): Cmd/Ctrl+click (the platform command modifier, #1238)
//! toggles one edge in/out of the operation's selection, Shift+click takes the whole face loop the edge lies on -
//! one click closes a pocket rim. The loop finder is the boundary
//! picker's (`operation::extend_to_loop`); every change is a command.
//!
//! Provenance: our own pick law, specced from public CAM conventions
//! (chain/loop selection); no vendor material involved.

use camcraft_app::geometry::feature;
use camcraft_app::operation::{self, FeatureSel, GeoSel, SrcRef};
use camcraft_app::model;

use crate::interact::{Mods, queue_feature_pick};
use crate::state::{CommandKind, State};

/// Route an edge pick by its modifiers: Shift = the whole loop, command
/// (Cmd on macOS, Ctrl elsewhere) = toggle this edge, plain = replace the
/// selection with it.
pub fn queue_edge_pick(state: &mut State, pick: GeoSel, mods: Mods) {
    if state.op_ui.name.is_empty() || state.boundary_pick.is_some() {
        queue_feature_pick(state, pick, mods.command || mods.shift);
        return;
    }
    if mods.shift && !mods.command {
        take_loop(state, pick);
    } else if mods.command {
        toggle(state, pick);
    } else {
        queue_feature_pick(state, pick, false);
    }
}

/// The open operation's applied pick list (empty when it is not a
/// feature selection).
fn applied(state: &State) -> Vec<GeoSel> {
    model::find_by_name(&state.world, &state.op_ui.name)
        .and_then(|e| model::operation_of(&state.world, &e))
        .map_or(Vec::new(), |op| match op.src {
            SrcRef::Selection(items) => items,
            _ => Vec::new(),
        })
}

fn toggle(state: &mut State, pick: GeoSel) {
    let name = state.op_ui.name.clone();
    let mut items = applied(state);
    // An exact repeat, else a stored pick (a loop or chain taken with
    // Shift) that already covers this edge: Ctrl on it takes it back out,
    // never stacks a second, overlapping pick (#1505).
    let mine = edges_of(state, &pick);
    let covers = |g: &GeoSel| {
        g.entity == pick.entity && !mine.is_empty() && {
            let e = edges_of(state, g);
            mine.iter().all(|x| e.contains(x))
        }
    };
    let Some(i) = items.iter().position(|g| *g == pick).or_else(|| items.iter().position(covers)) else {
        queue_feature_pick(state, pick, true);
        return;
    };
    items.remove(i);
    state.status = format!("{} of '{}' removed from {name}", pick.feature, pick.entity);
    if items.is_empty() {
        state.pending.push_back(CommandKind::EditOperationLinks { name, tool: None, src: Some(SrcRef::Stock) });
    } else {
        state.pending.push_back(CommandKind::SetOperationGeometry { name, items });
    }
}

/// Every edge a chain pick covers (empty for a face/vertex pick or a
/// chain no longer in the model).
fn edges_of(state: &State, g: &GeoSel) -> Vec<u32> {
    let FeatureSel::Chain(id) = g.feature else { return Vec::new() };
    model::find_by_name(&state.world, &g.entity)
        .and_then(|e| model::features_of(&state.world, &e))
        .and_then(|f| feature::resolve_chain(&f, id))
        .map_or(Vec::new(), |c| c.edges)
}

fn take_loop(state: &mut State, pick: GeoSel) {
    let ring = match operation::extend_to_loop(&state.world, std::slice::from_ref(&pick)) {
        Ok(ring) => ring,
        Err(why) => {
            // No face loop through it: say so and keep the edge as a
            // plain addition, so the click is never lost in silence.
            queue_feature_pick(state, pick, true);
            state.status = format!("{} - {why}", state.status);
            return;
        }
    };
    let name = state.op_ui.name.clone();
    let ring_edges: Vec<u32> = ring.iter().flat_map(|g| edges_of(state, g)).collect();
    // Picks the loop already covers drop out, so no edge rides twice.
    let mut items: Vec<GeoSel> = applied(state)
        .into_iter()
        .filter(|g| {
            let e = edges_of(state, g);
            g.entity != pick.entity || e.is_empty() || !e.iter().all(|x| ring_edges.contains(x))
        })
        .collect();
    let n = ring.len();
    for r in ring {
        if !items.contains(&r) {
            items.push(r);
        }
    }
    state.status = format!("{} of '{}' closed around its loop ({n} runs) in {name}", pick.feature, pick.entity);
    state.pending.push_back(CommandKind::SetOperationGeometry { name, items });
}
