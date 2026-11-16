//! The wire, sinker and plasma arms of `OpParams::stamp_with` (#1849
//! split): the wire EDM, sinker EDM and plasma kinds. Provenance: moved
//! verbatim from `params.rs`; our own digest law.

use super::*;
use super::digest::{self, w};

impl OpParams {
    /// The digest bytes of the wire EDM, sinker EDM and plasma kinds;
    /// `stamp_with` dispatches here exhaustively, so any other kind never
    /// reaches this match.
    pub(super) fn stamp_cutting(
        &self,
        world: &World,
        setup: Option<&str>,
        src: Option<&SrcRef>,
        h: &mut std::hash::DefaultHasher,
    ) {
        let wh = |h: &mut std::hash::DefaultHasher, v: Height| digest::wh(world, setup, src, h, v);
        match self {
            // The wire contour (#532): every stored word. The wire
            // and the gap are the machine condition's - `current_stamp`
            // digests them (`wire_op::stamp_condition`).
            OpParams::WireContour(p) => {
                wh(h, p.top_z);
                wh(h, p.bottom_z);
                h.write(p.condition.as_bytes());
                h.write_u8(0xff);
                // #1370: the side as its own discriminant - die and punch
                // keep the bytes they always wrote (0 / 1), so no closed
                // cut's stamp moves; an open run's left / right (#1109)
                // and its run-out clearance, read only by an open run,
                // stand apart.
                match p.side {
                    super::WireSide::Die => h.write_u8(0),
                    super::WireSide::Punch => h.write_u8(1),
                    super::WireSide::Left => {
                        h.write_u8(2);
                        w(h, p.clearance);
                    }
                    super::WireSide::Right => {
                        h.write_u8(3);
                        w(h, p.clearance);
                    }
                }
                // #540: written only when set, so no contour's stamp moves.
                if let Some(c) = p.burnout {
                    h.write(b"burnout");
                    w(h, c);
                }
                match p.start {
                    Some([x, y]) => {
                        h.write_u8(1);
                        w(h, x);
                        w(h, y);
                    }
                    None => h.write_u8(0),
                }
                // #539: the tabs, only when stated, so no stamp of an
                // untabbed cut moves. `capture` shapes no move and is
                // read live at the post.
                if let Some(t) = &p.tabs {
                    h.write(b"tabs");
                    h.write_u32(t.count.unwrap_or(u32::MAX));
                    w(h, t.width.unwrap_or(f64::NAN));
                    h.write_usize(t.at.len());
                    for [x, y] in &t.at {
                        w(h, *x);
                        w(h, *y);
                    }
                    h.write(t.cutoff_condition.as_bytes());
                    h.write_u8(0xff);
                }
                // #536: only a stated taper is digested, so every
                // vertical cut's stamp stays what it was.
                if p.taper_deg != 0.0 {
                    h.write(b"taper");
                    w(h, p.taper_deg);
                }
                // #537: likewise only a stated 4-axis cut.
                if let Some(f) = &p.four_axis {
                    super::wire_ruled::digest(f, h);
                }
            }
            // The sinker burn (#541): every stored number, the datum
            // and the condition id.
            // #545: the side burn writes the burn's bytes under its own tag.
            OpParams::SinkerBurn(p) | OpParams::SinkerSideBurn(super::SinkerSideBurnOp { burn: p, .. }) => {
                wh(h, p.clearance_z);
                wh(h, p.top_z);
                wh(h, p.floor_z);
                h.write_u8(match p.datum {
                    super::SinkerDatum::WorkTop => 0,
                    super::SinkerDatum::CavityFloor => 1,
                    super::SinkerDatum::ElectrodeTip => 2,
                });
                for v in [p.depth, p.x, p.y, p.feed] {
                    w(h, v);
                }
                h.write(p.condition.as_bytes());
                h.write_u8(0xff);
                // #543: the pass and the finished cavity it is judged
                // against - a byte and the sizes only for a rough or
                // finish, so a single burn's digest never moves.
                if p.pass != super::SinkerPass::Single {
                    h.write_u8(match p.pass {
                        super::SinkerPass::Single => 0,
                        super::SinkerPass::Rough => 1,
                        super::SinkerPass::Finish => 2,
                    });
                    w(h, p.cavity_x);
                    w(h, p.cavity_y);
                }
                // #544: the orbit - bytes only when one is stated, so a
                // plain plunge's digest does not move.
                if let Some(o) = &p.orbit {
                    h.write(o.kind.word().as_bytes());
                    w(h, o.radius);
                    h.write_u32(o.vectors);
                    h.write_u32(o.turns);
                }
                // #546: the layer step and a measured wear, a tagged
                // byte only when stated - an older burn's digest holds.
                for (tag, v) in [(1u8, p.layer), (2, p.wear_lost), (3, p.wear_over)] {
                    if let Some(v) = v {
                        h.write_u8(tag);
                        w(h, v);
                    }
                }
                // #545: then the side's direction, amount and undersize.
                if let OpParams::SinkerSideBurn(s) = self {
                    h.write(s.direction.word().as_bytes());
                    w(h, s.amount);
                    w(h, s.undersize);
                }
            }
            // The plasma profile (#670): every stored number, the
            // optional ones with a presence byte.
            OpParams::PlasmaProfile(p) => {
                wh(h, p.clearance_z);
                wh(h, p.top_z);
                wh(h, p.bottom_z);
                h.write_u8(u8::from(p.comp == super::PlasmaComp::Control));
                h.write_u8(u8::from(p.reversed));
                for v in [p.lead_in, p.lead_out, p.small_hole_dia, p.small_hole_feed, p.overburn] {
                    w(h, v);
                }
                for v in [p.kerf, p.feed, p.pierce_height, p.cut_height, p.pierce_delay] {
                    match v {
                        Some(x) => {
                            h.write_u8(1);
                            w(h, x);
                        }
                        None => h.write_u8(0),
                    }
                }
            }
            _ => {}
        }
    }
}
