//! The WCS dispatch arms: `CommandKind::SetWcs` (an origin and angles the
//! pane, the anchor grid or a pick chip queued) lands as one undoable
//! `SetWcsOrigin`; `CommandKind::SetWcsCircle` (#739, the "Circle centre"
//! anchor) as one `SetWcsAtCircleCentre` on a picked circular edge. A
//! refusal names itself on the status line and lands nothing.
//! Provenance: our own routing over our own commands; no vendor material.

use camcraft_app::commands::{SetWcsAtCircleCentre, SetWcsOrigin};
use camcraft_app::geometry::{Point3, quat_from_euler};
use camcraft_app::operation::GeoSel;
use camcraft_app::schema::Location;
use camcraft_app::units;

use super::{ErrorScope, State};

/// Roll, pitch, yaw degrees as the world's quaternion; `None` when any
/// angle is not finite (the axis-angle builders would quietly substitute
/// identity - refused instead, Prime Directive).
fn rotation(rpy_deg: [f64; 3]) -> Option<[f64; 4]> {
    let [r, p, y] = rpy_deg;
    (r.is_finite() && p.is_finite() && y.is_finite())
        .then(|| quat_from_euler(r.to_radians(), p.to_radians(), y.to_radians()))
}

impl State {
    fn wcs_placed_status(&mut self) {
        self.doc_version += 1;
        let active = camcraft_app::setup::active(&self.world);
        self.status = format!("{} origin set", camcraft_app::setup::wcs_label(&self.world, active.as_deref()));
    }

    pub(super) fn dispatch_set_wcs(&mut self, pos: [f64; 3], rpy_deg: [f64; 3]) {
        let Some(rot) = rotation(rpy_deg) else {
            self.refuse(ErrorScope::Setup, "WCS not set: rotation angles must be finite".to_owned());
            return;
        };
        let location = Location { pos: Point3::new(pos[0], pos[1], pos[2]), rot };
        match SetWcsOrigin::new(&self.world, location) {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.wcs_pick = None; // #701: a landed frame ends the role draft
                self.wcs_placed_status();
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("WCS not set: {err}")),
        }
    }

    pub(super) fn dispatch_set_wcs_circle(&mut self, pick: &GeoSel, rpy_deg: [f64; 3]) {
        let Some(rot) = rotation(rpy_deg) else {
            self.refuse(ErrorScope::Setup, "WCS not set: rotation angles must be finite".to_owned());
            return;
        };
        match SetWcsAtCircleCentre::new(&self.world, pick, rot) {
            Ok(command) => {
                let c = command.circle().clone();
                self.wcs_ui.circle = Some(Ok(c.clone()));
                self.wcs_ui.pick_circle = false; // done: the viewport picks normally again
                self.history.execute(&self.world, command);
                self.wcs_pick = None; // #701: a landed frame ends the role draft
                self.wcs_placed_status();
                let u = self.units();
                self.status = format!(
                    "{} on the centre of {} {} {}",
                    self.status,
                    if c.closed { "circle Ø" } else { "arc Ø" },
                    units::len(c.diameter(), u, 2),
                    u.suffix()
                );
            }
            Err(err) => {
                let why = match err {
                    camcraft_app::commands::CommandError::Invalid(why) => why,
                    other => other.to_string(),
                };
                self.wcs_ui.circle = Some(Err(why.clone()));
                self.refuse(ErrorScope::Setup, format!("WCS not set: {why}"));
            }
        }
    }
}
