//! The pieces of a pocket region that no legal tool position reaches
//! (#1655 bands, #2438 corners). A pocket's cutter-center area is the region eroded by the tool
//! radius plus the stock to leave (`pocket`); a connected piece of the
//! region that erodes to nothing gets no pass. The generator refuses only
//! a region that erodes to nothing as a whole, so a caller that must say
//! which material stays uncut asks here and puts the answer in a note.
//! The corner check also finds residual material inside pieces that keep
//! a cutter-center area: dilating that area by the same reach leaves sharp
//! corners uncut. When a stock outline is supplied, residuals outside the
//! stock (extended air) are excluded. Narrow slivers and circular-arc
//! approximation noise are filtered out; these checks produce notes.
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

/// A leftover corner cusp that erodes to nothing by this (mm) is
/// circular-arc approximation noise, not uncut sharp-corner stock.
const CORNER_HALF_WIDTH: f64 = 0.03;

/// Connected material pieces of `region` (each outer loop less any nested
/// outer loops), dropping narrow tessellation/offset slivers.
fn connected_pieces(region: &Region) -> Result<Vec<Region>, GenError> {
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
        out.push(piece);
    }
    Ok(out)
}

/// The connected pieces of `region` (each outer loop less the outer loops
/// nested in it) that erode to nothing by `reach` - the tool radius plus
/// the stock to leave. Slivers of noise are left out. Empty when every
/// piece keeps a cutter-center area, and for an empty region.
pub fn unreached_pieces(region: &Region, reach: f64) -> Result<Vec<Region>, GenError> {
    if !(reach.is_finite() && reach > 0.0) {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for piece in connected_pieces(region)? {
        if piece.offset(-reach)?.is_empty() {
            out.push(piece);
        }
    }
    Ok(out)
}

/// Connected corner residuals left inside reached pieces of `region`:
/// where a piece's cutter-center area is non-empty, its round dilation
/// `center.offset(reach)` stops short of sharp `< 180 deg` corners. When
/// `stock` is given, cusps outside the stock outline (in extended air)
/// are excluded. Polygon arc discretization slivers thinner than
/// [`CORNER_HALF_WIDTH`] are filtered out.
pub fn unreached_corners(region: &Region, reach: f64, stock: Option<&Region>) -> Result<Vec<Region>, GenError> {
    if !(reach.is_finite() && reach > 0.0) {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for piece in connected_pieces(region)? {
        let center = piece.offset(-reach)?;
        if center.is_empty() {
            continue;
        }
        let swept = center.offset(reach)?;
        let mut leftover = piece.difference(&swept)?;
        if let Some(st) = stock {
            leftover = leftover.intersect(st)?;
        }
        for corner in connected_pieces(&leftover)? {
            if !corner.offset(-CORNER_HALF_WIDTH)?.is_empty() {
                out.push(corner);
            }
        }
    }
    Ok(out)
}

fn len_label(v: f64, unit: Unit) -> String {
    format!("{} {}", units::len(v, unit, 3), unit.suffix())
}

fn leave_note(stock_to_leave: f64, unit: Unit) -> String {
    if stock_to_leave > 0.0 {
        format!(" with {} stock to leave", len_label(stock_to_leave, unit))
    } else {
        String::new()
    }
}

fn format_boxes(pieces: &[Region], unit: Unit) -> Vec<String> {
    pieces
        .iter()
        .filter_map(Region::bounds)
        .map(|(lo, hi)| {
            format!(
                "X {} to {}, Y {} to {}",
                len_label(lo.x, unit),
                len_label(hi.x, unit),
                len_label(lo.y, unit),
                len_label(hi.y, unit),
            )
        })
        .collect()
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
    let boxes = format_boxes(&pieces, unit);
    let leave = leave_note(stock_to_leave, unit);
    Ok(Some(format!(
        "{} of the area to clear {} narrower than the {} tool{leave}, so no pass cuts {}: {} - use a smaller tool, or a wider stock margin",
        if pieces.len() == 1 { "1 band" } else { "some bands" },
        if pieces.len() == 1 { "is" } else { "are" },
        len_label(tool_diameter, unit),
        if pieces.len() == 1 { "it" } else { "them" },
        boxes.join("; "),
    )))
}

/// The note that names each corner cusp [`unreached_corners`] leaves
/// uncut inside reached pieces (such as a sharp box stock's corners when
/// cutting around an island with no boundary extension). `None` when the
/// swept cutter clears every stock corner.
pub fn unreached_corner_note(
    region: &Region,
    stock: Option<&Region>,
    tool_diameter: f64,
    stock_to_leave: f64,
    unit: Unit,
) -> Result<Option<String>, GenError> {
    let corners = unreached_corners(region, tool_diameter * 0.5 + stock_to_leave, stock)?;
    if corners.is_empty() {
        return Ok(None);
    }
    let boxes = format_boxes(&corners, unit);
    let leave = leave_note(stock_to_leave, unit);
    Ok(Some(format!(
        "{} of the area to clear {} tighter than the {} tool{leave}, leaving uncut corner material at {} - extend the Boundary or use a smaller tool",
        if corners.len() == 1 { "1 corner" } else { "some corners" },
        if corners.len() == 1 { "is" } else { "are" },
        len_label(tool_diameter, unit),
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

    fn rounded_rect(hx: f64, hy: f64, rad: f64, segs: usize) -> Loop {
        let mut pts = Vec::with_capacity(4 * (segs + 1));
        for k in 0..4 {
            let (cx, cy, a0) = match k {
                0 => (hx - rad, hy - rad, 0.0),
                1 => (-hx + rad, hy - rad, std::f64::consts::FRAC_PI_2),
                2 => (-hx + rad, -hy + rad, std::f64::consts::PI),
                _ => (hx - rad, -hy + rad, 3.0 * std::f64::consts::FRAC_PI_2),
            };
            for i in 0..=segs {
                let a = a0 + std::f64::consts::FRAC_PI_2 * (i as f64 / segs as f64);
                pts.push(Point2::new(cx + rad * a.cos(), cy + rad * a.sin()));
            }
        }
        Loop::new(pts).expect("rounded rect")
    }

    #[test]
    fn sharp_stock_corners_around_island_are_named_until_extended() {
        let stock = Region::new(vec![square(-50.0, -30.0, 50.0, 30.0)]).expect("stock");
        let island = rounded_rect(30.0, 15.0, 5.0, 8);
        let region = Region::new(vec![square(-50.0, -30.0, 50.0, 30.0), island.clone()]).expect("region");

        let corners = unreached_corners(&region, 5.0, Some(&stock)).expect("corners");
        assert_eq!(corners.len(), 4, "four sharp stock corners, zero island slivers: {corners:?}");
        let note = unreached_corner_note(&region, Some(&stock), 10.0, 0.0, Unit::Millimeters)
            .expect("note")
            .expect("some");
        assert!(note.contains("some corners") && note.contains("10.000 mm tool"), "{note}");

        // Extending outside the stock clears all four stock corners: nothing inside stock remains.
        let extended = Region::new(vec![square(-56.0, -36.0, 56.0, 36.0), island]).expect("extended");
        assert!(unreached_corners(&extended, 5.0, Some(&stock)).expect("ext").is_empty());
        assert!(unreached_corner_note(&extended, Some(&stock), 10.0, 0.0, Unit::Millimeters).expect("note").is_none());
    }

    #[test]
    fn filleted_corners_and_unreached_bands_emit_no_corner_note() {
        let filleted = Region::new(vec![rounded_rect(30.0, 20.0, 5.0, 8)]).expect("filleted");
        assert!(unreached_corners(&filleted, 5.0, None).expect("filleted").is_empty());

        let band = Region::new(vec![square(0.0, 0.0, 50.0, 50.0), square(5.0, 5.0, 45.0, 45.0)]).expect("band");
        assert!(unreached_corners(&band, 6.0, None).expect("band").is_empty());
    }

    #[test]
    fn a_noise_sliver_and_an_empty_region_say_nothing() {
        let r = Region::new(vec![square(0.0, 0.0, 50.0, 50.0), square(0.004, 0.004, 49.996, 49.996)]).expect("region");
        assert!(unreached_pieces(&r, 3.0).expect("pieces").is_empty());
        assert!(unreached_pieces(&Region::empty(), 3.0).expect("pieces").is_empty());
        assert!(unreached_pieces(&r, f64::NAN).expect("pieces").is_empty());
        assert!(unreached_corners(&r, 3.0, None).expect("corners").is_empty());
        assert!(unreached_corners(&Region::empty(), 3.0, None).expect("corners").is_empty());
        assert!(unreached_corners(&r, f64::NAN, None).expect("corners").is_empty());
    }
}
