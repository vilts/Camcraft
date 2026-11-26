//! #3452: the part-off kerf's side and its gouge law - the parting plane is the finished part's end face, the whole kerf of the blade stands on the CHUCK side (the bar that stays held), and a parting move whose band enters the finished part is refused by name; #3469: the user's kept piece (cut-off or chuck) and back stock set the scrap side and the kerf's offset. Machine-tool handbook cutoff practice (the part keeps its length, the kerf comes out of the bar) and ISO 841's lathe Z (positive away from the workholding); our own reading, no vendor material consulted.

use crate::cl::{MoveKind, Toolpath};
use crate::geometry::Triangle;

use super::profile;
use super::revolve;

/// How far a kerf may stand past the end face and still be "beside"
/// it (mm): one micrometre. A float hair, never a real cut.
pub const KERF_TOL: f64 = 1e-6;

/// Which side of the parting plane the chuck (and the scrap bar it
/// holds) is on, along the part's axis: `Below` is toward the lower
/// stations (world -Z), `Above` toward the higher ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WasteSide {
    Below,
    Above,
}

/// The note a part-off path carries where the side rule decides what
/// is kept (#3452): the model goes on past the plane on both sides, so
/// the model alone cannot say which piece is the part.
pub const CHUCK_NOTE: &str = "part-off: the model goes on past the parting plane on both sides. The bar is \
     taken as held on the chuck side (lower Z), so the piece above the plane is cut off as the part and the \
     whole kerf stands below the plane";

/// Does the side rule decide what is kept? Only where the model has
/// length on both sides of the plane at `s` (more than `tol` each).
pub fn rule_decides(d: (f64, f64), s: f64, tol: f64) -> bool {
    s - d.0 > tol && d.1 - s > tol
}

/// The chuck side of a part-off. The scrap side is the side where the
/// bar is HELD, never a guess from the model or the stock overhang (a
/// facing allowance would turn such a guess). No setup or machine
/// states which spindle holds the bar yet, so this is ISO 841's lathe
/// Z: +Z points away from the workholding, the chuck is at -Z. The
/// user's [`PartKerf`] (#3469) says which side of it is scrap.
pub fn chuck_side() -> WasteSide {
    WasteSide::Below
}

impl WasteSide {
    /// The kerf band `(lo, hi)` a blade of width `w` takes beside the
    /// plane at station `s`: its whole width on this side, one flank
    /// on the plane.
    pub fn kerf(self, s: f64, w: f64) -> (f64, f64) {
        match self {
            WasteSide::Below => (s - w, s),
            WasteSide::Above => (s, s + w),
        }
    }

    /// The blade's CL station (the middle of its tip edge) for that
    /// kerf.
    pub fn centre(self, s: f64, w: f64) -> f64 {
        match self {
            WasteSide::Below => s - w * 0.5,
            WasteSide::Above => s + w * 0.5,
        }
    }

    /// The part of a band `(lo, hi)` that stands on the finished
    /// part's side of the plane at `s` (the side away from the chuck).
    fn into_part(self, s: f64, lo: f64, hi: f64) -> (f64, f64) {
        match self {
            WasteSide::Below => (lo.max(s), hi),
            WasteSide::Above => (lo, hi.min(s)),
        }
    }

    /// How much of the model's station span `d` stands on the part's
    /// side of the plane at `s`.
    pub fn part_length(self, d: (f64, f64), s: f64) -> f64 {
        match self {
            WasteSide::Below => d.1 - s,
            WasteSide::Above => s - d.0,
        }
    }
}

/// Which piece of a part-off is the finished part (#3469). The default
/// is #3452's rule: the piece the blade cuts off the free end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PartKept {
    /// The piece cut off the free end is the part; the bar in the
    /// chuck is the scrap side.
    #[default]
    CutOff,
    /// The piece in the chuck is the part; a stub is cut off the free
    /// end (a part held end for end in a second setup).
    Chuck,
}

/// The user's part-off kerf setting (#3469): the piece kept, and the
/// back stock - how far the whole kerf stands off the parting plane
/// into the scrap side, so the part keeps that much to face later.
/// The default is #3452's kerf: one flank on the plane.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct PartKerf {
    pub kept: PartKept,
    pub back_stock: f64,
}

