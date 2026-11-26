//! The quality presets of the Display menu (#2834): "Performance" and
//! "Quality". A preset is a set of values for the shading switches. No
//! preset is stored: the active preset is the one whose values the
//! switches have now, so a switch changed by hand leaves no preset active
//! and the menu can never show a preset that the view does not have.
//! Provenance: our own wording and our own preset contents; no vendor text.

use crate::draw_list::Effects;
use crate::state::State;

/// A named set of values for the shading switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualityPreset {
    /// Each shading switch off: the cheapest frame.
    Performance,
    /// Each shading switch on.
    Quality,
}

impl QualityPreset {
    /// The presets in menu order.
    pub const ALL: [QualityPreset; 2] = [QualityPreset::Performance, QualityPreset::Quality];

    /// The menu label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            QualityPreset::Performance => "Performance",
            QualityPreset::Quality => "Quality",
        }
    }

    /// What the preset does, for the tooltip.
    #[must_use]
    pub const fn tooltip(self) -> &'static str {
        match self {
            QualityPreset::Performance => {
                "Fastest drawing. The ground shadow and the ambient occlusion are off."
            }
            QualityPreset::Quality => {
                "Best picture. The ground shadow and the ambient occlusion are on. A large model can draw more slowly."
            }
        }
    }

    /// The switch values of the preset.
    #[must_use]
    pub const fn switches(self) -> Effects {
        match self {
            QualityPreset::Performance => Effects { ground_shadow: false, ambient_occlusion: false },
            QualityPreset::Quality => Effects { ground_shadow: true, ambient_occlusion: true },
        }
    }

    /// The preset that has exactly the values `now`, if one has.
    #[must_use]
    pub fn active(now: Effects) -> Option<QualityPreset> {
        QualityPreset::ALL.into_iter().find(|p| p.switches() == now)
    }

    /// Set the switches of `state` to the values of the preset. View state:
    /// a direct field write, never a command.
    pub fn apply(self, state: &mut State) {
        let fx = self.switches();
        state.ground_shadow = fx.ground_shadow;
        state.ambient_occlusion = fx.ambient_occlusion;
    }
}

#[cfg(test)]
mod tests {
    use super::QualityPreset;
    use crate::draw_list::Effects;
    use crate::state::State;

    #[test]
    fn each_preset_has_its_own_words_and_values_and_reads_back() {
        let [a, b] = QualityPreset::ALL;
        assert_ne!(a.label(), b.label());
        assert_ne!(a.switches(), b.switches());
        for p in QualityPreset::ALL {
            assert!(!p.tooltip().is_empty());
            assert_eq!(QualityPreset::active(p.switches()), Some(p));
            let mut st = State::new();
            p.apply(&mut st);
            assert_eq!(st.effects(), p.switches());
        }
        assert_eq!(QualityPreset::active(Effects::default()), Some(QualityPreset::Performance), "the start");
        for mixed in [Effects { ground_shadow: true, ambient_occlusion: false }, Effects { ground_shadow: false, ambient_occlusion: true }] {
            assert_eq!(QualityPreset::active(mixed), None, "one switch changed: no preset");
        }
    }
}
