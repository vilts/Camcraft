//! The Fixtures page's placement arms (#2174): the place draft's Apply
//! lands one `MoveFixture` with the Model page's three angles, and each
//! quarter-turn button one `TurnFixture`. A refusal names itself on the
//! status line; a clash or held-body warning follows the done line.
//! Provenance: our own routing over our own commands; no vendor material.

use camcraft_app::commands::{MoveFixture, TurnFixture};
use camcraft_app::commands::fixture_turn::settle_rot;
use camcraft_app::geometry::{Point3, quat_from_euler, quat_to_euler};
use camcraft_app::model;
use camcraft_app::schema::Location;

use super::locate_ui::with_warning;
use super::{ErrorScope, State};

impl State {
    /// Apply the place draft: `deg` is roll, pitch, yaw. A draft whose
    /// angles still name the orientation the fixture has writes that
    /// orientation unchanged - no float dust from the round trip (#2177).
    pub(super) fn dispatch_move_fixture(&mut self, name: &str, pos: [f64; 3], deg: [f64; 3]) {
        if !deg.iter().all(|d| d.is_finite()) {
            self.refuse(ErrorScope::Setup, "fixture not moved: the angles must be numbers of degrees".into());
            return;
        }
        let prev = model::find_by_name(&self.world, name)
            .and_then(|e| model::transform_of(&self.world, &e))
            .unwrap_or_default();
        let next = quat_from_euler(deg[0].to_radians(), deg[1].to_radians(), deg[2].to_radians());
        let location = Location { pos: Point3::new(pos[0], pos[1], pos[2]), rot: settle_rot(prev.rot, next) };
        match MoveFixture::new(&self.world, name, location) {
            Ok(command) => {
                let warning = command.warning().map(str::to_owned);
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = with_warning(format!("moved fixture {name}"), warning);
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("fixture not moved: {err}")),
        }
    }

    /// One quarter-turn button: a `TurnFixture`, then the place draft
    /// (when open on this fixture) re-reads the turned orientation.
    pub(super) fn dispatch_turn_fixture(&mut self, name: &str, axis: usize, degrees: f64) {
        match TurnFixture::new(&self.world, name, axis, degrees) {
            Ok(command) => {
                let warning = command.warning().map(str::to_owned);
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                let about = ["X", "Y", "Z"].get(axis).copied().unwrap_or("?");
                self.status = with_warning(format!("turned fixture {name} {degrees} degrees about {about}"), warning);
                if self.fixture_ui.name == name
                    && let Some(loc) = model::find_by_name(&self.world, name).and_then(|e| model::transform_of(&self.world, &e))
                {
                    let [r, p, y] = euler_deg(loc.rot);
                    self.fixture_ui.roll_deg = r;
                    self.fixture_ui.pitch_deg = p;
                    self.fixture_ui.yaw_deg = y;
                }
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("fixture not turned: {err}")),
        }
    }
}

/// Roll, pitch and yaw in degrees, with float dust under 1e-9 shown as
/// a plain 0 (no "-0.0" in the row or the draft).
pub(crate) fn euler_deg(rot: [f64; 4]) -> [f64; 3] {
    let (r, p, y) = quat_to_euler(rot);
    [r, p, y].map(|a| {
        let d = a.to_degrees();
        if d.abs() < 1e-9 { 0.0 } else { d }
    })
}
