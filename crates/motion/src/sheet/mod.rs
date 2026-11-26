//! The shared 2D sheet-cutting kernel (#1883): contour nesting and the
//! inner-first cut order, the kerf ring of one contour and the walks
//! along it, the kerf and cover laws, the ring by arc length, and the
//! one-sided offset of an open run. Plasma, wire EDM and waterjet cut
//! with it; each names itself in the refusals by its [`Voice`].
//!
//! Provenance: moved out of `plasma` (#670, #672) and `wire_edm`
//! (#539, #1109) without change of behaviour; each file's head cites
//! its public sources (Hypertherm, the LinuxCNC Plasma Primer, Tormach's
//! 1300PL manual, general wire EDM practice). Our own code.

pub mod arclen;
pub mod laws;
pub mod lead;
pub mod nest;
pub mod open;
pub mod ring;

use crate::cl::ContourRole;
use crate::machining2d::GenError;

pub use nest::Contour;

/// Who speaks in a refusal: the operation's name (`plasma profile`)
/// and the cutting tool's (`torch`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Voice {
    pub who: &'static str,
    pub tool: &'static str,
}

impl Voice {
    pub const PLASMA: Voice = Voice { who: "plasma profile", tool: "torch" };
    pub const WIRE: Voice = Voice { who: "wire contour", tool: "wire" };
    pub const WATERJET: Voice = Voice { who: "waterjet profile", tool: "jet" };

    /// A refusal by name, prefixed with the operation's name.
    #[must_use]
    pub fn invalid(self, why: String) -> GenError {
        GenError::Invalid(format!("{}: {why}", self.who))
    }
}

/// One lit stretch of a cut, for the laws: the contour it cuts (its
/// cut-order id), that contour's role, and the first and last move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lit {
    pub contour: u32,
    pub role: ContourRole,
    pub on: usize,
    pub off: usize,
}
