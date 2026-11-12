//! The generators' refusal type, and the place a refusal names (#261
//! part 2): a refusal that is about one spot of the part - a hole, a
//! strip between two holes, a plunge into solid - carries that spot, so
//! the viewport can show where the operation was refused instead of
//! leaving the user to read coordinates off the sentence.
//! Provenance: our own error model (split out of `face.rs`, where the
//! enum was born); no vendor material consulted.
//!
//! A located refusal reads exactly like an [`GenError::Invalid`] one -
//! same prefix, same sentence - so no message, log line or test that
//! reads the text changes when a site starts naming its place. The
//! point is in the frame the generator ran in; the caller that knows
//! the frame (the app's `run_job`) maps it to world before the UI sees
//! it ([`GenError::map_location`]).
//!
//! #261 part 3: a refusal raised with a path in hand - the ladders'
//! centre-cutting refusal, the plunge law's audit of a finished path -
//! may also carry that path as a [`Ghost`], display-only, read the same
//! way ([`GenError::ghost`]). It reads exactly like the refusal without
//! it, and it never becomes a toolpath.

use thiserror::Error;

use crate::geo2d::GeoError;
use crate::geometry::Point3;

use super::ghost::Ghost;

#[derive(Error, Debug, PartialEq)]
pub enum GenError {
    #[error("invalid machining parameters: {0}")]
    Invalid(String),
    /// An [`Invalid`](GenError::Invalid) refusal about one spot: `at` is
    /// that spot in the generator's frame. Displays as `Invalid` does.
    #[error("invalid machining parameters: {msg}")]
    At { msg: String, at: Point3 },
    /// An [`Invalid`](GenError::Invalid) refusal raised with a path in
    /// hand (#261 part 3): `ghost` is the moves built so far, `at` the
    /// spot it names if any, both in the generator's frame. Displays as
    /// `Invalid` does.
    #[error("invalid machining parameters: {msg}")]
    Ghosted { msg: String, at: Option<Point3>, ghost: Ghost },
    #[error("nothing to machine: {0}")]
    Empty(String),
    #[error(transparent)]
    Geo(#[from] GeoError),
}

impl GenError {
    /// An invalid-parameters refusal about the spot `at`. A non-finite
    /// spot is no place to show: the refusal stays, unlocated, so a bad
    /// coordinate can never move the camera to NaN.
    #[must_use]
    pub fn invalid_at(msg: impl Into<String>, at: Point3) -> Self {
        let msg = msg.into();
        if at.x.is_finite() && at.y.is_finite() && at.z.is_finite() {
            GenError::At { msg, at }
        } else {
            GenError::Invalid(msg)
        }
    }

    /// The spot this refusal names, if it names one.
    #[must_use]
    pub fn location(&self) -> Option<Point3> {
        match self {
            GenError::At { at, .. } => Some(*at),
            GenError::Ghosted { at, .. } => *at,
            _ => None,
        }
    }

    /// The moves a refused generation had built, if it handed them back
    /// (#261 part 3) - display-only, never a toolpath.
    #[must_use]
    pub fn ghost(&self) -> Option<&Ghost> {
        match self {
            GenError::Ghosted { ghost, .. } => Some(ghost),
            _ => None,
        }
    }

    /// This invalid-parameters refusal with `path` - the moves built when
    /// it was raised - handed back as its ghost. A path with nothing
    /// finite to show, and any other kind of refusal, pass untouched.
    #[must_use]
    pub fn with_ghost(self, path: &crate::cl::Toolpath) -> Self {
        self.with_ghost_moves(&path.moves)
    }

    /// [`GenError::with_ghost`] over a bare move list: for a refusal
    /// raised part-way, with only a builder's moves in hand (#871).
    #[must_use]
    pub fn with_ghost_moves(self, moves: &[crate::cl::Move]) -> Self {
        let (msg, at) = match self {
            GenError::Invalid(msg) => (msg, None),
            GenError::At { msg, at } => (msg, Some(at)),
            other => return other,
        };
        match Ghost::of_moves(moves) {
            Some(ghost) => GenError::Ghosted { msg, at, ghost },
            None => match at {
                Some(at) => GenError::At { msg, at },
                None => GenError::Invalid(msg),
            },
        }
    }

