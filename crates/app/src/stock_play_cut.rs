//! The cut dispatch of the playback (moved from `stock_play.rs`, the
//! 400-line law) and its layer mode (#2629, GPU stock sim S3b): the ops
//! that the f32 twin can cut leave the f64 field and are cut into a dense
//! display layer by a caller-given cutter (a GPU kernel, or the CPU twin
//! law); the picture is `max(pack_cut(field), layer)`.
//! Provenance: our own design (decision 2629) over camcraft-sim's
//! `layer_play`; nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **Each op has one owner.** The layer mode decides once, before the
//!   first cut, which ops the layer cuts (a plain op whose tool and moves
//!   have a twin); the field cuts every other op and never a layer op.
//!   Every move is cut by exactly one of them, so the max-law picture is
//!   the whole program's.
//! - **Opt-in.** The mode is off unless the caller sets a cutter. The
//!   field (`FieldView::stock`) does not hold the layer ops' cut, so only
//!   a caller whose eye reads the packed frame alone may set one.
//! - **A layer fault falls back to the field, never crashes.** When the
//!   layer cannot cut (a window past the twin's ceiling, a cutter error),
//!   the mode goes off, the reason is logged, and the field is rebuilt
//!   from the uncut stock with every op: the CPU playback of before.

use std::sync::Arc;

use super::{FieldPlay, MAX_CHECKPOINTS};
use crate::stock_sim::preview::LatticeMeta;
use crate::stock_sim::{layer_tool, LayerCutter, LayerPlay, LayerSpan, SdfTool, SimOp};
use camcraft_sim::play_keys::Keyframes;

/// The layer mode's state: the layer and each op's twin tool (`None`:
/// the field cuts that op).
pub(super) struct LayerMode {
    play: LayerPlay,
    tools: Vec<Option<SdfTool>>,
}

impl LayerMode {
    fn holds(&self, op: usize) -> bool {
        self.tools.get(op).is_some_and(Option::is_some)
    }
}

impl FieldPlay {
    /// Turn the layer mode on (#2629): the ops that the twin can cut go
    /// to `cutter` on `meta`'s lattice (the packed frame's), the rest stay
    /// on the field. Answers the ops kept on the field, each with its
    /// reason. Only before the first cut: the field's keyframes must not
    /// hold a layer op's cut.
    pub fn set_layer(&mut self, meta: LatticeMeta, cutter: Arc<LayerCutter>) -> Result<Vec<String>, String> {
        if self.at != 0 || self.keys.len() > 1 || self.partial.is_some() || self.layer.is_some() {
            return Err("layer playback: the layer is set before the first cut, one time".into());
        }
        let mut tools = Vec::with_capacity(self.ops.len());
        let mut kept = Vec::new();
        for op in &self.ops {
            let plain = op.rotary.is_none() && op.lathe.is_none() && op.undercut.is_none() && !op.drill;
            let sim = SimOp { moves: &op.moves, radius: op.radius, ball: op.ball, cone: op.cone, corner: op.corner };
            let tool = if plain {
                layer_tool(&sim, &self.cur).map_err(|e| e.to_string())
            } else {
                Err("a rotary, turning, undercut or drill op has no twin".to_string())
            };
            match tool {
                Ok(t) => tools.push(Some(t)),
                Err(why) => {
                    kept.push(format!("{}: {why}", op.name));
                    tools.push(None);
                }
            }
        }
        // #3336: with no op for the layer, the mode stays off. A layer frame
        // is always a whole layer; the field's patch frames are far smaller.
        if tools.iter().all(Option::is_none) {
            return Ok(kept);
        }
        let play = LayerPlay::new(meta, cutter, self.keys.stride()).map_err(|e| e.to_string())?;
        self.layer = Some(LayerMode { play, tools });
        Ok(kept)
    }

    /// Whether the layer mode is on.
    #[must_use]
    pub const fn layer_on(&self) -> bool {
        self.layer.is_some()
    }

