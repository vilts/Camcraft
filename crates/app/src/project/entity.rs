use super::*;

/// One CL move: kind, endpoints, contact feed (absent for rapids).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct MoveDto {
    pub kind: MoveKindDto,
    pub from: [f64; 3],
    pub to: [f64; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<f64>,
    /// Added at v5: the circular segment this move traverses, absent for
    /// the straight-line moves every pre-v5 path held (and every path
    /// with no arcs still writes - the field skips serialization).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arc: Option<ArcDto>,
    /// Added at v22 (M5.1 stage A, no version bump - the arc-v5
    /// precedent): the tool pose the move states, as the unit axis's
    /// lattice-snapped components. Absent means no angle stated -
    /// what every path this app has ever generated held - so a save
    /// of a prismatic document is byte-shape identical to its
    /// pre-seam self, and the field skips serialization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<[f64; 3]>,
    /// Added with issue #33 (no version bump - the axis-v22 additive
    /// precedent): the spindle's stated C angle, unwrapped degrees on
    /// the lattice. Absent on every move but a C-axis live-tool cut,
    /// so every document without one saves byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotary: Option<f64>,
}

/// An arc move's center with direction. `kind` stays the motion class
/// (rapid/feed/plunge); the arc is geometry. `center` is the plane's
/// word-order pair (`cl::ArcPlane`): XY's (x, y) for every arc saved
/// before M11.2.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ArcDto {
    pub center: [f64; 2],
    pub ccw: bool,
    /// Added with M11.2 A (no version bump - the arc-v5 / axis-v22
    /// additive precedent): `"xz"` / `"yz"` for a vertical-plane arc;
    /// absent is XY, which every pre-M11.2 file holds and every XY arc
    /// still writes - the key skips serialization, so a save of an
    /// XY-only document is byte-identical to its pre-M11.2 self.
    #[serde(default, skip_serializing_if = "ArcPlaneDto::is_xy")]
    pub plane: ArcPlaneDto,
}

/// The stored plane spelling (`cl::ArcPlane`'s file face).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ArcPlaneDto {
    #[default]
    Xy,
    Xz,
    Yz,
}

impl ArcPlaneDto {
    fn is_xy(&self) -> bool {
        *self == ArcPlaneDto::Xy
    }

    fn from_plane(p: ArcPlane) -> Self {
        match p {
            ArcPlane::Xy => ArcPlaneDto::Xy,
            ArcPlane::Xz => ArcPlaneDto::Xz,
            ArcPlane::Yz => ArcPlaneDto::Yz,
        }
    }

