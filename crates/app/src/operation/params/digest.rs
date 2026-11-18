//! The shared writers of `OpParams::stamp_with` and its link trailer
//! (#1849 split). Provenance: moved verbatim from `params.rs`; our own
//! digest law.

use super::*;

/// One scalar, as its bit pattern.
pub(super) fn w(h: &mut std::hash::DefaultHasher, v: f64) {
    h.write(&v.to_bits().to_le_bytes());
}

/// A height: its reference (tag, plus slot for a work offset) and its
/// offset bits; a non-Fixed reference also writes its resolved Z (or an
/// unresolved marker) - `stamp_with` says why.
pub(super) fn wh(
    world: &World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    h: &mut std::hash::DefaultHasher,
    v: Height,
) {
    let (tag, slot) = match v.reference {
        HeightReference::Fixed => (0u8, 0u8),
        HeightReference::TopOfStock => (1, 0),
        HeightReference::BottomOfStock => (2, 0),
        HeightReference::TopOfModel => (3, 0),
        HeightReference::BottomOfModel => (4, 0),
        // #329: the slot is the setup's; digesting the resolved
        // one keeps every migrated stamp exactly as it was.
        HeightReference::WorkOffset => (5, u8::try_from(crate::setup::wcs_slot(world, setup)).unwrap_or(u8::MAX)),
        HeightReference::TopOfSelection => (6, 0),
        HeightReference::BottomOfSelection => (7, 0),
    };
    h.write_u8(tag);
    h.write_u8(slot);
    w(h, v.offset);
    if tag != 0 {
        match resolve_height(world, setup, src, v) {
            Some(z) => {
                h.write_u8(1);
                w(h, z);
            }
            None => h.write_u8(0),
        }
    }
}

/// An optional height: presence first, then the height.
pub(super) fn wh_opt(
    world: &World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    h: &mut std::hash::DefaultHasher,
    v: Option<Height>,
) {
    match v {
        Some(v) => {
            h.write_u8(1);
            wh(world, setup, src, h, v);
        }
        None => h.write_u8(0),
    }
}

/// A 2D point, x then y.
pub(super) fn wp2(h: &mut std::hash::DefaultHasher, p: Point2) {
    w(h, p.x);
    w(h, p.y);
}

/// The 3+2 index (M5.1 stage B): a tilt changes every move the
/// generator makes, so it changes the digest. Presence first, then the
/// basis version (omitted for legacy version 0), then the two letters
/// and the anchor. A basis change invalidates an incompatible cache.
pub(super) fn wt(h: &mut std::hash::DefaultHasher, t: Option<Tilt>) {
    match t {
        Some(t) => {
            h.write_u8(1);
            // Legacy tilts keep their original digest and frame. A corrected
            // basis must never accept a cache made under the legacy rule.
            if t.basis_version != 0 {
                h.write_u8(0xB1);
                h.write_u8(t.basis_version);
            }
            w(h, t.a_deg);
            w(h, t.b_deg);
            for v in t.origin {
                w(h, v);
            }
        }
        None => h.write_u8(0),
    }
}

impl OpParams {
    /// The link trailer `stamp_with` writes after every kind's arm.
    pub(super) fn stamp_links(&self, h: &mut std::hash::DefaultHasher) {
        // M11.2 C/D: the level transitions and links change the moves,
        // so they change the digest - appended only when they differ
        // from the kind's pre-M11.2 shape, so every op that never
        // touched them keeps the digest it was cached under. (A contour
        // saved with the old stay-down ramp goes Stale once and
        // regenerates the same path - the mechanism saying so.)
        // (The runtime centre-cutting flag is the tool's, never the op's.)
        if let Some(l) = self.links().map(|l| crate::links::Links {
            approach: crate::links::Approach { center_cutting: true, ..l.approach },
            ..*l
        })
            && l != self.default_links()
        {
            h.write_u8(0xC2);
            let link = |k: crate::links::Link| match k {
                crate::links::Link::Clearance => 0u8,
                crate::links::Link::Retract => 1,
                crate::links::Link::KeepDown => 2,
            };
            h.write_u8(match l.transition {
                None => 0,
                Some(Entry3D::Plunge) => 1,
                Some(Entry3D::Ramp) => 2,
                Some(Entry3D::Helix) => 3,
                Some(Entry3D::Arc) => 4,
            });
            h.write_u8(link(l.level));
            h.write_u8(link(l.pass));
            for v in [l.ramp_angle, l.helix_pitch, l.max_stay_down] {
                w(h, v);
            }
            // M11.3 B/C/D: the entry, lead and feed parameters - bytes
            // only when they differ from the defaults, so an op with
            // links but default approach keeps its digest.
            let a = l.approach;
            if !a.is_default() {
                h.write_u8(0xC3);
                for v in [
                    a.ramp_angle, a.ramp_max_len, a.helix_min_radius, a.helix_pitch, a.helix_angle,
                    a.arc_radius, a.arc_sweep, a.plunge_height, a.ramp_feed, a.plunge_feed,
                    a.lead_feed_pct, a.max_engagement, a.ext_in, a.ext_out, a.lead_height,
                    a.overlap, a.axis_tol,
                ] {
                    w(h, v);
                }
                for f in [a.ramp_to_depth, a.helix_cw, a.axis_start] {
                    h.write_u8(u8::from(f));
                }
                // M11.3: the horizontal arc entry - a byte only when on,
                // so every earlier approach keeps its digest.
                if a.arc_xy {
                    h.write_u8(0xC4);
                }
                // #1449: pre-drilled entry - a byte only when on.
                if a.predrilled {
                    h.write_u8(0xC7);
                }
                // M13.11: control compensation moves what the post
                // writes - a byte only when on, so every earlier
                // approach keeps its digest.
                if a.compensation == crate::links::Compensation::Control {
                    h.write_u8(0xC5);
                }
                // #38: the wear variant posts the centre path under
                // G41/G42 - its own byte, again only when on.
                if a.compensation == crate::links::Compensation::Wear {
                    h.write_u8(0xC6);
                }
                // #1448: the user's start point - bytes only when set.
                if let Some(q) = a.start_point {
                    h.write_u8(0xC7);
                    w(h, q.x);
                    w(h, q.y);
                }
                // #1473: the exit point - bytes only when set.
                if let Some(q) = a.exit_point {
                    h.write_u8(0xC8);
                    w(h, q.x);
                    w(h, q.y);
                }
                match a.lead_out {
                    None => h.write_u8(0xFF),
                    Some(Lead::None) => h.write_u8(0),
                    Some(Lead::Linear { length }) => {
                        h.write_u8(1);
                        w(h, length);
                    }
                    Some(Lead::Arc { radius }) => {
                        h.write_u8(2);
                        w(h, radius);
                    }
                    Some(Lead::Tangential { radius }) => {
                        h.write_u8(3);
                        w(h, radius);
                    }
                    Some(Lead::Perpendicular { length }) => {
                        h.write_u8(4);
                        w(h, length);
                    }
                }
            }
        }
    }
}
