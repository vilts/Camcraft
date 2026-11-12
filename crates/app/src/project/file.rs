use super::*;

/// v43: the ParamsDto grows the two 2D roughing kinds (M2.4 A). The
/// arms are NEW VARIANTS, not additive fields — a v42 build reading a
/// v43 file fails on the unknown variant by name, which is the honest
/// refusal; the bump makes it a version refusal before a serde one.
/// The kinds join the 2D mill's run in place (the ribbon's contiguity
/// law), so every registry index from the old Drill row up shifted by
/// two — the only numbers a v42 file carries by index are the ribbon
/// pins, and the ladder remaps them (the PostDto law seen from the
/// other side: names survive growth, indices need a rung).
/// v44: the same shape twice more (M2.4 B) — the ParamsDto grows the
/// Engrave/Chamfer2D/Circular arms, the ToolDto kind grammar the
/// `taper` word, and the three rows join the 2D mill's run after
/// Trace, so the ladder shifts every pin index from the old Drill row
/// up by three.
/// v45: the shape once more WITH the shift (M2.4 C) — the ParamsDto
/// grows the Bore/Thread arms and the two rows seat IN their sections'
/// runs (Thread closes Mill2D at 10, Bore joins Drilling at 12): the
/// partition law wants contiguous runs and the family law wants the
/// turning rows to stay the tail, so the rung steps the old Drill
/// index by +1 and everything from the old Pocket3D row up by +2.
/// v46: the pure step-up again (M2.5 A) — the Finish3D arm grows one
/// optional key (`steep_stepover`) and the pattern grammar grows two
/// words (steep_and_shallow, flat); no row moves, so no pin shifts —
/// an absent key decodes to the scallop law, exactly what every v45
/// file ran, and the new words cannot exist in one (the arrival law).
/// v47: the pure step-up again (M2.5 B) — the Finish3D arm grows one
/// optional key (`pencil_reference`) and the pattern grammar grows two
/// words (pencil, corner); no row moves, so no pin shifts — an absent
/// key decodes to "no reference ball named", exactly what every v46
/// file ran, and the new words cannot exist in one (the arrival law).
/// v48: the shape with the shift, one row (M2.5 E B) — the ParamsDto
/// grows the Deburr arm and the row seats IN the Mill3D run at 15
/// (the partition law wants contiguous sections, so the 5-Axis Contour
/// and everything after it slide +1): a v47 file cannot contain the
/// new variant (arrival law) and its pins name a table that moved.
/// v49: the pure step-up (M11.4 C) - the strategy grammar grows
/// `offset_rings`; no row moves, no pin shifts, absent words read as
/// before (the arrival law).
/// v50: operation labels and within-setup order become persisted. An old
/// reader must refuse the file rather than silently post in name order.
/// v51 (#196): a setup may carry its picked program offset
/// (`work_offset`, a G-word). Absent reads as the inferred law every v50
/// file posted with; an old reader must refuse the file rather than
/// silently post a picked G55 setup at G54.
/// v52 (#329): a work-offset height names no slot of its own - it reads
/// its setup's. Older files' per-height slots move onto their setup when
/// the setup agreed and refuse by name when not (`wcs_migrate`); an old
/// reader must refuse a v52 file rather than read `{"work_offset": {}}`.
/// v53 (#326): a setup may hold the part turned (`part_alignment`, a
/// rigid transform document -> setup frame). Absent reads as "as
/// modelled", so every v52 file loads unchanged; an old reader must
/// refuse a v53 file rather than machine a flipped setup unflipped.
/// v54 (#342): one WCS per setup - a setup names one WCS entity
/// (`"wcs": "<name>"`), and `work_offset` is only the word the post
/// writes for it. Each older setup keeps the origin of the slot it
/// posted; slots no setup posted are dropped with a load note
/// (`wcs_one`). An old reader must refuse a v54 file rather than read a
/// string where it expects a slot list.
/// v55 (#343): a setup may own its machine and post (`SetupDto::machine`
/// / `SetupDto::post`); absent follows the document's, so a v54 file
/// steps up unchanged. An old reader must refuse a v55 file rather than
/// post every setup on the document's machine.
/// v56 (#316): a setup may be placed relative to another
/// (`SetupDto::relative_to`), so a change of that setup's part alignment
/// carries into it. Absent links nothing, so a v55 file steps up
/// unchanged; a link naming no setup, or a loop of links, is corrupt. An
/// old reader must refuse a v56 file rather than let an edit of one
/// setup leave its re-clamp behind.
/// v57 (#320): a setup may pick where its stock comes from
/// (`SetupDto::stock_source`: its own, or what the setup before leaves).
/// Absent reads as #317's implicit rule, so a v56 file steps up and
/// simulates unchanged; an unknown word is corrupt. An old reader must
/// refuse a v57 file rather than sim a setup from the wrong stock.
/// v58 (#368): a tool may carry a modelled body (`body` on the tool,
/// `tool_body_wire`). Absent is the holder cylinder every v57 file
/// judged; an old reader must refuse a v58 file rather than drop the
/// body and judge the smaller cylinder in its place.
pub const PROJECT_VERSION: u32 = 58;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported project version {0} (this build writes {PROJECT_VERSION})")]
    UnsupportedVersion(u32),
    #[error("malformed project file: {0}")]
    Corrupt(String),
    /// #329: an older file whose operations measured from different work
    /// offsets inside one setup - opening it would re-point a zero.
    #[error(
        "work offsets disagree - since project v52 a setup's work offset is program zero for \
         every operation in it, and opening this file would silently re-point an operation. \
         Make each setup's operations measure from one offset (in the release that wrote the \
         file), then reopen:\n{0}"
    )]
    WorkOffsetConflict(String),
}