    /// #1015: a nested refusal's merge. `outer` are the moves the
    /// enclosing generator had built before the inner refusal, in the
    /// same frame: they stand in front of the inner ghost (or become
    /// the ghost, when the inner refusal carried none). A join with
    /// nothing finite keeps the inner ghost as it was; other kinds of
    /// refusal pass untouched.
    #[must_use]
    pub fn after_moves(self, outer: &[crate::cl::Move]) -> Self {
        match self {
            GenError::Ghosted { msg, at, ghost } => {
                let ghost = ghost.after(outer).unwrap_or(ghost);
                GenError::Ghosted { msg, at, ghost }
            }
            other => other.with_ghost_moves(outer),
        }
    }

    /// The same refusal with its ghost carried through `f` (the frame's
    /// local-to-world map of a path). A ghost `f` leaves nothing finite
    /// of is dropped, never drawn in the wrong frame.
    #[must_use]
    pub fn map_ghost(self, f: impl FnOnce(&crate::cl::Toolpath) -> crate::cl::Toolpath) -> Self {
        match self {
            GenError::Ghosted { msg, at, ghost } => match ghost.map_path(f) {
                Some(ghost) => GenError::Ghosted { msg, at, ghost },
                None => GenError::Invalid(msg).located(at),
            },
            other => other,
        }
    }

    /// The same refusal without its ghost: for a caller that ran the
    /// generator in a space it maps no path out of (a rotary wrap's
    /// development), where the moves would draw in the wrong place.
    #[must_use]
    pub fn without_ghost(self) -> Self {
        match self {
            GenError::Ghosted { msg, at, .. } => GenError::Invalid(msg).located(at),
            other => other,
        }
    }

    /// The same refusal with its sentence rewritten by `f` (an
    /// invalid-parameters one, located or ghosted alike); spot and ghost
    /// ride along. Other kinds pass untouched.
    #[must_use]
    pub fn map_msg(self, f: impl FnOnce(String) -> String) -> Self {
        match self {
            GenError::Invalid(m) => GenError::Invalid(f(m)),
            GenError::At { msg, at } => GenError::invalid_at(f(msg), at),
            GenError::Ghosted { msg, at, ghost } => GenError::Ghosted { msg: f(msg), at, ghost },
            other => other,
        }
    }

    /// An `Invalid` refusal given the spot `at`, if any.
    fn located(self, at: Option<Point3>) -> Self {
        match (self, at) {
            (GenError::Invalid(msg), Some(at)) => GenError::invalid_at(msg, at),
            (e, _) => e,
        }
    }

