//! The 2D Pocket's rest machining group in the project file (#1489): the
//! previous tool as the user entered it, as plain keys. Absent is off -
//! every earlier file. #2033: `source` says which reading plans the rest
//! area; absent is the entered tool - every file before it.
//! Provenance: our own file format (`project/mod.rs`).

use super::*;

use crate::machining2d::{RestParams, RestSource};

/// Where the rest area is read from (#2033), as the file and MCP say it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RestSourceDto {
    /// The entered previous tool.
    #[default]
    EnteredTool,
    /// The stock that the earlier operations of the setup leave.
    PreviousOperations,
}

impl RestSourceDto {
    fn is_entered(&self) -> bool {
        *self == RestSourceDto::EnteredTool
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct RestDto {
    /// Which reading plans the rest area; absent is the entered tool.
    #[serde(default, skip_serializing_if = "RestSourceDto::is_entered")]
    pub source: RestSourceDto,
    /// The diameter of the tool that roughed the pocket (mm). Not read
    /// (and it can be absent) in `previous_operations` mode.
    #[serde(default)]
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
            source: match r.source {
                RestSource::EnteredTool => RestSourceDto::EnteredTool,
                RestSource::PreviousOperations => RestSourceDto::PreviousOperations,
            },
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
            source: match r.source {
                RestSourceDto::EnteredTool => RestSource::EnteredTool,
                RestSourceDto::PreviousOperations => RestSource::PreviousOperations,
            },
            previous_tool_diameter: r.previous_tool_diameter,
            previous_corner_radius: r.previous_corner_radius,
            previous_stock_to_leave: r.previous_stock_to_leave,
            allowance: r.allowance,
        }
    }
}