    fn to_plane(self) -> ArcPlane {
        match self {
            ArcPlaneDto::Xy => ArcPlane::Xy,
            ArcPlaneDto::Xz => ArcPlane::Xz,
            ArcPlaneDto::Yz => ArcPlane::Yz,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MoveKindDto {
    Rapid,
    Feed,
    Plunge,
}

/// The cached generation output: path plus the warnings that came with it,
/// plus - from v10 - the canned-cycle list that rides alongside the moves
/// of a drilling path (see `crate::cl::Cycle`). Absent from any pre-v10
/// cache, which is exactly right: no pre-v10 build ever generated a cycle.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PathDto {
    pub moves: Vec<MoveDto>,
    pub warnings: Vec<Warning>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cycles: Vec<CycleDto>,
    /// The generator's status notes (M8.1): substitutions the entry
    /// laws made at generation time. Absent is none - a pre-Phase-8
    /// cache generated no notes, and the key never appears on a path
    /// generated quiet (the cycles-v10/rev-v20 additive law).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// The control-compensated spans (M13.11, `crate::cl::CompSpan`):
    /// absent is none - every path cut in computer, and every cache
    /// before the option. The same additive law as the notes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comps: Vec<CompSpanDto>,
    /// The threading-cycle records (issue #34): the same additive law.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub threads: Vec<super::thread_dto::ThreadCycleDto>,
    /// The probing touches (#108, `crate::cl::ProbeTouch`): absent is
    /// none - every cutting path. A touch lost on load would post as a
    /// plain feed through the part, so a touch that does not sit on
    /// its move is dropped AND the path's probe op re-generates (its
    /// cache no longer matches: see `to_cache`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub probes: Vec<super::touch_dto::ProbeTouchDto>,
    /// The declared edge breaks (#303, `crate::cl::EdgeBreak`): the
    /// same additive law - absent is none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<super::break_dto::EdgeBreakDto>,
    /// The plasma record (#670, `crate::cl::PlasmaCut`): the same
    /// additive law - absent is none, every path but a plasma cut's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plasma: Option<super::plasma_dto::PlasmaCutDto>,
}

/// One cached control-compensated span: the engage / cancel move
/// indices, the side, the register's radius, and the programmed
/// moves the post writes in place of the centre ones - the same
/// lattice snap `MoveDto` gives every coordinate, so a span
/// round-trips byte-identically with its moves.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CompSpanDto {
    pub side: CompSideDto,
    pub radius: f64,
    /// Wear compensation (#38): written only when on; absent is the
    /// control variant, every cache before the option.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wear: bool,
    pub first: u64,
    pub last: u64,
    pub programmed: Vec<MoveDto>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CompSideDto {
    Left,
    Right,
}

impl CompSpanDto {
    fn from_span(c: &crate::cl::CompSpan) -> Self {
        CompSpanDto {
            side: match c.side {
                crate::cl::CompSide::Left => CompSideDto::Left,
                crate::cl::CompSide::Right => CompSideDto::Right,
            },
            radius: crate::cl::lattice(c.radius),
            wear: c.wear,
            first: c.first as u64,
            last: c.last as u64,
            programmed: c.programmed.iter().map(MoveDto::from_move).collect(),
        }
    }

    fn to_span(&self) -> crate::cl::CompSpan {
        crate::cl::CompSpan {
            side: match self.side {
                CompSideDto::Left => crate::cl::CompSide::Left,
                CompSideDto::Right => crate::cl::CompSide::Right,
            },
            radius: self.radius,
            wear: self.wear,
            first: usize::try_from(self.first).unwrap_or(usize::MAX),
            last: usize::try_from(self.last).unwrap_or(usize::MAX),
            programmed: self.programmed.iter().map(|m| m.to_move()).collect(),
        }
    }
}

/// One cached cycle: the post's source of truth for a hole, stored beside
/// (not instead of) the moves it expands to. Floats take the same lattice
/// snap `MoveDto` gives its coordinates, so a cycle round-trips
/// byte-identically and stays bit-exact with its moves' depths - what the
/// agreement law pins.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CycleDto {
    pub kind: CycleKindDto,
    pub xy: [f64; 2],
    pub r_z: f64,
    pub bottom_z: f64,
    pub entry_z: f64,
    pub peck: f64,
    pub dwell: f64,
    pub first_move: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CycleKindDto {
    Spot,
    Drill,
    Peck,
    Deep,
    Tap,

    Bore,
    BackBore,
    FloatingTap,
}

impl CycleDto {
    fn from_cycle(c: &Cycle) -> Self {
        CycleDto {
            kind: match c.kind {
                CycleKind::Spot => CycleKindDto::Spot,
                CycleKind::Drill => CycleKindDto::Drill,
                CycleKind::Peck => CycleKindDto::Peck,
                CycleKind::Deep => CycleKindDto::Deep,
                CycleKind::Tap => CycleKindDto::Tap,
                CycleKind::FloatingTap => CycleKindDto::FloatingTap,
                CycleKind::Bore => CycleKindDto::Bore,
                CycleKind::BackBore => CycleKindDto::BackBore,
            },
            xy: [lattice(c.xy.x), lattice(c.xy.y)],
            r_z: lattice(c.r_z),
            bottom_z: lattice(c.bottom_z),
            entry_z: lattice(c.entry_z),
            peck: lattice(c.peck),
            dwell: lattice(c.dwell),
            first_move: c.first_move as u64,
        }
    }

    pub(super) fn to_cycle(self) -> Cycle {
        Cycle {
            kind: match self.kind {
                CycleKindDto::Spot => CycleKind::Spot,
                CycleKindDto::Drill => CycleKind::Drill,
                CycleKindDto::Peck => CycleKind::Peck,
                CycleKindDto::Deep => CycleKind::Deep,
                CycleKindDto::Tap => CycleKind::Tap,
                CycleKindDto::FloatingTap => CycleKind::FloatingTap,
                CycleKindDto::Bore => CycleKind::Bore,
                CycleKindDto::BackBore => CycleKind::BackBore,
            },
            xy: Point2::new(self.xy[0], self.xy[1]),
            r_z: self.r_z,
            bottom_z: self.bottom_z,
            entry_z: self.entry_z,
            peck: self.peck,
            dwell: self.dwell,
            first_move: usize::try_from(self.first_move).unwrap_or(usize::MAX),
        }
    }
}

/// A setup as files hold it (M3.2 A): the furniture it owns, by name.
/// `wcs` names the setup's one WCS entity (v54, #342); it may name an
/// entity the file does not contain - the WCS is simply not placed yet.
/// Older files held a list indexed by [`WCS_SLOTS`] (see [`WcsDto`]).
/// `datum_from` is
/// clone provenance for the setup sheet (stage D); every stamp ignores
/// it, so it defaults and rides along for the reader, not the machinery.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct SetupDto {
    pub stock: String,
    #[serde(default)]
    pub wcs: WcsDto,
    #[serde(default)]
    pub datum_from: String,
    /// v51 (#196): the setup's picked program offset as its G-word
    /// (`"G55"`). Absent - every older file, and a setup never picked -
    /// is the inferred per-operation law, so the key is the migration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_offset: Option<String>,
    /// v53 (#326): how the part sits in this setup, document -> setup
    /// frame. Absent - every older file, and a setup holding the part as
    /// modelled - is the identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_alignment: Option<AlignmentDto>,
    /// v55 (#343): the setup's own machine. Absent follows the document's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<SetupMachineDto>,
    /// v55 (#343): the setup's own post. Absent follows the document's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<SetupPostDto>,
    /// v56 (#316): the setup this one is placed relative to. Absent -
    /// every older file - links nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_to: Option<String>,
    /// v57 (#320): where the setup's stock comes from (`"own"` |
    /// `"from_previous"`). Absent - every older file, and a setup never
    /// picked - is #317's implicit rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock_source: Option<String>,
    /// #389: the setup's own post options, additive (no version step).
    /// Absent follows the document's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_options: Option<SetupPostOptionsDto>,
    /// #1031: the rated maximum rpm of the setup's chuck or collet,
    /// additive (no version step). Absent - every older file - states none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workholding_max_rpm: Option<f64>,
}

/// A setup's WCS as files hold it. v54 (#342) writes one entity name
/// (`"wcs": "Setup2/WCS"`); v21-v53 wrote one name per `G54`..`G59` slot
/// (`"wcs": ["Setup2/G54", ...]`), which only the v53 -> v54 rung reads
/// (`project::wcs_one`). Absent is an empty name: the default entity.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum WcsDto {
    One(String),
    Slots(Vec<String>),
}

