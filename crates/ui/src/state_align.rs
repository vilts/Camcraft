//! The Setups page's part-alignment verbs, dispatched (#326, #330, #555,
//! #557, #606; split out of `state.rs` for #738): every one lands as one
//! undoable `AlignPartToSetup`. The preset / "Turn by" row
//! (`CommandKind::TurnPart`), the face, three-point and axis picks all
//! move the part from where it sits now (#738); `AlignPart` stays the
//! absolute turn ("As modelled" resets through it). A refusal names
//! itself on the status line and writes nothing.
//! Provenance: our own routing over our own commands; no vendor material.

use camcraft_app::commands::AlignPartToSetup;
use camcraft_app::setup_align;

use super::{CommandKind, ErrorScope, State};

/// Which pick draft an applied verb ends.
enum Draft {
    None,
    Plane,
    Points,
    Axes,
    Cylinder,
}

impl State {
    /// One alignment verb (the arms `drain_actions` hands over).
    pub(super) fn dispatch_align(&mut self, kind: CommandKind) {
        let w = &self.world;
        let (setup, built, done, refused, draft) = match kind {
            CommandKind::AlignPart { setup, angles, pivot } => {
                let c = AlignPartToSetup::turn(w, &setup, angles, pivot);
                (setup, c, "part alignment".to_owned(), "part alignment not changed", Draft::None)
            }
            CommandKind::TurnPart { setup, angles, pivot } => {
                let c = AlignPartToSetup::turn_by(w, &setup, angles, pivot);
                (setup, c, "part alignment".to_owned(), "part alignment not changed", Draft::None)
            }
            CommandKind::LayPartOnPlane { setup, picks, lay } => {
                let c = AlignPartToSetup::lay_on_plane(w, &setup, &picks, lay);
                let done = format!("part laid on {}", camcraft_app::pick::plane_label(lay.plane));
                (setup, c, done, "part not laid on the plane", Draft::Plane)
            }
            CommandKind::LayPartOnPoints { setup, picks, lay } => {
                let c = AlignPartToSetup::lay_on_points(w, &setup, &picks, lay);
                let done = format!("part laid on {} through three points", camcraft_app::pick::plane_label(lay.lay.plane));
                (setup, c, done, "part not laid on the three points", Draft::Points)
            }
            CommandKind::AlignPartFromAxes { setup, axes } => {
                let c = AlignPartToSetup::align_from_axes(w, &setup, &axes);
                (setup, c, "part aligned from its picks".to_owned(), "part not aligned from its picks", Draft::Axes)
            }
            CommandKind::AlignPartCylinder { setup, pick, target, angle_deg, flip } => {
                let c = AlignPartToSetup::align_from_cylinder(w, &setup, &pick, target, angle_deg, flip);
                (setup, c, "part aligned from its cylinder".to_owned(), "part not aligned from its cylinder", Draft::Cylinder)
            }
            _ => return, // only the alignment verbs are routed here
        };
        let command = match built {
            Ok(command) => command,
            Err(err) => {
                self.refuse(ErrorScope::Setup, format!("{refused}: {err}"));
                return;
            }
        };
        self.history.execute(&self.world, command);
        self.doc_version += 1;
        match draft {
            Draft::Plane => self.plane_pick = None,
            Draft::Points => self.points_pick = None,
            // The picks stay (disarmed) so Flip re-applies onto what the
            // setup now holds (#557 hand check).
            Draft::Axes => {
                if let Some(d) = self.axis_pick.as_mut().filter(|d| d.setup == setup) {
                    d.picking = None;
                    d.applied = true;
                }
            }
            // The picks stay (disarmed) so the angle re-applies onto what
            // the setup now holds.
            Draft::Cylinder => {
                if let Some(d) = self.cylinder_pick.as_mut().filter(|d| d.setup == setup) {
                    d.picking = false;
                }
            }
            Draft::None => {}
        }
        let words = crate::egui_app::align_text(setup_align::stored(&self.world, &setup));
        self.status = format!("{setup} {done}: {words}");
    }
}
