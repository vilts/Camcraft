//! The pocket family's region on a part, memoized by its full input
//! (#2235): `area::pocket_region_on_part` slices the part at every
//! ladder plane, and the viewport asks for the open operation's region
//! on every frame. Our own design notes (an exact-key memo of a pure
//! function); no vendor material consulted.
//!
//! The key is the whole input, compared for equality - the loops, the
//! part's facets, the planes and the unit - so a hit is the answer a
//! fresh slice gives, with no revision counter to forget. The region
//! and the two refusals the law names itself (`Invalid`, `Empty`) are
//! kept; any other error is computed again on every call.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use super::area::pocket_region_on_part;
use super::*;
use crate::units::Unit;

/// How many inputs the memo remembers (each holds one part's facets).
const CAP: usize = 4;

/// A kept answer: the region, or a refusal by its words.
#[derive(Clone)]
enum Answer {
    Region(Region),
    Invalid(String),
    Empty(String),
}

impl Answer {
    fn of(r: &Result<Region, GenError>) -> Option<Self> {
        match r {
            Ok(region) => Some(Self::Region(region.clone())),
            Err(GenError::Invalid(m)) => Some(Self::Invalid(m.clone())),
            Err(GenError::Empty(m)) => Some(Self::Empty(m.clone())),
            Err(_) => None,
        }
    }

    fn into_result(self) -> Result<Region, GenError> {
        match self {
            Self::Region(r) => Ok(r),
            Self::Invalid(m) => Err(GenError::Invalid(m)),
            Self::Empty(m) => Err(GenError::Empty(m)),
        }
    }
}

struct Entry {
    loops: Vec<Loop>,
    part: Arc<[Triangle]>,
    planes: Vec<f64>,
    unit: Unit,
    answer: Answer,
}

thread_local! {
    static MEMO: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
    /// Real slices on this thread - the counter the #2235 pins read.
    static SLICES: Cell<u64> = const { Cell::new(0) };
}

/// [`pocket_region_on_part`], memoized by its exact input.
pub(super) fn pocket_region_memo(
    loops: Vec<Loop>,
    part: &[Triangle],
    planes: &[f64],
    unit: Unit,
) -> Result<Region, GenError> {
    let hit = MEMO.with(|m| {
        let mut m = m.borrow_mut();
        let i = m.iter().position(|e| {
            e.unit == unit && e.planes == planes && e.loops == loops && *e.part == *part
        })?;
        let entry = m.remove(i);
        let answer = entry.answer.clone();
        m.push(entry); // MRU bump
        Some(answer)
    });
    if let Some(answer) = hit {
        return answer.into_result();
    }
    SLICES.with(|n| n.set(n.get().saturating_add(1)));
    let key = loops.clone();
    let result = pocket_region_on_part(loops, part, planes, unit);
    if let Some(answer) = Answer::of(&result) {
        MEMO.with(|m| {
            let mut m = m.borrow_mut();
            if m.len() >= CAP {
                m.remove(0);
            }
            m.push(Entry { loops: key, part: part.into(), planes: planes.to_vec(), unit, answer });
        });
    }
    result
}

/// How many real part slices [`pocket_region_memo`] made on this thread.
#[must_use]
pub fn pocket_slices() -> u64 {
    SLICES.with(Cell::get)
}

#[cfg(test)]
mod law {
    use super::*;
    use crate::geometry::{Point3, cube};

    fn ring(h: f64) -> Loop {
        Loop::new(vec![Point2::new(-h, -h), Point2::new(h, -h), Point2::new(h, h), Point2::new(-h, h)]).expect("ring")
    }

    #[test]
    fn a_hit_is_the_fresh_answer_and_any_changed_input_slices_again() {
        // A 4x4 boss standing in a 20x20 pick: an island at every plane.
        let part: Vec<Triangle> = cube(Point3::new(-2.0, -2.0, 0.0), Point3::new(2.0, 2.0, 10.0)).facets.to_vec();
        let planes = [10.0, 5.0, 0.0];
        let fresh = pocket_region_on_part(vec![ring(10.0)], &part, &planes, Unit::Millimeters).expect("region");
        let n = pocket_slices();
        let a = pocket_region_memo(vec![ring(10.0)], &part, &planes, Unit::Millimeters).expect("region");
        let b = pocket_region_memo(vec![ring(10.0)], &part, &planes, Unit::Millimeters).expect("region");
        assert_eq!(a, fresh, "the memo says what the fresh slice says");
        assert_eq!(b, fresh);
        assert_eq!(pocket_slices() - n, 1, "the second ask is a hit");

        // A moved part (a landing), another pick, another plane: each slices.
        let moved: Vec<Triangle> = cube(Point3::new(-3.0, -2.0, 0.0), Point3::new(2.0, 2.0, 10.0)).facets.to_vec();
        let n = pocket_slices();
        let c = pocket_region_memo(vec![ring(10.0)], &moved, &planes, Unit::Millimeters).expect("region");
        assert_eq!(c, pocket_region_on_part(vec![ring(10.0)], &moved, &planes, Unit::Millimeters).expect("region"));
        assert_ne!(c, fresh, "the moved boss moves the region");
        let _ = pocket_region_memo(vec![ring(9.0)], &part, &planes, Unit::Millimeters);
        let _ = pocket_region_memo(vec![ring(10.0)], &part, &[10.0, 0.0], Unit::Millimeters);
        assert_eq!(pocket_slices() - n, 3, "no changed input reads a kept answer");

        // A refusal is kept by its words and said again on a hit.
        let n = pocket_slices();
        let full = |r: Result<Region, GenError>| r.err().map(|e| e.to_string());
        let first = full(pocket_region_memo(vec![ring(1.0)], &part, &planes, Unit::Millimeters));
        let again = full(pocket_region_memo(vec![ring(1.0)], &part, &planes, Unit::Millimeters));
        assert!(first.as_deref().is_some_and(|m| m.contains("fills the whole selection")), "{first:?}");
        assert_eq!(first, again);
        assert_eq!(pocket_slices() - n, 1);
    }
}