impl Default for WcsDto {
    fn default() -> Self {
        WcsDto::One(String::new())
    }
}

/// A setup's part alignment as files hold it (#326): the same
/// translation + unit quaternion (x, y, z, w) an entity's placement uses.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AlignmentDto {
    pub translation: [f64; 3],
    pub rotation: [f64; 4],
}

impl AlignmentDto {
    #[must_use]
    pub fn from_location(l: &crate::schema::Location) -> Self {
        AlignmentDto { translation: [l.pos.x, l.pos.y, l.pos.z], rotation: l.rot }
    }

    /// The stored transform, or `None` when it is not rigid (a non-finite
    /// number, a quaternion off unit length by more than float noise).
    #[must_use]
    pub fn to_location(&self) -> Option<crate::schema::Location> {
        let [x, y, z] = self.translation;
        let q = self.rotation;
        let n2 = q.iter().map(|v| v * v).sum::<f64>();
        if !(x.is_finite() && y.is_finite() && z.is_finite()) || !n2.is_finite() || (n2 - 1.0).abs() > 1e-9 {
            return None;
        }
        Some(crate::schema::Location { pos: crate::geometry::Point3::new(x, y, z), rot: q })
    }
}

/// The fixture marker as files hold it (M3.2 stage B): a tag on a model
/// entity - the solid itself is that model's own `facets`, its placement
/// the entity's `translation`/`rotation`, and the machine's table is
/// never stored here (it derives from the `machine` key). `setup` is the
/// DECLARED binding, same grammar as `OperationDto::setup`: absent
/// means the first setup by name (the implicit-setup law), so a tagged
/// model in a setup-free document saves as one key and nothing more.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct FixtureDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
    /// Stands still while a rotary turns the part (M13.8 (2)); absent
    /// reads false - it turns with the part, every older file's law -
    /// and false saves as no key, so such files round-trip unchanged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixed: bool,
    /// A built-in vise's parameters (#788); absent - plain steel. Kept
    /// as raw JSON so a key this build can't read is dropped with a load
    /// note, never a failed load (#812, `fixture_keys`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vise: Option<serde_json::Value>,
    /// A fixture plate's grid (#819); absent - not a plate. Kept as raw
    /// JSON so a key this build can't read is dropped and logged on
    /// load, never a failed load (#812).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plate: Option<serde_json::Value>,
    /// Where this fixture stands on a plate (#819); same lenient read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_plate: Option<serde_json::Value>,
    /// A mod-vise jaw block's parameters (#820); same lenient read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_jaw: Option<serde_json::Value>,
    /// A built-in accessory's kind and dimensions (#789); absent - not
    /// one. Same lenient read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessory: Option<serde_json::Value>,
}

