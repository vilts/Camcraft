//! #1445: the drill page's Cycle combo - every cycle has one label, the
//! label picks the cycle back, and the ream reads in plain English. The
//! combo's own data, asserted structurally (no pixels).

use crate::operations::{cycle_from_index, cycle_index, cycle_names};
use camcraft_app::drilling::CycleChoice;

const ALL: [CycleChoice; 9] = [
    CycleChoice::Auto,
    CycleChoice::Spot,
    CycleChoice::Drill,
    CycleChoice::Peck,
    CycleChoice::Tap,
    CycleChoice::Bore,
    CycleChoice::BackBore,
    CycleChoice::FloatingTap,
    CycleChoice::Ream,
];

#[test]
fn every_cycle_has_one_label_and_picks_itself_back() {
    let names = cycle_names();
    assert_eq!(names.len(), ALL.len(), "one label per cycle: {names:?}");
    let mut seen = vec![false; names.len()];
    for c in ALL {
        let i = cycle_index(&c);
        assert!(i < names.len(), "{c:?} indexes past the list");
        assert!(!seen[i], "{c:?} shares label {:?}", names[i]);
        seen[i] = true;
        assert_eq!(cycle_from_index(i), c, "label {:?} picks {c:?} back", names[i]);
    }
}

#[test]
fn the_ream_reads_in_plain_english_and_an_unknown_index_is_auto() {
    assert_eq!(cycle_names()[cycle_index(&CycleChoice::Ream)], "Ream (feed in, feed out)");
    assert_eq!(cycle_from_index(cycle_names().len()), CycleChoice::Auto, "never a neighbouring cycle");
}
