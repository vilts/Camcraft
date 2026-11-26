//! The packed frame of the playback worker (moved from
//! `stock_play_worker.rs`, the 400-line law): the patch chain, the whole
//! or patched cut layer of each picture, and the layer mode's merge
//! (#2629, GPU stock sim S3b): `max(pack_cut(field), layer)`, always a
//! whole layer, so no eye ever repacks a layer frame from the field.
//! Provenance: our own publication law (the module law of the worker).

use std::collections::HashSet;
use std::sync::Arc;

use super::{lock, PlayFrame, Shared};
use crate::stock_play::FieldPlay;
use crate::stock_sim::merge_max;
use crate::stock_sim::preview::{pack_cut, pack_cut_patch, LatticeMeta};
use crate::stock_sim::LayerCutter;
use crate::stock_view::{CutLayer, Packed, PackedBase};

/// Cells in a brick (the field's 8^3): the patch-size yardstick.
const BRICK_TEXELS: usize = 512;

/// What the worker last published: the patch base of the next frame.
#[derive(Default)]
pub(super) struct Chain {
    seq: u64,
    last: Option<Last>,
}

enum Last {
    Full,
    Patch { from: u64, set: HashSet<[i32; 3]> },
}

fn texels(meta: &LatticeMeta) -> usize {
    meta.dims.iter().fold(1usize, |n, &d| n.saturating_mul(d as usize))
}

/// This frame's cut layer relation: `Some((from, bricks))` for a patch
/// over frame `from`, `None` for the whole layer (the module law).
fn delta(
    dirty: Option<HashSet<[i32; 3]>>,
    last: Option<Last>,
    untaken: bool,
    seq: u64,
) -> Option<(u64, HashSet<[i32; 3]>)> {
    match (dirty?, last?) {
        // The frame before still sits untaken and this one replaces
        // it: the patch must also carry that frame's bricks.
        (d, Last::Patch { from, mut set }) if untaken => {
            set.extend(d);
            Some((from, set))
        }
        (_, Last::Full) if untaken => None,
        (d, _) => Some((seq.saturating_sub(1), d)),
    }
}

/// Turn the layer mode on for the worker's playback, on the packed
/// frame's lattice; the ops kept on the field and a refusal are logged,
/// and the playback goes on either way.
pub(super) fn enable_layer(play: &mut FieldPlay, base: &PackedBase, cutter: Arc<LayerCutter>) {
    match play.set_layer(base.meta.clone(), cutter) {
        Ok(kept) => {
            for why in kept {
                tracing::info!(target: "camcraft::playback", "layer playback: the field cuts {why}");
            }
        }
        Err(e) => tracing::warn!(target: "camcraft::playback", "layer playback is off: {e}"),
    }
}

pub(super) fn snapshot(
    shared: &Shared,
    play: &mut FieldPlay,
    base: Option<&Arc<PackedBase>>,
    chain: &mut Chain,
) -> Result<PlayFrame, String> {
    // First: a layer fault rebuilds the field, which the view then shows.
    let layer = play.layer_frame()?;
    let dirty = play.take_dirty();
    let mut view = play.view();
    if let (Some(base), Some(f)) = (base, view.stock.field()) {
        chain.seq = chain.seq.saturating_add(1);
        let seq = chain.seq;
        let untaken = lock(&shared.out).is_some_and(|g| matches!(g.latest, Some(Ok(_))));
        let d = delta(dirty, chain.last.take(), untaken, seq)
            .filter(|(_, set)| set.len().saturating_mul(BRICK_TEXELS) < texels(&base.meta));
        let (cut, last) = match (layer, d) {
            (Some(layer), _) => {
                let mut whole = pack_cut(f, &base.meta);
                merge_max(&mut whole, &layer).map_err(|e| e.to_string())?;
                (CutLayer::Full(Arc::new(whole)), Last::Full)
            }
            (None, Some((from, set))) => {
                let patch = Arc::new(pack_cut_patch(f, &base.meta, &set));
                (CutLayer::Patch { from, patch }, Last::Patch { from, set })
            }
            (None, None) => (CutLayer::Full(Arc::new(pack_cut(f, &base.meta))), Last::Full),
        };
        chain.last = Some(last);
        view.packed = Some(Packed { base: Arc::clone(base), seq, cut });
    }
    Ok(PlayFrame { at: play.at(), frac: play.frac(), view: Arc::new(view) })
}