/// An operation as files hold it: definition, the stamp the last generate
/// saw (raw digest; its meaning lives in `operation::current_stamp`), and
/// the cache that stamp was taken for. `stamp` present without a cache (or
/// the reverse, e.g. after an edit clears the stamp but keeps the old path
/// on screen) is legitimate and reads Stale.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OperationDto {
    /// Optional UI label; the enclosing entity name remains its stable key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Position within the setup; absent preserves legacy name order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<u32>,
    pub tool: String,
    pub src: SrcDto,
    pub params: ParamsDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stamp: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<PathDto>,
    /// The setup this operation machines in (M3.2 A). Absent means the
    /// first setup by name - exactly what every pre-v21 operation was
    /// (the implicit-setup law), so a v21 save of a setup-free document
    /// is byte-identical to its v20 save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
    /// The area controls (M11.5). Absent means the identity - what every
    /// earlier file held - and an untouched operation writes nothing, so
    /// no version bump (the v22 `axis` precedent).
    #[serde(default, skip_serializing_if = "AreaDto::is_default")]
    pub area: AreaDto,
    /// The op's coolant (#112), `off` / `flood` / `mist` /
    /// `through-tool`. Absent reads flood - every earlier file's post -
    /// and flood writes no key, so older files round-trip byte for byte
    /// with no version bump (the `area` precedent).
    #[serde(default, skip_serializing_if = "crate::operation::CoolantMode::is_default")]
    pub coolant: crate::operation::CoolantMode,
    /// The op's after-action (#110), `optional-stop` / `program-stop` /
    /// `go-home`; absent reads none (carry on), the coolant's law.
    #[serde(default, skip_serializing_if = "crate::operation::AfterOperation::is_default")]
    pub after: crate::operation::AfterOperation,
    /// Left out of the posted program (#110); absent reads false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub suppressed: bool,
}