#[derive(Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: u32,
    /// The document's display unit ("mm" | "in"), v8+. Absent means
    /// millimeters - exactly what every pre-v8 file displayed, so the
    /// serde default is itself the whole migration. A millimeter document
    /// omits the field (byte-shape of the pre-v8 files, plus the stamp).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    /// The document's machine (M2.3 C), v19+. An absent key means no
    /// machine was ever loaded - exactly what every v18 file meant,
    /// since the checker did not exist to read one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<MachineDto>,
    /// The document's explicit active-setup choice (M3.2 A), v21+. An
    /// absent key means no choice was ever made, which resolves to the
    /// first setup by name - what every v20 document did. A name that
    /// matches no setup row is legal, not corrupt: it reads through the
    /// same existence fallback the live singleton does (the fallback that
    /// also protects an undo of `DeleteSetup` mid-history).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_setup: Option<String>,
    /// The document's post script (M3.3 A/B), v23+. An absent key
    /// means the default post - what every v22 file ran, since the
    /// registry then held one script. `{"file": path}` names a user
    /// `.cps`; `{"named": name}` picks a shipped registry entry BY
    /// NAME (stage B grew the registry to five, and an index into it
    /// would make a re-order silently change a saved document's post -
    /// names survive growth, indices don't). Chosen values are stored
    /// as chosen: opening does not re-validate (the `SelectPost`
    /// constructor did), and a script that went missing - file or
    /// registry name - refuses by name at post time, never silently
    /// falling back (`script_source`'s law).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<PostDto>,
    /// The post options (issue #34): absent is every default - what
    /// every earlier document means - so the default writes no key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_options: Option<PostOptionsDto>,
    // From v41 a `theme` key lived here (M6.1 C). Since #877 the theme is a
    // user preference (`app::prefs`): the key is no longer written, and an
    // older file that carries it reads cleanly - serde ignores unknown keys.
    /// The ribbon's pinned kinds (M6.2), v42+: registry indices in pin
    /// order. An absent key pins nothing - what every v41 document is,
    /// since the ribbon could not pin then. The indices ride the
    /// table: an append never disturbs them (the row order IS the
    /// product order, the combo law), and a mid-run insert shifts them
    /// in the version ladder - v43's rung maps idx >= 5 to idx + 2 and
    /// v44's maps idx >= 7 to idx + 3, each for its kinds' place in
    /// the 2D mill's run - so a pinned row keeps
    /// naming the kind it named when it was saved. Out-of-range ones
    /// drop at read (`pinned_of`), so a document pinned on a larger
    /// table still opens, showing only the kinds this build can name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pinned: Vec<usize>,
    /// The document's workpiece material (M13.2), additive (no version
    /// step): the cutting-data table's word. An absent key states no
    /// material - what every older document was - and states nothing
    /// a check could read; an unknown word refuses the file by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// "Skip pauses and homes" at post (#110), additive (no version
    /// step): an absent key is off - what every older document posted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_stops: bool,
    /// "One file per operation" at post (#111), additive like
    /// `skip_stops`: an absent key is one combined program.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub per_op_files: bool,
    /// Per-operation smoothing at post (#616), additive like
    /// `skip_stops`: an absent key is off - no smoothing words.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub smoothing: bool,
    pub entities: Vec<EntityDto>,
}

