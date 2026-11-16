//! The pieces of a pocket region that no legal tool position reaches
//! (#1655). A pocket's cutter-center area is the region eroded by the tool
//! radius plus the stock to leave (`pocket`); a connected piece of the
//! region that erodes to nothing gets no pass. The generator refuses only
//! a region that erodes to nothing as a whole, so a caller that must say
//! which material stays uncut asks here and puts the answer in a note.
//!
//! Provenance: our own, from first principles - the morphological erosion
//! of a planar region by a disc (the offset law of any computational-
//! geometry text) applied to each connected piece apart. No vendor
//! material consulted.

use crate::geo2d::Region;
use crate::units::{self, Unit};

use super::GenError;

/// Below this (mm^2) a piece is offset or tessellation noise, not
/// material a user left to clear.
const PIECE_AREA_MIN: f64 = 1e-4;

/// A piece that erodes to nothing by this (mm) is a sliver of noise along
/// a shared wall (a stock face that meets the part face), not a band.
const SLIVER_HALF_WIDTH: f64 = 5e-3;

/// The connected pieces of `region` (each outer loop less the outer loops
/// nested in it) that erode to nothing by `reach` - the tool radius plus
/// the stock to leave. Slivers of noise are left out. Empty when every
/// piece keeps a cutter-center area, and for an empty region.
pub fn unreached_pieces(region: &Region, reach: f64) -> Result<Vec<Region>, GenError> {
    if !(reach.is_finite() && reach > 0.0) {
        return Ok(Vec::new());
    }
    let outers: Vec<Region> = region
        .loops()
        .iter()
        .filter(|l| l.area() > 0.0)
        .map(|l| Region::new(vec![l.clone()]))
        .collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for (i, fill) in outers.iter().enumerate() {
        let mut piece = region.intersect(fill)?;
        for (j, inner) in outers.iter().enumerate() {
            // A nested outer loop stands inside a hole of this one: its
            // material is a piece of its own.
            if i != j && inner.area() < fill.area() && inner.difference(fill)?.area().abs() < PIECE_AREA_MIN {
                piece = piece.difference(inner)?;
            }
        }
        if piece.area().abs() < PIECE_AREA_MIN || piece.offset(-SLIVER_HALF_WIDTH)?.is_empty() {
            continue;
        }
        if piece.offset(-reach)?.is_empty() {
            out.push(piece);
        }
    }
    Ok(out)
}

/// The note that names each piece [`unreached_pieces`] finds: where it
/// stands (its XY box) and why no pass cuts it. `None` when every piece
/// is reached.
pub fn unreached_note(
    region: &Region,
    tool_diameter: f64,
    stock_to_leave: f64,
    unit: Unit,
) -> Result<Option<String>, GenError> {
    let pieces = unreached_pieces(region, tool_diameter * 0.5 + stock_to_leave)?;
    if pieces.is_empty() {
        return Ok(None);
    }
    let len = |v: f64| format!("{} {}", units::len(v, unit, 3), unit.suffix());
    let boxes: Vec<String> = pieces
        .iter()
        .filter_map(Region::bounds)
        .map(|(lo, hi)| format!("X {} to {}, Y {} to {}", len(lo.x), len(hi.x), len(lo.y), len(hi.y)))
        .collect();
    let leave = if stock_to_leave > 0.0 { format!(" with {} stock to leave", len(stock_to_leave)) } else { String::new() };
    Ok(Some(format!(
        "{} of the area to clear {} narrower than the {} tool{leave}, so no pass cuts {}: {} - use a smaller tool, or a wider stock margin",
        if pieces.len() == 1 { "1 band" } else { "some bands" },
        if pieces.len() == 1 { "is" } else { "are" },
        len(tool_diameter),
        if pieces.len() == 1 { "it" } else { "them" },
        boxes.join("; "),
    )))
}

#[cfg(test)]
mod law {
    use super::*;
    use crate::geo2d::{Loop, Point2};

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
        Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)])
            .expect("square")
    }

    #[test]
    fn a_band_narrower_than_the_tool_beside_a_wide_bore_is_named() {
        // A 50 mm stock, a 40 mm part (5 mm band), a 20 mm bore in the
        // part: a 12 mm tool fits the bore, never the band.
        let r = Region::new(vec![square(0.0, 0.0, 50.0, 50.0), square(5.0, 5.0, 45.0, 45.0), square(15.0, 15.0, 35.0, 35.0)])
            .expect("region");
        assert!(!r.offset(-6.0).expect("offset").is_empty(), "the bore keeps a center area");
        let pieces = unreached_pieces(&r, 6.0).expect("pieces");
        assert_eq!(pieces.len(), 1, "only the band");
        assert!((pieces[0].area() - (2500.0 - 1600.0)).abs() < 1e-3, "{}", pieces[0].area());
        let note = unreached_note(&r, 12.0, 0.0, Unit::Millimeters).expect("note").expect("some");
        assert!(note.contains("1 band") && note.contains("X 0.000 mm to 50.000 mm"), "{note}");
        // A tool that fits the band too: nothing to say.
        assert!(unreached_note(&r, 4.0, 0.0, Unit::Millimeters).expect("note").is_none());
        // The stock to leave counts in the reach.
        let left = unreached_note(&r, 4.0, 1.0, Unit::Millimeters).expect("note").expect("some");
        assert!(left.contains("with 1.000 mm stock to leave"), "{left}");
    }

    #[test]
    fn a_noise_sliver_and_an_empty_region_say_nothing() {
        let r = Region::new(vec![square(0.0, 0.0, 50.0, 50.0), square(0.004, 0.004, 49.996, 49.996)]).expect("region");
        assert!(unreached_pieces(&r, 3.0).expect("pieces").is_empty());
        assert!(unreached_pieces(&Region::empty(), 3.0).expect("pieces").is_empty());
        assert!(unreached_pieces(&r, f64::NAN).expect("pieces").is_empty());
    }
}
