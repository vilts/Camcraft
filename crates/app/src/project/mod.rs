//! Project save/load: `.camproj`, JSON, version 61 (see `PROJECT_VERSION`).
//!
//! Serialization goes through plain DTO structs (deliberately not flecs
//! reflection): the file format is our public persistence contract and must
//! stay stable regardless of ECS storage decisions. Entity order is by name,
//! so a save -> load -> save cycle is byte-identical.
//!
//! v2 added the rotation quaternion to every entity (rigid transforms, M1.2)
//! and the `kind`/`stock` fields for setup furniture; both are defaulted, so
//! a v2 file written before stock existed still parses. Parametric stock is
//! persisted as PARAMETERS only - `stock::stock_mesh` regenerates identical
//! facets in any build (see `stock.rs`), keeping files small and round-trips
//! exact. v3 adds `kind` values `tool`/`operation` with a `tool`/`operation`
//! payload; CAM data is persisted as definition plus the cached path (paths
//! are expensive to recompute, and staleness recomputes anyway). v4 makes
//! feature picks a contractual source: the `selection` variant of `src`
//! already existed in v3 files (it arrived with the domain type), so every
//! v3 file reads unchanged - the bump records that picks are now covered
//! by the save/load contract rather than an experiment in transit. v5 is
//! the M1.5.2 landing shape: welded-face picks (`patch`), arc moves
//! (`arc`), facing stepdowns (`top_z`/`stepdown` on face) and pocket
//! retract hops (`retract_z` on pocket) all appear in the file grammar at
//! once, each with a serde default that reads as exact legacy behavior -
//! a v4 file changes nothing in meaning, and a v5 build that has not
//! wired one of these yet round-trips files that carry it without
//! breaking it. Later steps wire the fields to the generators; the
//! format does not move again. v6 is the heights landing shape: every
//! operation height field (`clearance_z`, `top_z`, `bottom_z`,
//! `retract_z`, `cut_z`) is now a From+Offset expression
//! (`{"reference": ..., "offset": ...}`, see `HeightDto`), and a bare
//! number - the whole v5 grammar for a height - still reads, as
//! `{"reference": "fixed", "offset": n}` with the saved z preserved to
//! the bit. Work-offset references name a `G54..G59` slot; the slots'
//! placements were already persisted in every entity's
//! `translation`/`rotation`, so nothing else about the file moved. v7
//! adds the contour side choice (`side` on contour params, see
//! `SideDto`): an absent key reads as `auto` - today's Outside law until
//! the selection walk resolves it - so a v6 file changes nothing in
//! meaning. v8 adds document units (a top-level `units` key, see
//! `crate::units`): an absent key reads as `mm`, which is exactly what
//! every v7 file displayed. v9 widens the contour `lead` vocabulary with
//! `tangential` (see `LeadDto`); `lead` has been a required key since
//! v1, so no older file contains the new tag and none needs a content
//! change. v10 is the drilling landing shape: the `drill` operation
//! params (with its hole list, see `HoleDto`), the `tip_angle`/`pitch`
//! tool fields, and the cached cycle list (`PathDto.cycles`) all arrive
//! at once, each with a serde default that reads as exact legacy
//! behavior - no pre-v10 file could contain a drill-kind tool or a
//! drill operation (the tags did not exist), and no pre-v10 path ever
//! had cycles, so absent means precisely what it behaved as. v11 is the
//! 3-D pocket landing shape: the `pocket3d` operation params arrive with
//! a serde default on every new key (`retract_z` absent is
//! hop-to-clearance, `entry` absent is Plunge), and no pre-v11 file could
//! contain a 3-D pocket operation at all - the tag did not exist - so the
//! step-up is the stamp itself. v12-v18 added the later operation-family
//! grammars (corner/flow profiles, surface finish, ramp), each the same
//! pure step-up, one default per new key; v19 added the top-level `machine`
//! key (absent means no machine, the mute checker). v20 is M3.1: the model
//! `rev` key (the geometry-revision counter, see `schema::Revision`) - no
//! pre-v20 file could carry it, and absent means 0, which is exactly what
//! every pre-v20 document was. The signatures that make staleness work
//! recompute from the persisted facets; nothing else moved. v21 is
//! M3.2 stage A: setups arrive as a `setup` kind with its `SetupDto`
//! payload, an optional `setup` name inside `OperationDto`, and a
//! top-level `active_setup` key. No pre-v21 file can contain any of them:
//! the kind tag did not exist, and every default (no setup rows, no
//! setup names, no explicit active choice) is exactly the legacy global
//! behavior the implicit-setup law already guarantees. Step-up is the
//! stamp itself, like every bump since v6. v22 is M3.2 stage B: a
//! fixture is a model entity wearing a `fixture` marker (`FixtureDto` -
//! one key, the setup binding; the solid itself is the model's own
//! persisted geometry, and the machine's table is never stored, it is
//! derived from the `machine` key). No pre-v22 file can contain the
//! marker - the component did not exist - and the default (absent means
//! no fixture) is exactly the pre-B world: collision checked against
//! stock and model alone. Step-up is the stamp itself, like every bump
//! since v6. v23 is M3.3 stages A-B: the top-level `post` key (a user
//! `.cps` path, or a shipped registry name once stage B grew the
//! registry; absent means the default post - exactly what every
//! pre-v23 file ran, since the registry then held one script). No
//! pre-v23 file can contain the key and its default is the shipping
//! behavior verbatim. Step-up is the stamp itself, like every bump
//! since v6. v1-v22 files migrate on load, one version step at a time.

