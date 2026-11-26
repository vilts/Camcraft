//! The start-side entry step (#3843): an edit that moves a 2D Offset
//! from "Pass starts at" Auto to Inside or Outside, and keeps the seed
//! Helix entry, takes Arc in the same edit. A start-side pass enters in
//! the plane of its cut (the maintainer's words on #3741).
//!
//! Provenance: our own edit-door contract, after `tilt_basis`; no
//! vendor material.

use super::params::OpParams;
use crate::machining2d::PassStart;
use crate::machining3d::Entry3D;

impl OpParams {
    /// The edit door's start-side entry law (#3843). It acts only when
    /// this edit moves `pass_start` off Auto and leaves the entry as
    /// the seed Helix. An entry that the same edit changes, or a
    /// Plunge or Ramp, stays as the user set it. A later edit can
    /// select Helix again on an Inside or Outside pass.
    ///
    /// When `named_entry` is true (#3863), the request explicitly chose an
    /// entry in the same edit or create, so the law does not override it.
    #[must_use]
    pub fn with_start_side_entry(mut self, prev: &OpParams, named_entry: bool) -> OpParams {
        if named_entry {
            return self;
        }
        let OpParams::Offset2D { entry: before, pass_start: start_before, .. } = prev else {
            return self;
        };
        if let OpParams::Offset2D { entry, pass_start, .. } = &mut self {
            let to_start_side = *start_before == PassStart::Auto && *pass_start != PassStart::Auto;
            if to_start_side && *before == Entry3D::Helix && *entry == Entry3D::Helix {
                *entry = Entry3D::Arc;
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::operation::{OpType, seed_params};

    use super::*;

    fn offset(start: PassStart, with: Entry3D) -> OpParams {
        let mut p = seed_params(OpType::Offset2D.ordinal());
        if let OpParams::Offset2D { entry, pass_start, .. } = &mut p {
            *entry = with;
            *pass_start = start;
        }
        p
    }

    fn entry_of(p: &OpParams) -> Option<Entry3D> {
        match p {
            OpParams::Offset2D { entry, .. } => Some(*entry),
            _ => None,
        }
    }

    #[test]
    fn a_seeded_offset_moved_to_a_start_side_takes_arc() {
        let seed = seed_params(OpType::Offset2D.ordinal());
        assert_eq!(entry_of(&seed), Some(Entry3D::Helix), "the seed is Helix");
        for start in [PassStart::Inside, PassStart::Outside] {
            let next = offset(start, Entry3D::Helix).with_start_side_entry(&seed, false);
            assert_eq!(entry_of(&next), Some(Entry3D::Arc), "{start:?}");
        }
    }

    #[test]
    fn a_user_choice_of_entry_stays() {
        let auto_helix = offset(PassStart::Auto, Entry3D::Helix);
        // The same edit selects Plunge: the user's choice stays.
        let next = offset(PassStart::Outside, Entry3D::Plunge).with_start_side_entry(&auto_helix, false);
        assert_eq!(entry_of(&next), Some(Entry3D::Plunge));
        // The same edit selects Helix off a Plunge: the user's choice stays.
        let auto_plunge = offset(PassStart::Auto, Entry3D::Plunge);
        let next = offset(PassStart::Outside, Entry3D::Helix).with_start_side_entry(&auto_plunge, false);
        assert_eq!(entry_of(&next), Some(Entry3D::Helix));
        // A Ramp set before the start side moves: it stays.
        let auto_ramp = offset(PassStart::Auto, Entry3D::Ramp);
        let next = offset(PassStart::Inside, Entry3D::Ramp).with_start_side_entry(&auto_ramp, false);
        assert_eq!(entry_of(&next), Some(Entry3D::Ramp));
        // Helix selected again on an Outside pass: it stays.
        let outside_arc = offset(PassStart::Outside, Entry3D::Arc);
        let next = offset(PassStart::Outside, Entry3D::Helix).with_start_side_entry(&outside_arc, false);
        assert_eq!(entry_of(&next), Some(Entry3D::Helix));
        // A Helix kept on Inside, moved to Outside: it stays.
        let inside_helix = offset(PassStart::Inside, Entry3D::Helix);
        let next = offset(PassStart::Outside, Entry3D::Helix).with_start_side_entry(&inside_helix, false);
        assert_eq!(entry_of(&next), Some(Entry3D::Helix));
        // Back to Auto: the entry stays.
        let next = offset(PassStart::Auto, Entry3D::Arc).with_start_side_entry(&outside_arc, false);
        assert_eq!(entry_of(&next), Some(Entry3D::Arc));
        // #3863: an explicit Helix in the same request stays.
        let next = offset(PassStart::Inside, Entry3D::Helix).with_start_side_entry(&auto_helix, true);
        assert_eq!(entry_of(&next), Some(Entry3D::Helix));
    }
}
