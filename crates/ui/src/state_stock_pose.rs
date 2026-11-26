//! The Stock page's pose (#2576): the draft's turn from its axis choice,
//! the one `SetStock` that Apply lands through, and the plain-English note
//! the page shows before a simulation when the simulation cannot cut the
//! drafted stock. The note asks the simulation's own law
//! (`stock_follow::sim_frame`), so the page and the refusal never differ.
//! Provenance: our own page state over our own stock command.

use camcraft_app::commands::SetStock;
use camcraft_app::geometry::Point3;
use camcraft_app::geometry::stock_axis::stock_axis_rot;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_follow::sim_frame;

use super::{ErrorScope, State, StockUi};

/// The note for a stock the simulation refuses as tilted.
pub const SIM_NOTE_TILTED: &str = "The simulation cannot cut a stock that is tilted off the X, Y and Z axes yet. \
     The viewport shows it, but the simulation stops with a message.";

impl StockUi {
    /// The draft's shape, as Apply sends it.
    #[must_use]
    pub fn draft_shape(&self) -> StockShape {
        match self.shape {
            1 => StockShape::Cylinder { diameter: self.diameter, height: self.height },
            2 => StockShape::FromModel { source: self.source.clone() },
            _ => StockShape::Box { dx: self.dx, dy: self.dy, dz: self.dz },
        }
    }

    /// The draft's turn; `None` when a Custom angle is not a number.
    #[must_use]
    pub fn draft_rot(&self) -> Option<[f64; 4]> {
        stock_axis_rot(self.axis, self.angles_deg)
    }

    /// The note the page shows when the simulation cannot cut the
    /// drafted stock as it is turned; `None` when it can. Since #2758 the
    /// simulation cuts a box or a cylinder laid along X or Y, so only a
    /// stock tilted off the axes (`OBLIQUE_STOCK`) gets the note.
    #[must_use]
    pub fn sim_note(&self) -> Option<&'static str> {
        let rot = self.draft_rot()?;
        let loc = Location { pos: Point3::new(self.x, self.y, self.z), rot };
        sim_frame(&self.draft_shape(), loc).err().map(|_| SIM_NOTE_TILTED)
    }
}

impl State {
    /// One undoable `SetStock` for the active setup's stock at `pos`
    /// turned `rot`; a refusal is shown on the Setup scope.
    pub(super) fn dispatch_set_stock(&mut self, shape: StockShape, pos: [f64; 3], rot: [f64; 4]) {
        let location = Location { pos: Point3::new(pos[0], pos[1], pos[2]), rot };
        match SetStock::new(&self.world, shape, location) {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = String::from("stock set");
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("stock not set: {err}")),
        }
    }
}
