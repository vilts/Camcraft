//! The 2D Pocket's rest machining group in the project file (#1489): the
//! previous tool as the user entered it, as plain keys. Absent is off -
//! every earlier file. Provenance: our own file format (`project/mod.rs`).

use super::*;

use crate::machining2d::RestParams;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct RestDto {
    /// The diameter of the tool that roughed the pocket (mm).
    pub previous_tool_diameter: f64,
    /// Its corner radius (mm); absent is a flat end mill.
    #[serde(default)]
    pub previous_corner_radius: f64,
    /// The radial stock it left on the walls (mm).
    #[serde(default)]
    pub previous_stock_to_leave: f64,
    /// How much the rest area is grown (mm).
    #[serde(default)]
    pub allowance: f64,
}

impl From<RestParams> for RestDto {
    fn from(r: RestParams) -> Self {
        RestDto {
            previous_tool_diameter: r.previous_tool_diameter,
            previous_corner_radius: r.previous_corner_radius,
            previous_stock_to_leave: r.previous_stock_to_leave,
            allowance: r.allowance,
        }
    }
}

impl From<RestDto> for RestParams {
    fn from(r: RestDto) -> Self {
        RestParams {
            previous_tool_diameter: r.previous_tool_diameter,
            previous_corner_radius: r.previous_corner_radius,
            previous_stock_to_leave: r.previous_stock_to_leave,
            allowance: r.allowance,
        }
    }
}