    /// Whether the layer cuts the op of global move `k`.
    pub(super) fn layer_holds_move(&self, k: usize) -> bool {
        let Some(mode) = &self.layer else {
            return false;
        };
        self.ops.iter().position(|o| k >= o.start && k - o.start < o.moves.len()).is_some_and(|i| mode.holds(i))
    }

    /// The layer at the shown position (`at` moves and the travelled
    /// part of the move in flight), or `None` when the mode is off. A
    /// layer fault turns the mode off and rebuilds the field with every
    /// op (the module law); the answer is then `None`.
    pub fn layer_frame(&mut self) -> Result<Option<Arc<Vec<f32>>>, String> {
        let Some(LayerMode { play, tools }) = self.layer.as_mut() else {
            return Ok(None);
        };
        let spans: Vec<LayerSpan<'_>> = tools
            .iter()
            .zip(&self.ops)
            .filter_map(|(t, op)| t.as_ref().map(|tool| LayerSpan { start: op.start, moves: &op.moves, tool }))
            .collect();
        let frame = play.frame(self.at, self.frac, &spans);
        drop(spans);
        match frame {
            Ok(layer) => Ok(Some(layer)),
            Err(e) => {
                self.drop_layer(&e.to_string())?;
                Ok(None)
            }
        }
    }

    /// Turn the layer mode off and rebuild the field with every op, at
    /// the same position. The keyframes start again from the uncut stock.
    pub fn drop_layer(&mut self, why: &str) -> Result<(), String> {
        if self.layer.take().is_none() {
            return Ok(());
        }
        tracing::warn!(target: "camcraft::playback", "layer playback is off, the field cuts every op: {why}");
        let uncut = self.keys.floor(0).ok_or("playback lost its uncut stock")?.1.clone();
        let (at, frac) = (self.at, self.frac);
        self.keys = Keyframes::new(uncut.clone(), self.keys.stride(), MAX_CHECKPOINTS);
        self.drop_partial();
        self.cur = uncut;
        self.cur.set_dirty_tracking(!self.batched);
        self.at = 0;
        self.restored = true;
        self.seek_to(at, frac)?;
        Ok(())
    }

    /// Subtract global moves `[from, to)` from the shown stock (the
    /// layer ops excepted, when the layer mode is on).
    pub(super) fn cut(&mut self, from: usize, to: usize) -> Result<(), String> {
        for (i, op) in self.ops.iter().enumerate() {
            let end = op.start.saturating_add(op.moves.len());
            let (a, b) = (from.max(op.start), to.min(end));
            if a >= b || self.layer.as_ref().is_some_and(|m| m.holds(i)) {
                continue;
            }
            let moves = op.moves.get(a - op.start..b - op.start).ok_or("playback: a move range fell outside its op")?;
            let sim = SimOp { moves, radius: op.radius, ball: op.ball, cone: op.cone, corner: op.corner };
            // A rotary slice is chorded about the fitted axis, turning
            // from the pose the program held before it (M13.1).
            let cut = match (&op.rotary, &op.lathe, &op.undercut) {
                (_, _, Some((cutter, neck_len))) => self.cur.simulate_undercut(moves, cutter, *neck_len),
                (_, Some(l), None) => self.cur.simulate_lathe_profile(moves, &l.profile, &l.axis),
                (Some(ax), None, None) => {
                    let tol = crate::stock_sim::rotary_pose_tol(self.cur.cell());
                    crate::stock_sim::simulate_rotary_slice(&mut self.cur, &sim, &op.moves, a - op.start, ax, tol).map(|_| ())
                }
                (None, None, None) => {
                    if op.drill {
                        self.cur.simulate_drill_display(std::slice::from_ref(&sim))
                    } else {
                        self.cur.simulate_display(std::slice::from_ref(&sim))
                    }
                }
            };
            cut.map_err(|e| format!("playback could not cut {}: {e}", op.name))?;
            self.moves_cut = self.moves_cut.saturating_add(b - a);
        }
        Ok(())
    }
}
