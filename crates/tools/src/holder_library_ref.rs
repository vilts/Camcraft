//! #2016: a tool's holder as a reference to a library entry. The tool
//! states which built-in holder it sits in and the two lengths that
//! place it; the outline is drawn from the entry each time it is
//! needed, so a tool never carries a hand-sized copy of the holder.
//! The entry is named exactly (one bore, one body diameter): a grouped
//! pick is resolved before it becomes a reference, and a name the
//! library does not hold is refused, never replaced by another holder.
//! Our own record over the library's tables.

use super::{find, HolderEntry, LibraryError};
use crate::spun::SpunPoint;

/// A library holder on a tool: the entry's name as
/// [`HolderEntry::name`] states it, and where it sits (mm).
#[derive(Clone, Debug, PartialEq)]
pub struct HolderRef {
    /// The library entry, "BT30 ER20" or "BT40 Weldon 12 mm".
    pub entry: String,
    /// From the gauge line to the holder's nose face.
    pub gauge_len: f64,
    /// The nose face's height above the tool tip.
    pub projection: f64,
}

impl HolderRef {
    /// The reference to `entry` with these lengths, under the entry's
    /// own spelling of its name.
    #[must_use]
    pub fn to(entry: &HolderEntry, gauge_len: f64, projection: f64) -> Self {
        Self { entry: entry.name(), gauge_len, projection }
    }

    /// The entry this names. An unknown name is refused by name.
    pub fn resolve(&self) -> Result<HolderEntry, LibraryError> {
        find(&self.entry)
    }

    /// The holder's half-outline as the entry draws it at these
    /// lengths ([`HolderEntry::points`]), with every refusal that has.
    pub fn points(&self) -> Result<Vec<SpunPoint>, LibraryError> {
        self.resolve()?.points(self.projection, self.gauge_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::holder_library::library;

    #[test]
    fn a_reference_draws_its_entrys_outline_for_every_entry() {
        for e in library() {
            let r = HolderRef::to(&e, e.gauge_len, 52.0);
            assert_eq!(r.resolve().expect("entry"), e, "{}", e.name());
            assert_eq!(r.points().expect("outline"), e.points(52.0, e.gauge_len).expect("outline"), "{}", e.name());
        }
    }

    #[test]
    fn an_unknown_entry_and_a_grouped_pick_are_refused_by_name() {
        for name in ["BT30 ER99", "BT40 Weldon", ""] {
            let r = HolderRef { entry: name.to_owned(), gauge_len: 60.0, projection: 52.0 };
            assert_eq!(r.points(), Err(LibraryError::Unknown(name.to_owned())), "{name:?}");
        }
    }

    #[test]
    fn lengths_the_entry_refuses_are_refused() {
        let e = find("BT30 ER20").expect("entry");
        assert!(matches!(HolderRef::to(&e, 1.0, 52.0).points(), Err(LibraryError::TooShort { .. })));
        assert_eq!(HolderRef::to(&e, 60.0, -1.0).points(), Err(LibraryError::BelowTip));
        assert_eq!(HolderRef::to(&e, f64::NAN, 52.0).points(), Err(LibraryError::NonFinite));
    }
}
