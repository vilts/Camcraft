//! #1445: the drill page's Cycle combo - every cycle has one label, the
//! label picks the cycle back, and the ream and the fine bore read in
//! plain English. The combo's own data, asserted structurally (no pixels).

use crate::operations::{cycle_from_index, cycle_index, cycle_names};
use camcraft_app::drilling::CycleChoice;

/// A fine bore as the combo picks it: no shift stated.
const FINE_UNSET: CycleChoice = CycleChoice::FineBore { shift_x_um: 0, shift_y_um: 0 };

const ALL: [CycleChoice; 10] = [
    CycleChoice::Auto,
    CycleChoice::Spot,
    CycleChoice::Drill,
    CycleChoice::Peck,
    CycleChoice::Tap,
    CycleChoice::Bore,
    CycleChoice::BackBore,
    CycleChoice::FloatingTap,
    CycleChoice::Ream,
    FINE_UNSET,
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

#[test]
fn the_fine_bore_reads_in_plain_english_and_its_pick_states_no_shift() {
    let stated = CycleChoice::FineBore { shift_x_um: 300, shift_y_um: -400 };
    let i = cycle_index(&stated);
    assert_eq!(cycle_names()[i], "Fine bore (oriented stop, shift)");
    assert_eq!(cycle_index(&FINE_UNSET), i, "the label is the cycle's, whatever its shift");
    // No default shift: the pick is zero, which the command refuses by name.
    assert_eq!(cycle_from_index(i), FINE_UNSET);
}
