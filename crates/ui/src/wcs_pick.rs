//! The WCS-from-features pick draft (#701): while a Setups page row
//! collects, every viewport feature click lands here instead of in the
//! open operation, stamped with the role chosen in the row's box -
//! Origin, X, Y or Z. Faces, edges, corners and curved walls all
//! answer; the toolbar's pick mode is the user's own. One pick per
//! role: re-picking a role replaces its pick in place, a mis-click
//! corrects itself without disarming. Ephemeral view state like the
//! plane and axes drafts - nothing here writes the world; Apply reads
//! `camcraft_app::pick::align_roles` and queues the undoable
//! `CommandKind::SetWcs` the WCS pane already uses.
//!
//! Provenance: our own UI over datum-plane/line/point practice; no
//! vendor material consulted.

use camcraft_app::operation::GeoSel;
use camcraft_app::pick::Role;

use crate::state::State;

/// The role's word, for labels and status lines (`Role::label` is the
/// app's own, private to its refusal strings).
pub fn role_label(role: Role) -> &'static str {
    match role {
        Role::Origin => "Origin",
        Role::X => "X",
        Role::Y => "Y",
        Role::Z => "Z",
    }
}

/// The picks being collected for one setup's WCS.
#[derive(Clone, Debug, PartialEq)]
pub struct WcsPick {
    /// The setup whose WCS the picks will move.
    pub setup: String,
    /// The role the next viewport pick is stamped with.
    pub role: Role,
    /// The picks, each with its stamped role: at most one per role.
    pub picks: Vec<(GeoSel, Role)>,
    /// Viewport clicks land here; `false` is a paused draft (#664: its
    /// page was left) - the picks stay, shown when the page is back.
    pub picking: bool,
}

/// Start (or restart) collecting role picks for `setup`. The plane and
/// axes drafts stop (#557: one draft collects at a time). The pick mode
/// stays as the toolbar left it - this row takes faces, edges, corners
/// and walls alike, so forcing one mode would blind the rest. A paused
/// draft of the same setup resumes with its picks kept (#664).
pub fn arm(state: &mut State, setup: &str) {
    crate::plane_pick::disarm(state);
    crate::axis_pick::disarm(state);
    state.points_pick = None; // #555: one draft collects at a time
    match state.wcs_pick.as_mut().filter(|d| d.setup == setup) {
        Some(d) => d.picking = true,
        None => state.wcs_pick =
            Some(WcsPick { setup: setup.to_owned(), role: Role::Z, picks: Vec::new(), picking: true }),
    }
    state.status = format!("{setup}: pick faces, edges, corners or walls for the WCS - each takes the role set in the box");
}

/// Stop collecting; the draft is dropped.
pub fn disarm(state: &mut State) {
    state.wcs_pick = None;
}

/// Stop collecting but keep the picks (#664: the page was left).
pub fn pause(state: &mut State) {
    if let Some(d) = state.wcs_pick.as_mut() {
        d.picking = false;
    }
}

/// Is a Setups page row collecting role picks?
#[must_use]
pub fn collecting(state: &State) -> bool {
    state.wcs_pick.as_ref().is_some_and(|d| d.picking)
}

/// One viewport pick into the draft, stamped with the current role. The
/// role's earlier pick is replaced in place - one pick per role, the
/// newest wins, the other roles stay put.
pub fn take(state: &mut State, pick: GeoSel) {
    let Some(draft) = state.wcs_pick.as_mut().filter(|d| d.picking) else { return };
    let role = draft.role;
    let setup = draft.setup.clone();
    let what = format!("{} of '{}'", pick.feature, pick.entity);
    match draft.picks.iter_mut().position(|(_, r)| *r == role) {
        Some(i) => draft.picks[i] = (pick, role),
        None => draft.picks.push((pick, role)),
    }
    state.status = format!("{setup}: {what} picked as {}", role_label(role));
}

#[cfg(test)]
mod tests {
    use super::*;
    use camcraft_app::geometry::{Point3, cube};
    use camcraft_app::operation::FeatureSel;
    use camcraft_app::commands::ImportModel;

    fn part_state() -> State {
        let mut state = State::new();
        let part = ImportModel::from_mesh("part", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0)));
        state.history.execute(&state.world, part);
        state
    }

    fn face_pick() -> GeoSel {
        GeoSel { entity: "part".into(), feature: FeatureSel::Face(1) }
    }

    #[test]
    fn arming_defaults_the_role_box_to_z_and_stops_the_other_drafts() {
        let mut state = part_state();
        crate::plane_pick::arm(&mut state, "Setup1");
        crate::wcs_pick::arm(&mut state, "Setup1");
        assert!(!crate::plane_pick::collecting(&state), "the plane draft let go");
        let d = state.wcs_pick.clone().expect("armed");
        assert!(d.picking);
        assert_eq!(d.role, Role::Z, "the first pick defaults to Z");
        assert!(d.picks.is_empty());
    }

    #[test]
    fn re_picking_a_role_replaces_only_its_pick() {
        let mut state = part_state();
        crate::wcs_pick::arm(&mut state, "Setup1");
        crate::wcs_pick::take(&mut state, face_pick());
        let other = GeoSel { entity: "part".into(), feature: FeatureSel::Face(2) };
        crate::wcs_pick::take(&mut state, other.clone());
        let d = state.wcs_pick.clone().expect("still drafting");
        assert_eq!(d.picks, vec![(other, Role::Z)], "the newest Z wins");
    }

    #[test]
    fn a_paused_draft_takes_nothing() {
        let mut state = part_state();
        crate::wcs_pick::arm(&mut state, "Setup1");
        crate::wcs_pick::pause(&mut state);
        assert!(!collecting(&state));
        crate::wcs_pick::take(&mut state, face_pick());
        assert!(state.wcs_pick.as_ref().expect("kept").picks.is_empty(), "a paused draft holds");
    }
}