/// The file mirror of the `PostOptions` singleton (issue #34): each
/// choice spelled by its word, absent when it is the default.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PostOptionsDto {
    /// `"g76"` for the cycle; absent is the G32 passes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threading: Option<String>,
    /// `"inch"` / `"mm"`: the unit the program ends in (#596), additive
    /// (no version step); absent leaves the program's unit modal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_units: Option<String>,
}

/// The file mirror of the `PostRef` singleton (M3.3): exactly the two
/// choices a document can carry beyond the default, each spelled for
/// what it is - a registry name or a path, never one string standing
/// ambiguously for both.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PostDto {
    Named(String),
    File(String),
}

/// The file mirror of the `MachineDef` singleton: the parsed machine
/// and the `.mch` path it came from, kept for display so opening a
/// project restates where the setup's numbers live. `Machine` is
/// serde-transparent (our own struct, our own field semantics).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MachineDto {
    pub path: String,
    pub machine: crate::machine::Machine,
}

/// What an entity is, in file terms. Absent means `Model` (the only kind
/// v1 and early v2 files could hold).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    Model,
    Stock,
    Wcs,
    Tool,
    Operation,
    Setup,
    /// A Pause / Home entry of the operation list (#155).
    Entry,
}

/// The file mirror of `StockShape`, kept separate so the domain enum can
/// evolve without editing the persistence contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StockDto {
    Box { dx: f64, dy: f64, dz: f64 },
    Cylinder { diameter: f64, height: f64 },
    FromModel { source: String },
}

impl StockDto {
    pub(super) fn from_shape(shape: &StockShape) -> Self {
        match shape {
            StockShape::Box { dx, dy, dz } => StockDto::Box {
                dx: *dx,
                dy: *dy,
                dz: *dz,
            },
            StockShape::Cylinder { diameter, height } => StockDto::Cylinder {
                diameter: *diameter,
                height: *height,
            },
            StockShape::FromModel { source } => StockDto::FromModel {
                source: source.clone(),
            },
        }
    }

    pub(super) fn to_shape(&self) -> StockShape {
        match self {
            StockDto::Box { dx, dy, dz } => StockShape::Box {
                dx: *dx,
                dy: *dy,
                dz: *dz,
            },
            StockDto::Cylinder { diameter, height } => StockShape::Cylinder {
                diameter: *diameter,
                height: *height,
            },
            StockDto::FromModel { source } => StockShape::FromModel {
                source: source.clone(),
            },
        }
    }
}