impl MoveDto {
    /// Coordinates stand on the 1 um lattice ([`crate::cl::lattice`]):
    /// every float a file carries is then a fixed point of write-parse,
    /// so save-load-save is byte-identical - an off-lattice double's
    /// shortest decimal can parse back one ulp over from what was
    /// written, and a cache that drifts a ulp per reopen is a file
    /// that never repeats itself. Generated coordinates are already
    /// lattice values (clipper's grid, user-typed feeds); the snap
    /// only ever moves anything computed - a lead tangent point - and
    /// does so by at most half a micron.
    fn from_move(m: &Move) -> Self {
        let lat = |p: Point3| [lattice(p.x), lattice(p.y), lattice(p.z)];
        MoveDto {
            kind: match m.kind {
                MoveKind::Rapid => MoveKindDto::Rapid,
                MoveKind::Feed => MoveKindDto::Feed,
                MoveKind::Plunge => MoveKindDto::Plunge,
            },
            from: lat(m.from),
            to: lat(m.to),
            feed: m.feed.map(lattice),
            arc: m.arc.map(|a| ArcDto {
                center: [lattice(a.center.x), lattice(a.center.y)],
                ccw: a.ccw,
                plane: ArcPlaneDto::from_plane(a.plane),
            }),
            // The exact vertical stays exactly (0, 0, 1) through the
            // snap (zeros and ones are lattice fixed points), so a
            // stated-up path saves and reloads into the prismatic
            // code paths the sim and dump branch on.
            axis: m
                .axis
                .map(|a| [lattice(a[0]), lattice(a[1]), lattice(a[2])]),
            rotary: m.rotary.map(lattice),
        }
    }

    pub(super) fn to_move(self) -> Move {
        let m = Move::new(
            match self.kind {
                MoveKindDto::Rapid => MoveKind::Rapid,
                MoveKindDto::Feed => MoveKind::Feed,
                MoveKindDto::Plunge => MoveKind::Plunge,
            },
            Point3::new(self.from[0], self.from[1], self.from[2]),
            Point3::new(self.to[0], self.to[1], self.to[2]),
            self.feed,
        );
        let m = match self.arc {
            Some(a) => m.with_arc_in(
                Point2::new(a.center[0], a.center[1]),
                a.ccw,
                a.plane.to_plane(),
            ),
            None => m,
        };
        // VERBATIM, not `with_axis`: the file's numbers are the law -
        // re-normalizing on load would move a saved axis by bits and
        // break the save-load-save byte identity (the lattice snap in
        // from_move is already the normalization gate at write time).
        let mut m = m.attach_axis(self.axis);
        // Verbatim too: the stored angle is already on the lattice.
        m.rotary = self.rotary.filter(|c| c.is_finite());
        m
    }
}

impl PathDto {
    pub(super) fn from_cache(t: &ToolpathCache) -> Self {
        PathDto {
            moves: t.path.moves.iter().map(MoveDto::from_move).collect(),
            warnings: t.warnings.clone(),
            cycles: t.path.cycles.iter().map(CycleDto::from_cycle).collect(),
            notes: t.path.notes.clone(),
            comps: t.path.comps.iter().map(CompSpanDto::from_span).collect(),
            threads: t.path.threads.iter().map(super::thread_dto::ThreadCycleDto::from_cycle).collect(),
            probes: t.path.probes.iter().map(super::touch_dto::ProbeTouchDto::from_touch).collect(),
            breaks: t.path.breaks.iter().map(super::break_dto::EdgeBreakDto::from_break).collect(),
            plasma: t.path.plasma.as_ref().map(super::plasma_dto::PlasmaCutDto::from_cut),
        }
    }

