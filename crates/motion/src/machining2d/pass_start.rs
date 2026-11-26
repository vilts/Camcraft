//! Where the passes of one level start (#3316): the side of the area the
//! level begins on. One shared type for every kind with a pass order
//! (#1448 adds the start point beside it). The law is ours, from general
//! machining practice (cut from the open side so each pass engages one
//! stepover); no vendor material consulted.

use crate::entry::Entry3D;
use crate::geo2d::Region;
use crate::verify::AdvisedEntry;

use super::pocket_hsm::RINGS_FALLBACK;

/// The first words of every note the start-side planner writes: the op
/// page shows each one as a warning line.
pub const PASS_START_NOTE: &str = "Pass start";

/// Which side of the cutter-center area a level starts on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PassStart {
    /// The order each kind always cut: for the ring planner the core
    /// first, then outward. Every file before the option, byte for byte.
    #[default]
    Auto,
    /// Start at the area's holes (an island wall, the part wall of an
    /// Outside band) and walk out to its outer boundary.
    Inside,
    /// Start at the area's outer boundary (a pocket wall, the stock edge
    /// of an Outside band) and walk in to its holes.
    Outside,
}

/// The note of a Zigzag plan with Inside or Outside (#3354): rows sweep
/// across the area and have no inside or outside, so the choice is not
/// used - said by name, never dropped silently.
pub const PASS_START_ZIGZAG_NOTE: &str =
    "Pass starts at: Zigzag rows have no inside or outside order; the choice is used only by the ring strategies (Waterline, Offset rings)";

/// The note of a rest pocket with Inside or Outside (#3354): rest
/// machining walks its own rest areas, not the pocket's rings.
pub const PASS_START_REST_NOTE: &str =
    "Pass starts at: rest machining keeps its own order of the rest areas; the choice is not used";

/// #3748: the advice note of a start-side pass whose user selected a
/// lead-in other than Arc - the selected type, the recommended Arc and the
/// reason, in one sentence ([`crate::verify::entry_advice`]).
/// `None` for Arc and for Auto. The ladder writes it on each ring pass
/// with Inside or Outside, with or without an air room (#3884); the operation turns
/// it into one `Warning::EntryNotRecommended`, and the Lead in page shows
/// the same text next to the selector.
#[must_use]
pub fn entry_advice(start: PassStart, entry: Entry3D) -> Option<&'static str> {
    let (kind, inside) = advised(start, entry)?;
    Some(crate::verify::entry_advice(kind, inside))
}

/// The ladder's note of [`entry_advice`]: an `entry:` note, so that the op
/// page does not show it as a "Pass start" warning line next to the one
/// `Warning::EntryNotRecommended` that it posts.
#[must_use]
pub fn entry_advice_note(start: PassStart, entry: Entry3D) -> Option<String> {
    entry_advice(start, entry).map(|a| format!("entry: {a}"))
}

/// The selected type and the side that [`entry_advice`] names.
#[must_use]
pub fn advised(start: PassStart, entry: Entry3D) -> Option<(AdvisedEntry, bool)> {
    let kind = match entry {
        Entry3D::Plunge => AdvisedEntry::Plunge,
        Entry3D::Ramp => AdvisedEntry::Ramp,
        Entry3D::Helix => AdvisedEntry::Helix,
        Entry3D::Arc => return None,
    };
    match start {
        PassStart::Inside => Some((kind, true)),
        PassStart::Outside => Some((kind, false)),
        PassStart::Auto => None,
    }
}

impl PassStart {
    /// Every value, in the order the page lists them.
    pub const ALL: [PassStart; 3] = [PassStart::Auto, PassStart::Inside, PassStart::Outside];

    /// The name the page and the notes use.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            PassStart::Auto => "Auto",
            PassStart::Inside => "Inside",
            PassStart::Outside => "Outside",
        }
    }
}