/// The file mirror of `tools::Tool`; same reason as `StockDto`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolDto {
    pub kind: ToolKindDto,
    pub diameter: f64,
    pub corner_r: f64,
    /// M1.6 additions: absent from a v9 file, serde-defaults to the
    /// inert 0.0 both fields were behaviorally equal to before.
    #[serde(default)]
    pub tip_angle: f64,
    #[serde(default)]
    pub pitch: f64,
    /// M5.2 addition (v33): absent from any older file, serde-defaults
    /// to the inert 0.0 every mill kind already meant - the `tip_angle`
    /// / `pitch` law restated.
    #[serde(default)]
    pub nose_radius: f64,
    /// M5.2 addition (v35): absent from any older file, serde-defaults
    /// to the inert 0.0 every non-blade kind already meant - the same
    /// law a third time.
    #[serde(default)]
    pub width: f64,
    pub flute_len: f64,
    pub shank_d: f64,
    pub shank_len: f64,
    pub holder_d: f64,
    pub holder_len: f64,
    pub material_group: String,
    pub rpm: f64,
    pub feed: f64,
    /// M13.2 additions, additive (no version step): absent from any
    /// older file, serde-defaults to "not stated" - exactly what every
    /// tool of an older file was, since nothing then read either.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_material: Option<ToolMaterialDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flutes: Option<u32>,
    /// M11.3 addition: absent from every older file (and written only
    /// when false), serde-defaults to the centre-cutting tool every
    /// earlier tool was taken to be - no version step.
    #[serde(default = "centre_cutting", skip_serializing_if = "is_centre_cutting")]
    pub center_cutting: bool,
    /// #221 addition, additive (no version step): the controller's tool
    /// number. Absent from every older file and written only when set;
    /// absent means "post by list position", what every older tool did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    /// #368 (v58): the tool's modelled body. Absent is none - the
    /// holder cylinder, what every older tool judged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<super::tool_body_wire::ToolBodyDto>,
    /// #93 slice 2, additive (no version step): the tool's drawn
    /// cutter outline, `[z, r, arc]` per point tip-up. Absent is the
    /// outline its numbers draw - every older file. A reader that
    /// predates it skips the key and shows that outline: display only,
    /// nothing a generator or law reads changes. A present outline
    /// that disagrees with the tool's numbers refuses the file by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cutter: Option<Vec<[f64; 3]>>,
    /// #630 addition, additive (no version step): the lathe tool
    /// orientation (LinuxCNC 1-9). Absent from every older file and
    /// written only when stated; absent reads as the kind's default
    /// (`orientation::default_for`) - 2 for a turning insert, 3 for a
    /// boring bar. An out-of-range code is refused at read by the
    /// tool sanity law (`validate`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<u8>,
    /// #891 addition, additive (no version step): the turning cutting
    /// data (surface speed m/min, feed mm/rev, max-rpm clamp). Absent
    /// from every older file and written only when stated; absent
    /// reads as unstated - the turning ops post the tool's rpm (G97).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turning: Option<camcraft_tools::turning_data::TurningData>,
}