    pub(super) fn to_cache(&self) -> ToolpathCache {
        let mut path = Toolpath::new(self.moves.iter().map(|m| m.to_move()).collect());
        path.cycles = self
            .cycles
            .iter()
            .copied()
            .map(CycleDto::to_cycle)
            .collect();
        path.notes = self.notes.clone();
        // A span that does not chain onto the loaded moves is not
        // trusted (the dump refuses it by name too): the file's bytes
        // are the law, the sanity door is the reader's.
        path.comps = self
            .comps
            .iter()
            .map(CompSpanDto::to_span)
            .filter(|c| c.is_sane(&path.moves))
            .collect();
        path.probes = super::touch_dto::load(&self.probes, &path.moves);
        path.threads = self.threads.iter().filter_map(|t| t.to_cycle(&path.moves)).collect();
        path.breaks = super::break_dto::load(&self.breaks);
        path.plasma = self.plasma.as_ref().and_then(|p| p.load(&path.moves));
        ToolpathCache::new(path, self.warnings.clone())
    }
}

#[derive(Serialize, Deserialize)]
pub struct EntityDto {
    pub name: String,
    pub translation: [f64; 3],
    /// Unit quaternion (x, y, z, w). Defaulted so v1 files parse; the v1 -> v2
    /// migration below makes that choice explicit rather than implicit.
    #[serde(default = "identity_rotation")]
    pub rotation: [f64; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facets: Option<Vec<[[f64; 3]; 3]>>, // a, b, c per triangle
    /// Geometry-revision counter (M3.1, `schema::Revision`). Absent means 0,
    /// which is what every pre-v20 file held; a model always carries >= 1
    /// (import stamps the first landing), so this key never appears on
    /// setup furniture and a v20 save of a pre-M3.1 document is byte-shape
    /// identical to its v19 save.
    #[serde(default, skip_serializing_if = "is_unrevised")]
    pub rev: u64,
    /// The chord error (mm) of a model imported coarser than the STEP
    /// importer promises (#864, `schema::ReducedFidelity`). Absent on
    /// every other entity, so a document without such a model - and every
    /// file from before the key existed - saves byte for byte as it did
    /// and loads as full fidelity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coarse_mm: Option<f64>,
    /// The STEP facet budget a model was imported under when it is not the
    /// default (#1054, `schema::StepBudget`). Absent everywhere else, so a
    /// file from before the key existed - and every default import - reads
    /// as `step_budget::DEFAULT` and saves byte for byte as it did. An old
    /// reader dropping it loses only the record: the facets are stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facet_budget: Option<u64>,
    /// Absent means Model; only non-model entities carry the tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<EntityKind>,
    /// Stock parameters; present exactly when `kind == Some(Stock)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock: Option<StockDto>,
    /// #1094: a from-model stock's source model is raw stock only, not
    /// judged as the part. Absent is false - every older file judges it,
    /// and an older reader that drops the key judges more, never less.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub raw_stock_only: bool,
    /// #1094: the fingerprint of the source model's geometry the mark was
    /// set on (`stock_print::to_text`). The mark loads only with it; a
    /// flag without it (an early file, or by hand) loads unmarked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_stock_print: Option<String>,
    /// Tool definition; present exactly when `kind == Some(Tool)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolDto>,
    /// Operation definition (+ cached path); present exactly when
    /// `kind == Some(Operation)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<OperationDto>,
    /// Setup definition; present exactly when `kind == Some(Setup)`
    /// (M3.2 A).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<SetupDto>,
    /// Fixture marker (M3.2 B); present exactly on an UNTAGGED-kind
    /// (Model) entity - a fixture is a model that collision checks
    /// respect, so it borrows the model row's geometry, never a kind
    /// of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixture: Option<FixtureDto>,
    /// A Pause / Home entry (#155); present exactly when
    /// `kind == Some(Entry)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<super::program_entry_dto::ProgramEntryDto>,
}