/// The start side and what the planner needs to judge it: where the
/// stock is, and what the area's holes are.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StartSide {
    pub start: PassStart,
    /// The stock's footprint in the area's frame, when it is known. A
    /// start ring whose disc stands off it runs in open air; without it
    /// every start ring is read as a wall in uncut stock.
    pub stock: Option<Region>,
    /// The area surrounds the part (an Outside band): its holes are the
    /// part, not islands.
    pub around_part: bool,
}

impl From<PassStart> for StartSide {
    fn from(start: PassStart) -> Self {
        StartSide { start, stock: None, around_part: false }
    }
}

impl StartSide {
    /// The warning for a level whose first ring runs along a wall in
    /// uncut stock, so that it cuts the full tool width (#3316): the
    /// outer wall of a closed pocket for Outside, the part or island
    /// wall for Inside. `None` for Auto.
    #[must_use]
    pub fn wall_first_note(&self) -> Option<String> {
        match (self.start, self.around_part) {
            (PassStart::Auto, _) => None,
            (PassStart::Outside, _) => Some(format!(
                "{PASS_START_NOTE} Outside: the outer edge of this area is a wall in the stock, not the open \
                 stock edge, so each level starts with a cut of the full tool width along that wall"
            )),
            (PassStart::Inside, true) => Some(format!(
                "{PASS_START_NOTE} Inside: the inner edge of this area is the part wall, so each level starts \
                 with a cut of the full tool width along the part"
            )),
            (PassStart::Inside, false) => Some(format!(
                "{PASS_START_NOTE} Inside: the inner edge of this area is an island wall, so each level starts \
                 with a cut of the full tool width along the island"
            )),
        }
    }

    /// The note for `n` parts of the area with no hole to start Inside
    /// from (material inside a hole): cut core first, as Auto cuts them.
    #[must_use]
    pub fn core_note(&self, n: usize) -> String {
        format!(
            "{PASS_START_NOTE} {}: {n} part(s) of this area have no hole to start from, so they are cut from the middle outward, as Auto cuts them",
            self.start.label()
        )
    }

    /// The warning when the rings from the start side would leave
    /// `area` mm2 uncut: the level is cut in the Auto order instead, so
    /// nothing is left standing (the completeness guard, #3316). The
    /// note carries [`RINGS_FALLBACK`], so the op raises the advisory
    /// `Warning::StrategyFallback` for it (#3035), as every other
    /// fallback to the Offset rings does.
    #[must_use]
    pub fn fallback_note(&self, area: f64) -> String {
        format!(
            "{PASS_START_NOTE} {}: the rings from the start side would leave {area:.1} mm2 of this area uncut{RINGS_FALLBACK}{PASS_START_FALLBACK_TAIL}",
            self.start.label()
        )
    }

    /// The warning for cuts after the first ring that still take the
    /// full tool width (#3316), by the flanks law (#3788): `n` of them,
    /// the first at `(x, y)`.
    #[must_use]
    pub fn narrow_note(&self, n: usize, x: f64, y: f64) -> String {
        format!(
            "{PASS_START_NOTE} {}: {n} cut(s) take the full tool width where the area is narrower than the \
             tool plus one stepover, where a wall hides the start side, or where two rings meet at a corner; the first is at X {x:.3}, Y {y:.3}",
            self.start.label()
        )
    }
}

/// The last words of the completeness guard's note (#3542): no Adaptive
/// note ends so ("... instead" / "... in that order instead"), and a
/// caller may put its own words before the note (3D pocket: "Z 1.000: ").
const PASS_START_FALLBACK_TAIL: &str = "this area in the Auto order instead";