fn centre_cutting() -> bool {
    true
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_centre_cutting(v: &bool) -> bool {
    *v
}

/// The file mirror of `cutting::ToolMaterial` (M13.2): a word, so an
/// unknown one is a named refusal at read.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolMaterialDto {
    Hss,
    Carbide,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolKindDto {
    EndMill,
    BallNose,
    BullNose,
    Drill,
    Tap,
    Bore,
    /// The turning insert (M5.2 A): a new word in the kind grammar -
    /// v32 files never carried it, so no migration is needed, exactly
    /// as `bore` needed none at its arrival.
    Turning,
    /// The grooving/parting blade (M5.2 C): the same arrival law -
    /// v34 files never carried the word, and the `width` key it
    /// needs serde-defaults to the inert 0.0 every other kind means.
    Groove,
    /// The threading insert (M5.2 D): the same arrival law again -
    /// v35 files never carried the word, and the `width` +
    /// `tip_angle` keys it reads both serde-default to the inert
    /// 0.0 every other kind means.
    Thread,
    /// The form gauge (M5.2 E2): the same arrival law - v37 files
    /// never carried the word, and it reads the `width` +
    /// `nose_radius` keys both already filed, the crown taking the
    /// nose circle's place in the row for this kind alone.
    Form,
    /// The chamfer mill's cone (M2.4 B): the same arrival law - v43
    /// files never carried the word, and the `tip_angle` key it reads
    /// (the cone's INCLUDED angle, the Drill's law restated) has been
    /// filed since v10.
    Taper,
    /// The touch probe (#108): an additive word in the v50 grammar - no
    /// older file names it, so no rung is needed to read one.
    Probe,
    /// The plasma torch (#670): additive in the same way - no older
    /// file names it.
    Torch,
}

impl ToolDto {
    pub(super) fn from_tool(t: &Tool) -> Self {
        ToolDto {
            kind: match t.kind {
                ToolKind::EndMill => ToolKindDto::EndMill,
                ToolKind::BallNose => ToolKindDto::BallNose,
                ToolKind::BullNose => ToolKindDto::BullNose,
                ToolKind::Drill => ToolKindDto::Drill,
                ToolKind::Tap => ToolKindDto::Tap,
                ToolKind::Bore => ToolKindDto::Bore,
                ToolKind::Turning => ToolKindDto::Turning,
                ToolKind::Groove => ToolKindDto::Groove,
                ToolKind::Thread => ToolKindDto::Thread,
                ToolKind::Form => ToolKindDto::Form,
                ToolKind::Taper => ToolKindDto::Taper,
                ToolKind::Probe => ToolKindDto::Probe,
                ToolKind::Torch => ToolKindDto::Torch,
            },
            diameter: t.diameter,
            corner_r: t.corner_r,
            tip_angle: t.tip_angle,
            pitch: t.pitch,
            nose_radius: t.nose_radius,
            width: t.width,
            flute_len: t.flute_len,
            shank_d: t.shank_d,
            shank_len: t.shank_len,
            holder_d: t.holder_d,
            holder_len: t.holder_len,
            material_group: t.material_group.clone(),
            rpm: t.rpm,
            feed: t.feed,
            tool_material: t.tool_material.map(|m| match m {
                crate::tools::ToolMaterial::Hss => ToolMaterialDto::Hss,
                crate::tools::ToolMaterial::Carbide => ToolMaterialDto::Carbide,
            }),
            flutes: t.flutes,
            center_cutting: t.center_cutting,
            number: None,
            body: None,
            cutter: None,
            orientation: t.orientation,
            turning: t.turning,
        }
    }

    pub(super) fn to_tool(&self) -> Tool {
        Tool {
            kind: match self.kind {
                ToolKindDto::EndMill => ToolKind::EndMill,
                ToolKindDto::BallNose => ToolKind::BallNose,
                ToolKindDto::BullNose => ToolKind::BullNose,
                ToolKindDto::Drill => ToolKind::Drill,
                ToolKindDto::Tap => ToolKind::Tap,
                ToolKindDto::Bore => ToolKind::Bore,
                ToolKindDto::Turning => ToolKind::Turning,
                ToolKindDto::Groove => ToolKind::Groove,
                ToolKindDto::Thread => ToolKind::Thread,
                ToolKindDto::Form => ToolKind::Form,
                ToolKindDto::Taper => ToolKind::Taper,
                ToolKindDto::Probe => ToolKind::Probe,
                ToolKindDto::Torch => ToolKind::Torch,
            },
            diameter: self.diameter,
            corner_r: self.corner_r,
            tip_angle: self.tip_angle,
            pitch: self.pitch,
            nose_radius: self.nose_radius,
            width: self.width,
            flute_len: self.flute_len,
            shank_d: self.shank_d,
            shank_len: self.shank_len,
            holder_d: self.holder_d,
            holder_len: self.holder_len,
            material_group: self.material_group.clone(),
            rpm: self.rpm,
            feed: self.feed,
            tool_material: self.tool_material.map(|m| match m {
                ToolMaterialDto::Hss => crate::tools::ToolMaterial::Hss,
                ToolMaterialDto::Carbide => crate::tools::ToolMaterial::Carbide,
            }),
            flutes: self.flutes,
            center_cutting: self.center_cutting,
            orientation: self.orientation,
            turning: self.turning,
        }
    }
}

/// The file mirror of `operation::SrcRef` (names, like everywhere else).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SrcDto {
    Stock,
    Entity { name: String },
    Selection { items: Vec<GeoSelDto> },
}

/// The file mirror of `operation::GeoSel`: a named entity and one of its
/// feature ids. Ids are geometry hashes (stable ids), not positions, so a
/// saved pick survives save/load and facet reorder.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GeoSelDto {
    pub entity: String,
    pub feature: FeatureSelDto,
}

/// The file mirror of `operation::FeatureSel`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FeatureSelDto {
    Face { id: u64 },
    Chain { id: u64 },
    Patch { id: u64 },
    /// M13.4: additive - files written before it never carry the tag,
    /// so they load unchanged.
    Vertex { id: u64 },
}