impl PartKerf {
    /// The scrap side of the plane: the chuck side when the cut-off
    /// piece is the part, the free end when the chuck piece is.
    pub fn waste(self) -> WasteSide {
        match (self.kept, chuck_side()) {
            (PartKept::CutOff, s) => s,
            (PartKept::Chuck, WasteSide::Below) => WasteSide::Above,
            (PartKept::Chuck, WasteSide::Above) => WasteSide::Below,
        }
    }

    /// The station of the kerf's flank next to the part: the plane at
    /// `s`, moved the back stock into the scrap side.
    pub fn flank(self, s: f64) -> f64 {
        match self.waste() {
            WasteSide::Below => s - self.back_stock,
            WasteSide::Above => s + self.back_stock,
        }
    }

    /// `Some(reason)` when the back stock is not a size (negative or
    /// not finite).
    pub fn problem(self) -> Option<String> {
        (!self.back_stock.is_finite() || self.back_stock < 0.0).then(|| {
            format!("the part-off back stock {} mm is not a size - it must be 0 or more", self.back_stock)
        })
    }
}

/// The words for the scrap side in a refusal.
fn side_words(w: WasteSide) -> &'static str {
    match w {
        WasteSide::Below => "the chuck side (lower Z)",
        WasteSide::Above => "the free-end side (higher Z)",
    }
}

/// Does the model go on past the plane at `parting_z` on both sides
/// (#3452)? Only then does the chuck-side rule decide which piece is the
/// part; at an end of the model the model itself is the part. A model
/// this cannot read answers `false`: the kernel refuses it by name.
pub fn plane_inside_model(facets: &[Triangle], parting_z: f64, tol: f64) -> bool {
    let Ok(f) = profile::Frame::about(facets) else { return false };
    let Ok(pts) = profile::silhouette(facets, &f, tol * 0.25) else { return false };
    match (pts.first(), pts.last()) {
        (Some(a), Some(b)) => rule_decides((a.0, b.0), f.station_of(parting_z), tol),
        _ => false,
    }
}

/// The refusal for a plane with no part beyond it: the model stands
/// wholly on the scrap side of the piece `kept` (#3469), so the cut
/// would part nothing the model keeps - or would cut into the model.
pub fn no_part_past_plane_for(f: &profile::Frame, d: (f64, f64), plane_z: f64, kept: PartKept) -> String {
    let (side, part, other) = match kept {
        PartKept::CutOff => (
            "the chuck side (lower Z)",
            "the part is what stands above the plane, away from the chuck",
            "if the part is the piece that stays in the chuck, set 'The part is' to 'Chuck piece'",
        ),
        PartKept::Chuck => (
            "the free-end side (higher Z)",
            "the part-off keeps the chuck piece, which is what stands below the plane",
            "if the part is the piece that is cut off, set 'The part is' to 'Cut-off piece'",
        ),
    };
    format!(
        "nothing to part - the model (Z{:.3} to Z{:.3}) stands wholly on {side} of the parting plane \
         Z{plane_z:.3}; {part} - {other}",
        f.height_of(d.0),
        f.height_of(d.1),
    )
}

/// The stations the bar spans: the model's own, widened by the setup
/// stock's ends where they read.
pub fn bar_span(d: (f64, f64), stock: Option<super::stock::StockEnv>) -> (f64, f64) {
    let (mut lo, mut hi) = d;
    if let Some(e) = stock {
        if e.s_bot.is_finite() {
            lo = lo.min(e.s_bot);
        }
        if e.s_top.is_finite() {
            hi = hi.max(e.s_top);
        }
    }
    (lo, hi)
}

/// The generation verdict for a part-off path (#3452): `Some(reason)`
/// when a cutting move's blade band takes material of the finished
/// part - the model on the side of the parting plane at `parting_z`
/// away from the chuck ([`chuck_side`]) - when no part stands past the
/// plane, or when the path cannot be judged at all (a judge that cannot
/// measure never reports silence). The band is the blade's own width,
/// never wider. The side is read from the chuck alone, never from the
/// path, so a kerf put on the wrong side by any cause is caught here.
pub fn part_kerf_refusal(facets: &[Triangle], path: &Toolpath, width: f64, parting_z: f64, tol: f64) -> Option<String> {
    part_kerf_refusal_for(facets, path, width, parting_z, tol, PartKerf::default())
}