/// How many of the Offset-rings fallbacks in `notes` (the ones
/// [`super::pocket_hsm::rings_fallbacks`] counts) are this planner's
/// completeness guard, not the Adaptive plan (#3542): the op names the
/// cause from the note that made the fallback, never from its kind. The
/// match does not read the start of the note, which a caller can prefix.
#[must_use]
pub fn pass_start_fallbacks(notes: &[String]) -> usize {
    notes
        .iter()
        .filter(|n| {
            n.contains(PASS_START_NOTE)
                && n.contains(&format!("{RINGS_FALLBACK}{PASS_START_FALLBACK_TAIL}"))
                && n.ends_with(PASS_START_FALLBACK_TAIL)
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::{PASS_START_NOTE, PassStart, StartSide, pass_start_fallbacks};
    use crate::machining2d::rings_fallbacks;

    /// #3316/#3035: the completeness guard's note is an Offset-rings
    /// fallback, so `rings_fallbacks` sees it and the op posts the
    /// advisory `Warning::StrategyFallback` for the area. The other
    /// "Pass start" notes are not fallbacks and count none.
    #[test]
    fn the_guard_fallback_is_an_offset_rings_fallback() {
        let side = StartSide::from(PassStart::Outside);
        let note = side.fallback_note(12.3);
        assert!(note.starts_with(PASS_START_NOTE), "{note}");
        assert_eq!(rings_fallbacks(std::slice::from_ref(&note)), 1, "{note}");
        // #3542: the guard's note is a pass-start fallback; an Adaptive
        // note with a pass start ("adaptive: Pass starts at ...") is not.
        assert_eq!(pass_start_fallbacks(std::slice::from_ref(&note)), 1, "{note}");
        let adaptive = "adaptive: Pass starts at Outside - the HSM plan failed - Offset rings cut the pocket in that order instead".to_owned();
        assert_eq!(rings_fallbacks(std::slice::from_ref(&adaptive)), 1, "{adaptive}");
        assert_eq!(pass_start_fallbacks(std::slice::from_ref(&adaptive)), 0, "{adaptive}");
        let plain = "adaptive: the HSM plan failed - Offset rings cut level at Z 1.00 instead".to_owned();
        assert_eq!(pass_start_fallbacks(std::slice::from_ref(&plain)), 0, "{plain}");
        // The 3D pocket note carries its level first (`level_side`): still
        // a pass-start fallback, and still one ring fallback.
        let level = format!("Z 1.000: {note}");
        assert_eq!(rings_fallbacks(std::slice::from_ref(&level)), 1, "{level}");
        assert_eq!(pass_start_fallbacks(std::slice::from_ref(&level)), 1, "{level}");
        let wall = side.wall_first_note().unwrap_or_default();
        assert_eq!(rings_fallbacks(std::slice::from_ref(&wall)), 0, "{wall}");
        let narrow = side.narrow_note(1, 0.0, 0.0);
        assert_eq!(rings_fallbacks(std::slice::from_ref(&narrow)), 0, "{narrow}");
        let core = StartSide::from(PassStart::Inside).core_note(1);
        assert_eq!(rings_fallbacks(std::slice::from_ref(&core)), 0, "{core}");
    }

    #[test]
    fn auto_says_nothing_and_each_wall_case_is_named() {
        assert_eq!(PassStart::default(), PassStart::Auto);
        assert_eq!(StartSide::from(PassStart::Auto).wall_first_note(), None);
        let out = StartSide::from(PassStart::Outside).wall_first_note().unwrap_or_default();
        assert!(out.starts_with(PASS_START_NOTE) && out.contains("outer edge"), "{out}");
        let part = StartSide { start: PassStart::Inside, stock: None, around_part: true };
        assert!(part.wall_first_note().is_some_and(|n| n.contains("part wall")));
        let island = StartSide::from(PassStart::Inside).wall_first_note().unwrap_or_default();
        assert!(island.contains("island wall"), "{island}");
        let narrow = StartSide::from(PassStart::Outside).narrow_note(2, 1.0, -2.5);
        assert!(narrow.starts_with("Pass start Outside: 2 cut(s)") && narrow.contains("X 1.000, Y -2.500"), "{narrow}");
    }
}