use std::path::Path;

use flecs_ecs::core::EntityViewGet; // not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::World;
use serde::{Deserialize, Deserializer, Serialize};

use crate::cl::{ArcPlane, Cycle, CycleKind, Move, MoveKind, Toolpath, lattice};
use crate::drilling::{CrossHole, CycleChoice, Hole};
use crate::geo2d::Point2;
use crate::geometry::{IDENTITY_Q, Point3, Triangle};
use crate::machining2d::{Lead, PocketStrategy, Side};
use crate::machining3d::Entry3D;
use crate::model;
use crate::operation::{FeatureSel, GenStamp, GeoSel, OpParams, SrcRef};
use crate::schema::{
    FixtureDef, Location, Operation, OperationMeta, STOCK_NAME, SetupDef, StockDef, ToolDef,
    Toolpath as ToolpathCache, LEGACY_WCS_NAMES, Wcs,
};
use crate::stock::{self, StockShape};
use crate::tools::{Tool, ToolKind};
use crate::units::{self, Unit};
use crate::verify::Warning;

mod area;
pub use area::*;
mod file;
pub use file::*;
mod params;
pub use params::*;
mod entity;
mod vise_dto;
mod fixture_keys;
mod plate_dto;
mod modvise_dto;
pub use vise_dto::ViseDto;
mod thread_dto;
mod break_dto;
pub use break_dto::EdgeBreakDto;
// #416: the threading infeed plan's file mirror.
mod thread_infeed_dto;
pub use thread_infeed_dto::{DepthScheduleDto, InfeedMethodDto, ThreadInfeedDto};
pub use thread_dto::{PassScheduleDto, ThreadCycleDto, ThreadHandDto, ThreadPassesDto};
mod touch_dto;
pub use touch_dto::ProbeTouchDto;
mod params_impl;
// #108: the Probe operation's file mirror.
mod probe_dto;
pub use probe_dto::*;
// #670: the plasma profile's file mirror and its path record.
mod plasma_dto;
// #890: a tool's cutting data per workpiece material.
mod cutting_rows_dto;
pub use cutting_rows_dto::{CuttingRowDto, MaterialWord};
pub use plasma_dto::*;
mod wire_edm_dto;
pub use wire_edm_dto::*;
// #541: the sinker burn's file mirror.
mod sinker_dto;
pub use sinker_dto::*;
mod trochoid_dto;
pub use trochoid_dto::*;
mod approach;
pub use approach::*;
mod links;
pub use links::*;
pub use entity::*;
mod wire;
pub use wire::*;
mod json;
pub use json::*;
mod apply;
mod fidelity_migrate;
use apply::*;
// #813: the load note for a machine embedded before #804.
mod swing_note;
// #902: the v59 -> v60 rung - the document's unit is the program's.
mod units_migrate;
mod validate;
use validate::*;
mod wcs_migrate;
// #342: the v53 -> v54 one-WCS-per-setup rung.
mod wcs_one;
mod wcs_pick;
pub(crate) use wcs_pick::{work_offset_pick, work_offset_text};
// #343: a setup's own machine and post (v55).
mod mount_dto;
pub use mount_dto::{SetupMachineDto, SetupPostDto};
// #389: a setup's own post options (additive).
mod setup_options_dto;
pub use setup_options_dto::SetupPostOptionsDto;
// #316: setup links (v56), checked before any world write.
mod setup_links;
// #368: a tool's modelled body on file (v57).
mod tool_body_wire;
pub use tool_body_wire::ToolBodyDto;
// #155: Pause / Home entries of the operation list.
mod program_entry_dto;
pub use program_entry_dto::ProgramEntryDto;