    /// The same refusal with its spot carried through `f` (a frame's
    /// local-to-world map). A spot `f` sends to a non-finite point is
    /// dropped, never shown; an unlocated refusal passes untouched.
    #[must_use]
    pub fn map_location(self, f: impl FnOnce(Point3) -> Point3) -> Self {
        match self {
            GenError::At { msg, at } => GenError::invalid_at(msg, f(at)),
            GenError::Ghosted { msg, at, ghost } => {
                let at = at.map(f).filter(|p| p.x.is_finite() && p.y.is_finite() && p.z.is_finite());
                GenError::Ghosted { msg, at, ghost }
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_located_refusal_reads_exactly_like_an_invalid_one() {
        let at = GenError::invalid_at("hole at (1, 2) is too small", Point3::new(1.0, 2.0, -3.0));
        let plain = GenError::Invalid("hole at (1, 2) is too small".into());
        assert_eq!(at.to_string(), plain.to_string());
        assert_eq!(at.location(), Some(Point3::new(1.0, 2.0, -3.0)));
        assert_eq!(plain.location(), None);
    }

    #[test]
    fn a_non_finite_spot_is_dropped_never_shown() {
        let e = GenError::invalid_at("x", Point3::new(f64::NAN, 0.0, 0.0));
        assert_eq!(e, GenError::Invalid("x".into()));
        let mapped = GenError::invalid_at("y", Point3::new(1.0, 1.0, 1.0)).map_location(|_| Point3::new(0.0, f64::INFINITY, 0.0));
        assert_eq!(mapped, GenError::Invalid("y".into()));
    }

    #[test]
    fn the_spot_rides_the_frame_map_and_unlocated_refusals_pass() {
        let e = GenError::invalid_at("z", Point3::new(1.0, 2.0, 3.0)).map_location(|p| Point3::new(p.x + 10.0, p.y, p.z));
        assert_eq!(e.location(), Some(Point3::new(11.0, 2.0, 3.0)));
        let empty = GenError::Empty("nothing".into()).map_location(|_| Point3::new(9.0, 9.0, 9.0));
        assert_eq!(empty, GenError::Empty("nothing".into()));
    }

    fn built() -> crate::cl::Toolpath {
        use crate::cl::{Move, MoveKind};
        let m = Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, -1.0), Point3::new(5.0, 0.0, -1.0), Some(500.0));
        crate::cl::Toolpath { moves: vec![m], ..Default::default() }
    }

    #[test]
    fn a_ghosted_refusal_reads_exactly_like_the_refusal_without_it() {
        let plain = GenError::Invalid("entry refused: x".into());
        let g = GenError::Invalid("entry refused: x".into()).with_ghost(&built());
        assert_eq!(g.to_string(), plain.to_string());
        assert_eq!(g.ghost().map(Ghost::len), Some(1));
        assert_eq!(g.location(), None);
        assert!(format!("{g:?}").contains("Ghost(1 moves)"), "Debug names the size, never the path: {g:?}");
        // A located refusal keeps its spot beside the ghost.
        let at = GenError::invalid_at("y", Point3::new(1.0, 2.0, 3.0)).with_ghost(&built());
        assert_eq!(at.location(), Some(Point3::new(1.0, 2.0, 3.0)));
        assert!(at.ghost().is_some());
        // Nothing to show: the refusal stays as it was.
        assert_eq!(GenError::Invalid("z".into()).with_ghost(&crate::cl::Toolpath::default()), GenError::Invalid("z".into()));
        assert_eq!(GenError::Empty("e".into()).with_ghost(&built()), GenError::Empty("e".into()));
    }

    #[test]
    fn the_ghost_rides_the_frame_and_the_sentence_rewrites_keep_it() {
        let g = GenError::invalid_at("w", Point3::new(1.0, 0.0, 0.0)).with_ghost(&built());
        let g = g.map_location(|p| Point3::new(p.x + 10.0, p.y, p.z)).map_ghost(|t| {
            let mut t = t.clone();
            t.moves[0].to.x += 10.0;
            t
        });
        assert_eq!(g.location(), Some(Point3::new(11.0, 0.0, 0.0)));
        assert_eq!(g.ghost().map(|h| h.moves()[0].to.x), Some(15.0));
        let g = g.map_msg(|m| format!("{m} (why)"));
        assert_eq!(g.to_string(), "invalid machining parameters: w (why)");
        assert!(g.ghost().is_some(), "a rewrite keeps the ghost");
        // A frame that leaves nothing finite drops the ghost, keeps the spot.
        let dropped = g.map_ghost(|_| crate::cl::Toolpath::default());
        assert_eq!(dropped, GenError::invalid_at("w (why)", Point3::new(11.0, 0.0, 0.0)));
    }

    #[test]
    fn a_nested_refusal_shows_the_outer_moves_then_its_own() {
        // #1015: the inner leg refused with its own ghost; the outer
        // generator's moves go in front, the words and spot stay.
        use crate::cl::{Move, MoveKind};
        let outer = [Move::new(MoveKind::Rapid, Point3::new(-5.0, 0.0, 9.0), Point3::new(0.0, 0.0, -1.0), None)];
        let inner = GenError::invalid_at("inner", Point3::new(5.0, 0.0, -1.0)).with_ghost(&built());
        let g = inner.after_moves(&outer);
        assert_eq!(g.to_string(), "invalid machining parameters: inner");
        assert_eq!(g.location(), Some(Point3::new(5.0, 0.0, -1.0)));
        let xs: Vec<f64> = g.ghost().map(|h| h.moves().iter().map(|m| m.to.x).collect()).unwrap_or_default();
        assert_eq!(xs, vec![0.0, 5.0], "outer first, then the inner ghost");
        // An inner refusal with no ghost takes the outer moves as one.
        let plain = GenError::Invalid("p".into()).after_moves(&outer);
        assert_eq!(plain.ghost().map(Ghost::len), Some(1));
        // Nothing outer: the inner ghost stands; other kinds pass.
        assert_eq!(GenError::Invalid("q".into()).with_ghost(&built()).after_moves(&[]).ghost().map(Ghost::len), Some(1));
        assert_eq!(GenError::Empty("e".into()).after_moves(&outer), GenError::Empty("e".into()));
    }
}
