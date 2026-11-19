//! The status strip's alert reading (#192): a refusal on the status line
//! is drawn so it cannot be missed - the palette's error ink, bold, on an
//! error-tinted band, behind a "!" marker - until the next status
//! replaces it. Ordinary status text (saved, generated, ready) stays as
//! it was, dim. Provenance: our own UI law.

use crate::state::State;

/// Phrasings the shell's own refusals use when they set the status line
/// directly rather than through `State::refuse` (a dark cell's gate note,
/// an import or export that could not run, an origin that was not set).
const REFUSAL_MARKS: [&str; 8] = [
    " refused",
    " not created:",
    " failed",
    " not set:",
    " not changed:",
    " not aligned:",
    " no longer ",
    "not generated",
];

/// Does the status line carry a refusal? A command refused through the
/// feedback path (its message IS the status), a gate note ("cannot …"),
/// or one of the [`REFUSAL_MARKS`] phrasings.
pub(super) fn is_alert(state: &State) -> bool {
    let s = state.status.as_str();
    if s.is_empty() {
        return false;
    }
    state.feedback.as_ref().is_some_and(|(_, m)| m == s)
        || s.starts_with("cannot ")
        || REFUSAL_MARKS.iter().any(|mark| s.contains(mark))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::CommandKind;

    #[test]
    fn ordinary_status_is_not_an_alert() {
        let mut state = State::new();
        for s in ["ready", "saved /tmp/a.camproj", "generated Contour1: 105 moves", "stock set", ""] {
            state.status = s.to_owned();
            assert!(!is_alert(&state), "{s:?} read as an alert");
        }
    }

    /// #2280: the notes of an operation with 0 warnings are not an
    /// error. The text is the status line the maintainer saw, without
    /// the playback refusal that #2280 removed from its end.
    #[test]
    fn the_notes_of_a_clean_operation_are_not_an_alert() {
        let mut state = State::new();
        state.status = "generated 2D Adaptive1: 4269 moves (0 plunge(s), 3 level(s), 1617 arc(s)), 0 warning(s) -             arc output: of 789 fitted arcs, 169 could not be proved inside the cutter-centre area, 85 would add a kink             and 2 would cross another feed - those stay chords - Carried stock: the operation's top Z20.000 is below             the material over its region (Z22.000) - the first descent starts over that material and feeds down;             the first level engages 2.000 mm"
            .to_owned();
        assert!(!is_alert(&state), "{}", state.status);
    }

    #[test]
    fn refusals_are_alerts() {
        let mut state = State::new();
        for s in [
            "cannot add an operation: no tool in the document - add one (Setup > Tools)",
            "import refused: not a mesh",
            "save failed: permission denied",
            "origin not set: the picked face is gone",
            "WCS not aligned: pick a flat face - it sets the Z axis",
            "the picked vertex of 'block' is no longer in the model",
            // #215's two, verbatim.
            "operation not created: invalid parameters: no stock; set up stock first",
            "operation not created: invalid parameters: a 2D Contour operation must cut downward: top 0 below bottom 5",
        ] {
            state.status = s.to_owned();
            assert!(is_alert(&state), "{s:?} not read as an alert");
        }
    }

    /// #215: routine status is drawn in the body text ink (the theme's
    /// `text`, which the visuals' `override_text_color` carries), and that
    /// ink on the strip's own fill meets the body-text contrast gate in
    /// both built-in themes - the old "weak" grey did not read.
    #[test]
    fn routine_status_ink_reads_on_the_strip() {
        use crate::egui_app::contrast;
        use crate::egui_app::theme::Tokens;
        for (name, t) in [("dark", Tokens::DARK), ("light", Tokens::LIGHT)] {
            let r = contrast::ratio(t.text, t.strip_fill());
            assert!(r >= contrast::BODY, "{name}: status text {:?} on {:?} reads {r:.2}:1", t.text, t.strip_fill());
            assert_eq!(t.visuals().override_text_color, Some(t.text), "{name}: the label ink is the body text");
        }
    }

    #[test]
    fn a_command_refused_through_the_feedback_path_is_an_alert() {
        let mut state = State::new();
        // Deleting what is not there is refused by the command door.
        state.pending.push_back(CommandKind::DeleteByName("nobody".into()));
        state.drain_actions();
        assert!(state.feedback.is_some(), "the refusal is recorded: {}", state.status);
        assert!(is_alert(&state), "{}", state.status);
        state.status = "ready".into();
        assert!(!is_alert(&state), "a later status retires the alert");
    }
}