/// [`part_kerf_refusal`] for the user's kerf setting (#3469): the part
/// is the piece `kerf.kept` names, and it keeps `kerf.back_stock` past
/// the plane - a blade band in that stock takes part material too.
pub fn part_kerf_refusal_for(
    facets: &[Triangle],
    path: &Toolpath,
    width: f64,
    parting_z: f64,
    tol: f64,
    kerf: PartKerf,
) -> Option<String> {
    if let Some(why) = kerf.problem() {
        return Some(why);
    }
    if facets.is_empty() || !width.is_finite() || width <= 0.0 || !parting_z.is_finite() || !tol.is_finite() || tol <= 0.0 {
        return Some("the part-off kerf cannot be judged: no model, or a blade width, plane or tolerance that is not a size".into());
    }
    let f = match profile::Frame::about(facets) {
        Ok(f) => f,
        Err(e) => return Some(format!("the part-off kerf cannot be judged: {e}")),
    };
    let pts = match profile::silhouette(facets, &f, tol * 0.25) {
        Ok(p) if !p.is_empty() => p,
        Ok(_) => return Some("the part-off kerf cannot be judged: the model has no silhouette".into()),
        Err(e) => return Some(format!("the part-off kerf cannot be judged: {e}")),
    };
    let sg = revolve::segs(&pts);
    let d = (pts[0].0, pts[pts.len() - 1].0);
    let s_p = f.station_of(parting_z);
    let side = kerf.waste();
    if side.part_length(d, s_p) <= tol.max(KERF_TOL) {
        return Some(no_part_past_plane_for(&f, d, parting_z, kerf.kept));
    }
    let s_k = kerf.flank(s_p);
    // The back stock is part material that the model does not draw:
    // the part's end, carried past the plane to the kerf's flank. Its
    // radius is the model's largest within that length of the face.
    let (zone, end_r) = match side {
        WasteSide::Below => ((s_k, s_p), revolve::span_max(&sg, s_p, s_p + kerf.back_stock)),
        WasteSide::Above => ((s_p, s_k), revolve::span_max(&sg, s_p - kerf.back_stock, s_p)),
    };
    let half = width * 0.5;
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        let (sa, ra) = f.sr(m.from);
        let (sb, rb) = f.sr(m.to);
        let (lo, hi) = (sa.min(sb) - half, sa.max(sb) + half);
        let (olo, ohi) = side.into_part(s_k, lo, hi);
        if ohi - olo <= KERF_TOL {
            continue;
        }
        let mut top = revolve::span_max(&sg, olo, ohi);
        if kerf.back_stock > 0.0 && ohi.min(zone.1) - olo.max(zone.0) > KERF_TOL {
            top = top.max(end_r);
        }
        if top > ra.min(rb) + KERF_TOL {
            let (plo, phi) = side.into_part(s_p, lo, hi);
            return Some(gouge_text(&f, i, width, (lo, hi), (phi - plo, ohi - olo), parting_z, kerf));
        }
    }
    None
}

/// The refusal of a part-off move whose blade band `(lo, hi)` takes
/// part material: `depth.0` past the end face, `depth.1` past the
/// kerf's flank (the face plus the back stock).
fn gouge_text(f: &profile::Frame, i: usize, width: f64, band: (f64, f64), depth: (f64, f64), plane_z: f64, kerf: PartKerf) -> String {
    let into = if depth.0 > KERF_TOL {
        let stock = if kerf.back_stock > 0.0 {
            format!(" and through the {:.3} mm back stock the part keeps there", kerf.back_stock)
        } else {
            String::new()
        };
        format!("{:.3} mm past the part's end face at Z{plane_z:.3}{stock}", depth.0)
    } else {
        format!(
            "{:.3} mm into the {:.3} mm back stock the part keeps at its end face Z{plane_z:.3}",
            depth.1, kerf.back_stock
        )
    };
    format!(
        "the part-off cuts into the finished part: move {i} puts the {width:.3} mm blade at Z{:.3} to \
         Z{:.3}, {into} - the whole kerf must stand on {} of that face",
        f.height_of(band.0),
        f.height_of(band.1),
        side_words(kerf.waste()),
    )
}
