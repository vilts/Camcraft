//! The through-openings of a retained wire EDM die plate, and the one opening that a selected
//! rim matches (#2957, #2977). Provenance: planar region boolean algebra on the closed
//! vertical prism of the plate.

use crate::geo2d::{Loop, Region};

/// Check that `region`, the cross-section of the retained die plate, has
/// exactly one shell and `openings` through-openings, and that it fits inside
/// the stock bounds `lo`..`hi`.
///
/// # Errors
/// A plate with another count of shells or openings, or a plate outside the stock.
pub fn check_die_plate(
    region: &Region,
    lo: [f64; 2],
    hi: [f64; 2],
    openings: usize,
) -> Result<(), String> {
    let loops = region.loops();
    if loops.len() != openings + 1 || loops.iter().filter(|r| r.area() > 0.0).count() != 1 {
        return Err(match openings {
            1 => "The native wire EDM job requires one retained die plate with exactly one through-opening".into(),
            n => format!("The die group requires one retained die plate with exactly {n} through-openings, one for each work"),
        });
    }
    if loops
        .iter()
        .flat_map(|r| r.points())
        .any(|p| p.x < lo[0] || p.x > hi[0] || p.y < lo[1] || p.y > hi[1])
    {
        return Err("The retained die plate must fit inside the stock".into());
    }
    Ok(())
}

/// The through-opening of the plate `region` whose footprint equals the
/// footprint of the selected rim `picked`. A rim with open chain ends
/// (`picked_is_closed` false) matches no opening. Reading only the picked
/// loop could post a cut through a blind floor or on an unrelated face, so
/// the caller first checks the plate with [`check_die_plate`].
///
/// # Errors
/// A rim that matches no opening or more than one, and an invalid footprint.
pub fn matching_opening<'r>(
    region: &'r Region,
    picked: &Loop,
    picked_is_closed: bool,
) -> Result<&'r Loop, String> {
    let footprint = |r: &Loop| Region::new(vec![r.clone()]).map_err(|e| e.to_string());
    let selection = footprint(picked)?;
    let mut matched = None;
    for hole in region.loops().iter().filter(|r| r.area() < 0.0) {
        let candidate = footprint(hole)?;
        if picked_is_closed
            && candidate.difference(&selection).is_ok_and(|r| r.is_empty())
            && selection.difference(&candidate).is_ok_and(|r| r.is_empty())
        {
            if matched.is_some() {
                return Err(
                    "The selected rim matches more than one opening of the die plate".into(),
                );
            }
            matched = Some(hole);
        }
    }
    matched.ok_or_else(|| {
        "The selected chains must match the full through-opening of the retained die plate".into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo2d::Point2;

    fn square(cx: f64, cy: f64, half: f64) -> Loop {
        Loop::new(vec![
            Point2::new(cx - half, cy - half),
            Point2::new(cx + half, cy - half),
            Point2::new(cx + half, cy + half),
            Point2::new(cx - half, cy + half),
        ])
        .unwrap()
    }

    fn plate() -> Region {
        Region::new(vec![
            square(0.0, 0.0, 30.0),
            square(-10.0, 0.0, 4.0),
            square(10.0, 0.0, 4.0),
        ])
        .unwrap()
    }

    #[test]
    fn a_plate_needs_the_stated_openings_inside_the_stock() {
        let plate = plate();
        assert!(check_die_plate(&plate, [-40.0, -40.0], [40.0, 40.0], 2).is_ok());
        let one = check_die_plate(&plate, [-40.0, -40.0], [40.0, 40.0], 1).unwrap_err();
        assert!(one.contains("exactly one through-opening"), "{one}");
        let small = check_die_plate(&plate, [-20.0, -40.0], [40.0, 40.0], 2).unwrap_err();
        assert!(small.contains("inside the stock"), "{small}");
    }

    #[test]
    fn a_closed_rim_matches_its_own_opening_only() {
        let plate = plate();
        let right = matching_opening(&plate, &square(10.0, 0.0, 4.0), true).unwrap();
        assert!(right.points().iter().all(|p| p.x > 0.0));
        for (rim, closed) in [
            (square(10.0, 0.0, 4.0), false),
            (square(10.0, 0.0, 3.0), true),
            (square(0.0, 0.0, 30.0), true),
        ] {
            let error = matching_opening(&plate, &rim, closed).unwrap_err();
            assert!(
                error.contains("must match the full through-opening"),
                "{error}"
            );
        }
    }
}
