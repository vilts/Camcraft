//! 5-axis contour (M5.1 stage C): the waterline's Z-level ring ladder
//! over the freeform, every cut move holding the pose [`tilt`]
//! commands. The path is the ball-center tangent locus exactly as
//! [`crate::finishing3d::waterline`] builds it - a sphere cares which
//! way its shank points not at all - and the 5-axis content is the
//! orientation: on steep flanks a vertical spindle drags its shank
//! into the wall, and the lead/lag lean (plus the max-tilt clamp) is
//! the difference between a finishing pass and a crash. The levels
//! ride the scallop stepdown law `Δz = 2√(r·tol)`; the tolerance, not
//! a stepdown field, remains the author of depth.
//!
//! Provenance: 5-axis contouring over a Z-level pattern and the
//! lead/side-tilt axis law are published multi-axis milling practice
//! (ISO 10791-10 test-part tradition, standard CAM manuals); the
//! construction here reuses this codebase's own forbidden-center
//! rings and adds the pose stamp. No vendor source was consulted.

use crate::cl::Toolpath;
use crate::finishing3d::{self, Finish3DParams};
use crate::geometry::Triangle;
use crate::machining2d::GenError;

use super::avoid::Shank;
use super::stamp::stamp_axes;
use super::tilt::AxisControl;

/// The ring ladder with the tool leaning where the law says - and,
/// with a `shank` (M5.1 stage F), dodging the part inside the cone
/// where the law's pose would touch. The positions are the
/// waterline's either way: law (f) makes the dodge exact.
pub fn contour5d(
    facets: &[Triangle],
    p: &Finish3DParams,
    c: &AxisControl,
    shank: Option<&Shank>,
) -> Result<Toolpath, GenError> {
    contour5d_seamed(facets, p, c, shank, &finishing3d::RingSeam::default())
}

/// [`contour5d`] with the user's start and exit points (#3330): the
/// waterline's rings seated at the start pick and run on to the exit
/// pick (`finishing3d::waterline_seamed`), then posed by the same
/// stamp - the tilt limit and the shank dodge are judged at the pick
/// as at every other ring point. An empty `seam` is [`contour5d`].
pub fn contour5d_seamed(
    facets: &[Triangle],
    p: &Finish3DParams,
    c: &AxisControl,
    shank: Option<&Shank>,
    seam: &finishing3d::RingSeam,
) -> Result<Toolpath, GenError> {
    let base = finishing3d::waterline_seamed(facets, p, seam)?;
    stamp_axes(&base, facets, p.tool_diameter * 0.5, c, shank)
}
