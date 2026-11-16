use super::*;

use crate::machining2d::FaceHoles;
use crate::operation::Tilt;

/// The file mirror of `operation::Tilt` (M5.1 stage B): plain f64
/// letters and anchor - serde's shortest-representation floats parse
/// back to the same bits, so a round trip needs no lattice (unlike
/// the normalized axis vectors the CL stream stores).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct TiltDto {
    pub a_deg: f64,
    pub b_deg: f64,
    pub origin: [f64; 3],
}

impl From<Tilt> for TiltDto {
    fn from(t: Tilt) -> Self {
        TiltDto {
            a_deg: t.a_deg,
            b_deg: t.b_deg,
            origin: t.origin,
        }
    }
}

impl From<TiltDto> for Tilt {
    fn from(d: TiltDto) -> Self {
        Tilt {
            a_deg: d.a_deg,
            b_deg: d.b_deg,
            origin: d.origin,
        }
    }
}

/// The file mirror of `operation::OpParams`, tool-free by the same rule as
/// the domain type: the diameter lives in the tool, never a parameter.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ParamsDto {
    Face {
        stepover: f64,
        clearance_z: HeightDto,
        cut_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Declared at v5, wired in step 4: absent (or the defaults) is
        /// exactly the legacy single-level face.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        top_z: Option<HeightDto>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepdown: Option<f64>,
        /// M11.1 A turning-point blends: absent (and written only when
        /// ON) is the sharp serpentine every earlier file held - no
        /// version step, the same pure default as `tilt`.
        #[serde(default, skip_serializing_if = "is_false")]
        blend_turns: bool,
        /// M13.11 hole law: absent (and written only when not the
        /// default) is `lift` - a file that predates the word lifts
        /// over its holes, the safe reading; no version step.
        #[serde(default, skip_serializing_if = "FaceHolesDto::is_default")]
        holes: FaceHolesDto,
        /// Declared at v22-era M5.1 B: absent is no index - the
        /// prismatic op every earlier file held.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
    },
    Contour {
        /// M11.2 level transitions and links: absent is the kind's earlier shape.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        links: Option<LinksDto>,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        retract_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        lead: LeadDto,
        tolerance: f64,
        tab_count: u32,
        tab_width: f64,
        tab_height: f64,
        /// Declared at v7, wired with the side feature: absent is exactly
        /// `auto`, the choice every pre-v7 contour file silently had -
        /// Auto resolves at generate, so the stored path never changes.
        #[serde(default)]
        side: SideDto,
        /// Absent means `plunge` - the graze at the lead-in every
        /// contour posted before M8.2 A gave the wall ladder an entry
        /// field at all (additive defaults: the format version does
        /// not move).
        #[serde(default)]
        entry: EntryDto,
        /// Read only when `entry` is Helix; absent means the seed's
        /// 1.0 mm, the same default the pockets carry.
        #[serde(default = "default_helix_radius")]
        helix_radius: f64,
        /// M11.1 B turning-point blends: absent (written only when ON)
        /// is the sharp walk every earlier file held - no version step.
        #[serde(default, skip_serializing_if = "is_false")]
        blend_turns: bool,
        /// Level to level: absent is `retract_reenter`, the transition
        /// every earlier file posted - written only when the user picks
        /// the stay-down ramp, so no version step.
        #[serde(default, skip_serializing_if = "LevelLinkDto::is_default")]
        level_link: LevelLinkDto,
        /// The stay-down ramp's maximum angle, degrees; absent is the
        /// 3 deg default, written only when it differs.
        #[serde(default = "default_level_ramp_angle", skip_serializing_if = "is_default_level_ramp")]
        ramp_angle: f64,
        /// 3+2 index; see the Face arm.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
    },
    Pocket {
        /// M11.2 level transitions and links: absent is the kind's earlier shape.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        links: Option<LinksDto>,
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: StrategyDto,
        /// Declared at v5, wired in step 5: absent is exactly "hop to
        /// clearance", the behavior every pre-v5 pocket had.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        /// Absent means `plunge` - the climb-and-dive every pocket
        /// posted before M5.1 H gave the planar ladder an entry field
        /// at all (additive defaults: the format version does not move).
        #[serde(default)]
        entry: EntryDto,
        /// Read only when `entry` is Helix; absent means the seed's
        /// 1.0 mm, the same default the 3-D pocket carries.
        #[serde(default = "default_helix_radius")]
        helix_radius: f64,
        /// Read only when `strategy` is Adaptive; absent means the
        /// law's 45° (`pocket3d`'s law, same word). Pre-H files that
        /// stored Adaptive meant the waterline alias - they now ring
        /// at r(1 - cos 45); RECOMMENDED.md owns that statement.
        /// The pre-M11.4 word: the contact angle in degrees. Still READ
        /// (absent meant the law's 45 deg), converted to `optimal_load`
        /// in memory on load - exact, `crate::load` - and never written
        /// again: a saved file carries `optimal_load` instead (additive,
        /// the format version does not move; M11.4 B).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_engagement: Option<f64>,
        /// M11.4 B: the adaptive optimal load, radial width as a fraction
        /// of the tool diameter (ae/D; the page shows it in % of D).
        /// Absent on every older file, which reads `max_engagement`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        optimal_load: Option<f64>,
        /// M11.1 C turning-point blends: absent (written only when ON)
        /// is the sharp walk every earlier file held - no version step.
        #[serde(default, skip_serializing_if = "is_false")]
        blend_turns: bool,
        /// #1733 corner arc radius: absent (the tool radius) on every
        /// earlier file - no version step.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blend_radius: Option<f64>,
        /// 3+2 index; see the Face arm.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
    },
    /// 2D Adaptive (M2.4 A): the pocket arm without the strategy tag -
    /// the kind fixes the ladder; every default the pocket needs, this
    /// arm states (the fields arrived with the kind, v43).
    Adaptive2D {
        /// M11.2 level transitions and links: absent is the kind's earlier shape.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        links: Option<LinksDto>,
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        #[serde(default)]
        entry: EntryDto,
        #[serde(default = "default_helix_radius")]
        helix_radius: f64,
        /// Pre-M11.4 Spiral files stored the contact angle in `max_engagement`
        /// with `optimal_load` absent (converted via `load_from_file`). For
        /// M11.4+ files (`optimal_load` present), this holds #1588's
        /// Trochoidal engagement limit (deg; absent means 90 deg).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_engagement: Option<f64>,
        /// M11.4 B: the adaptive optimal load, radial width as a fraction
        /// of the tool diameter (ae/D; the page shows it in % of D).
        /// Absent on every older file, which reads `max_engagement`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        optimal_load: Option<f64>,
        /// #1509: how the region is cleared. Absent on every older file,
        /// which reads as `spiral` - the path those files always cut.
        #[serde(default)]
        method: AdaptiveMethodDto,
        /// #1588: return between trochoidal loops (`auto`, `microlift`,
        /// `retract`); absent reads as `auto`.
        #[serde(default, rename = "return")]
        troch_return: AdaptiveTrochReturnDto,
        /// #1588: microlift height (mm); read when `return` is `microlift`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lift: Option<f64>,
        /// #1588: return feed (mm/min); absent uses cutting `feed`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        return_feed: Option<f64>,
        /// #1588: loop width (mm); absent uses all medial room.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        loop_width: Option<f64>,
        /// #1733 arc turns: absent (written only when ON) is the sharp
        /// walk every earlier file held - no version step.
        #[serde(default, skip_serializing_if = "is_false")]
        blend_turns: bool,
        /// #1733 corner arc radius: absent (the tool radius) on every
        /// earlier file - no version step.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blend_radius: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
    },
    /// 2D Offset (M2.4 A): the same arm minus the engagement door -
    /// style-A rings are spaced by the stepover alone.
    Offset2D {
        /// M11.2 level transitions and links: absent is the kind's earlier shape.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        links: Option<LinksDto>,
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        #[serde(default)]
        entry: EntryDto,
        #[serde(default = "default_helix_radius")]
        helix_radius: f64,
        /// #1733 arc turns: absent (written only when ON) is the sharp
        /// walk every earlier file held - no version step.
        #[serde(default, skip_serializing_if = "is_false")]
        blend_turns: bool,
        /// #1733 corner arc radius: absent (the tool radius) on every
        /// earlier file - no version step.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blend_radius: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
    },
    /// Engraving (M2.4 B): the trace arm verbatim under the new tag -
    /// the twin law again, so the arms state the same shape by
    /// construction (the fields arrived with the kind, v44).
    Engrave {
        clearance_z: HeightDto,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// 3+2 index; see the Face arm.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
    },
    /// 2D chamfer (M2.4 B): the two authored face numbers and three
    /// heights - no bottom key exists, because the face's depth is
    /// derived (the variant docs' law: a second number for the same
    /// line could only disagree with the first). The tag follows the
    /// 2D family's shipped grammar (`adaptive2_d`, `offset2_d`), not
    /// the `pocket3d` rename - the twins' word shape is the newer
    /// precedent the table already speaks.
    Chamfer2D {
        width: f64,
        angle_deg: f64,
        clearance_z: HeightDto,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default)]
        side: SideDto,
        tolerance: f64,
        /// 3+2 index; see the Face arm.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
    },
    /// Circular pocket / vault (M2.4 B): the pocket's arm with the
    /// authored disk's two numbers in place of a picked region -
    /// `center` rides the wire like the slot's endpoints, as `vault`
    /// is the dome law's one bool (the fields arrived with the
    /// kind, v44).
    Circular {
        /// M11.2 level transitions and links: absent is the kind's earlier shape.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        links: Option<LinksDto>,
        center: [f64; 2],
        radius: f64,
        vault: bool,
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default)]
        entry: EntryDto,
        #[serde(default = "default_helix_radius")]
        helix_radius: f64,
        /// 3+2 index; see the Face arm.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
    },
    Slot {
        /// M11.2 level transitions and links: absent is the kind's earlier shape.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        links: Option<LinksDto>,
        a: [f64; 2],
        b: [f64; 2],
        width: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        stepover: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Absent means `plunge` - the wall dive at the pass start
        /// every slot posted before M8.2 C gave the ladder an entry
        /// field at all (additive defaults: the format version does
        /// not move).
        #[serde(default)]
        entry: EntryDto,
        /// Read only when `entry` is Helix; absent means the seed's
        /// 1.0 mm, the same default the pockets carry.
        #[serde(default = "default_helix_radius")]
        helix_radius: f64,
        /// 3+2 index; see the Face arm.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
        /// #1731: how far the pass runs past `a` / past `b`, mm. Absent
        /// (and not written) when zero, so earlier files and saves stay
        /// byte-identical.
        #[serde(default, skip_serializing_if = "is_zero_f64")]
        extend_start: f64,
        #[serde(default, skip_serializing_if = "is_zero_f64")]
        extend_end: f64,
    },
    Trace {
        clearance_z: HeightDto,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// 3+2 index; see the Face arm.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
    },
    /// Declared at v10 with the drilling operation. The hole list is
    /// params data, not a live query: what recognition (or a typed edit)
    /// produced is what the file holds, and the source digest makes a
    /// moved model mark the op Stale beside it.
    Drill {
        clearance_z: HeightDto,
        /// Absent means the hop-to-clearance every drilling default
        /// uses, like the pocket family's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        /// The whole-pattern depth override; absent is each hole's own
        /// floor - the normal shape, so `skip_serializing_if` keeps the
        /// common file lean.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bottom_z: Option<HeightDto>,
        cycle: CycleChoiceDto,
        peck: f64,
        dwell: f64,
        feed: f64,
        plunge_feed: f64,
        holes: Vec<HoleDto>,
        /// The radial list (M5.2 F2). Absent - the common shape, since
        /// a mill document's list is empty and `skip` keeps the file
        /// byte-identical to a pre-v40 one - is "no cross holes".
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        cross: Vec<CrossHoleDto>,
    },
    /// Declared at v11 with the 3-D pocket. No footprint on the wire:
    /// a 3-D op's machine area is the model's silhouette at generation
    /// time (the level law reads the mesh, not picks), and the source
    /// digest is what marks a moved model Stale beside it. The rename
    /// states the tag - the automatic snake_case of `Pocket3D` would be
    /// `pocket3_d`, and a file contract deserves the cleaner word.
    #[serde(rename = "pocket3d")]
    Pocket3D {
        /// M11.2 level transitions and links: absent is the kind's earlier shape.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        links: Option<LinksDto>,
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: StrategyDto,
        /// Absent means hop-to-clearance, the pocket family's law.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        #[serde(default)]
        entry: EntryDto,
        /// Read only when `entry` is Helix; a stored number is the
        /// user's choice, never a stale derivation from the tool.
        helix_radius: f64,
        /// Absent means off: the v11 files predate the rest pass, and
        /// the plain level ladder is exactly what they generated.
        #[serde(default)]
        rest_rough: bool,
        /// (v13: read only when `strategy` is `Adaptive`; v12 files can
        /// carry no Adaptive ring family to feed it.)
        /// The pre-M11.4 word: the contact angle in degrees. Still READ
        /// (absent meant the law's 45 deg), converted to `optimal_load`
        /// in memory on load - exact, `crate::load` - and never written
        /// again: a saved file carries `optimal_load` instead (additive,
        /// the format version does not move; M11.4 B).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_engagement: Option<f64>,
        /// M11.4 B: the adaptive optimal load, radial width as a fraction
        /// of the tool diameter (ae/D; the page shows it in % of D).
        /// Absent on every older file, which reads `max_engagement`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        optimal_load: Option<f64>,
        /// Absent means off: v13 files posted every F word at the
        /// programmed feed, which is exactly what "no modulation"
        /// means for them.
        #[serde(default)]
        feed_opt: bool,
        /// Absent means off: the offset-shell roughing (M2.5 F)
        /// arrived after every stored file, and the level ladder is
        /// exactly what they generated. An append-only key - the
        /// version stands, per the step-up law for keys whose absence
        /// IS the old behavior.
        #[serde(default)]
        shell: bool,
    },
    /// Declared at v15 with the waterline. The rename states the tag
    /// for the same reason `pocket3d` does - the automatic snake_case
    /// of `Finish3D` would be `finish3_d`, and a file contract
    /// deserves the cleaner word. No stepdown key: the scallop law
    /// derives it from tolerance and tool, so storing one could only
    /// disagree with the two numbers that author it. v16 added the
    /// pattern (absent means `waterline`, what every v15 file ran)
    /// and the raster stepover - the one number the law does not
    /// author, because a stored override states a machinist's
    /// decision, not the tolerance's derivation.
    #[serde(rename = "finish3d")]
    Finish3D {
        /// Absent means Waterline (the v15 truth).
        #[serde(default)]
        pattern: FinishPatternDto,
        /// Absent follows the stepover cos-law from the tolerance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepover: Option<f64>,
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Absent means hop-to-clearance, the pocket family's law.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        /// Absent means no ramping - the hop, what every pre-v17 file
        /// ran (M2.2e). Written always, like `climb`.
        #[serde(default)]
        ramp: bool,
        /// Absent means the march's own floor (delta0/4096), what
        /// every pre-v18 file ran (M2.2e surface finish). Absent
        /// spells None exactly like `stepover`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_step: Option<f64>,
        /// Absent means 45 degrees - the slope limit the raster's
        /// shallow jurisdiction already grants, hence exactly what
        /// v17's chain certified (M2.2e surface finish). Written
        /// always, like `ramp`.
        #[serde(default = "default_ramp_angle")]
        ramp_angle: f64,
        /// Absent means the vertical plunge - the tangent entry every
        /// pre-0.5 file cut, byte for byte (M8.2 B). Written always,
        /// like `ramp`.
        #[serde(default)]
        entry_ramp: bool,
        /// Absent means the scallop law `2√(r·tol)` spaces the ring
        /// ladder - what every v45 file and every pattern but Steep-
        /// and-shallow runs (M2.5 A). Absent spells None exactly like
        /// `stepover`: the derived ladder is authored by the two
        /// numbers already in the file, not stored a third time.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        steep_stepover: Option<f64>,
        /// Absent means "no reference ball was named": Pencil keeps
        /// every concave corner its locus rests on, Corner always cuts
        /// its own band (M2.5 B). Absent spells None exactly like
        /// `stepover`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pencil_reference: Option<f64>,
        /// Absent means no land model: the path rides the model's own
        /// surface, holes and all - what every older file ran (#1196).
        /// Absent spells None exactly like `stepover`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill_openings: Option<f64>,
    },
    /// Declared at v48 with the deburr ride (M2.5 E B). `Deburr`'s
    /// automatic snake_case tag IS the clean word - no rename needed -
    /// and the six keys are the kind's whole content: `width` is the
    /// one authored number (the tip's depth follows from it and the
    /// TOOL, never stored, per the tool-free rule), and the kernel
    /// hops every run to `clearance_z` itself, so there is no
    /// `retract_z` key to be absent. A v47 file cannot contain this
    /// variant (the arrival law).
    Deburr {
        width: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
    },
    /// Declared at v24 with the simultaneous-axis kinds (M5.1 stage
    /// C). The renames state the tags for the familiar reason - the
    /// automatic snake_case of `Contour5D` would be `contour5_d`,
    /// and a file contract deserves the cleaner word. No `pattern`
    /// key: the kind IS the choice of kinematic (the two-kinds-not-
    /// one-table decision), and no stepdown/ramp/min_step keys: the
    /// scallop law authors the ladder and the plunge owns the
    /// entry. The axis trio is written always - it is the whole
    /// content of the kind, and an absent lean would silently
    /// re-post someone's tilted program vertical.
    #[serde(rename = "contour5d")]
    Contour5D {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Absent means hop-to-clearance, the pocket family's law.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// Absent means off - the pose law every v28 file ran (M5.1
        /// stage F).
        #[serde(default)]
        shank_avoid: bool,
    },
    /// Declared at v24, the parallel twin (M5.1 stage C). The one
    /// extra field is the raster's optional stepover, absent-means-
    /// derived exactly as the finish's.
    #[serde(rename = "parallel5d")]
    Parallel5D {
        tolerance: f64,
        /// Absent follows the stepover cos-law from the tolerance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepover: Option<f64>,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// Absent means off, the contour's twin above.
        #[serde(default)]
        shank_avoid: bool,
    },
    /// Declared at v25 with the swarf family (M5.1 stage D). The
    /// parallel's shape field for field - the ruling is not a
    /// parameter but a reading of the model, and the file that
    /// carries no model can no more state it than a contour file
    /// can state an edge's hash that isn't there.
    #[serde(rename = "swarf")]
    Swarf {
        tolerance: f64,
        /// Absent follows the stepover cos-law from the tolerance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepover: Option<f64>,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
    },
    /// The ladder's one extra word: the count of tolerance rungs,
    /// written always - the kind IS the ladder, and an absent count
    /// would silently re-post someone's roughing program as a
    /// single finish pass. The range 1..4 is the door's, at load.
    #[serde(rename = "advanced_swarf")]
    AdvancedSwarf {
        tolerance: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepover: Option<f64>,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        passes: u32,
    },
    /// Declared at v26 with the rotary twins (M5.1 stage E). The
    /// contour's shape field for field - the axis of revolution is a
    /// reading of the model, not a parameter, and the automatic
    /// snake_case needs no rename here: the names were never awkward.
    RotaryContour {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Absent means hop-to-clearance, the pocket family's law.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
    },
    /// The parallel twin's shape: the raster's optional stepover,
    /// absent-means-derived exactly as the finish's (M5.1 stage E).
    RotaryParallel {
        tolerance: f64,
        /// Absent follows the stepover cos-law from the tolerance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepover: Option<f64>,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
    },
    /// Declared at v27 with the wrap ladder (M5.1 stage E2). The
    /// pocket's shape, read the wrap's way - and deliberately no
    /// `retract_z` key: the kernel names its own rapids plane one
    /// infeed and a tool radius off the skin, so a stored hop height
    /// could only ever be ignored, and the file contract says what
    /// the machine hears.
    RotaryOffset {
        /// M11.2 level transitions and links: absent is the kind's earlier shape.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        links: Option<LinksDto>,
        tolerance: f64,
        /// Developed arc-length spacing, a plain number like the
        /// pocket's - the ladder has no cos-law to derive from.
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        /// Radial infeed per wrap level.
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: StrategyDto,
        #[serde(default)]
        entry: EntryDto,
        /// Read only when `entry` is Helix; the pocket's law.
        helix_radius: f64,
        /// The pre-M11.4 word: the contact angle in degrees. Still READ
        /// (absent meant the law's 45 deg), converted to `optimal_load`
        /// in memory on load - exact, `crate::load` - and never written
        /// again: a saved file carries `optimal_load` instead (additive,
        /// the format version does not move; M11.4 B).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_engagement: Option<f64>,
        /// M11.4 B: the adaptive optimal load, radial width as a fraction
        /// of the tool diameter (ae/D; the page shows it in % of D).
        /// Absent on every older file, which reads `max_engagement`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        optimal_load: Option<f64>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
    },
    /// Declared at v28 with the stamped ladder (M5.1 stage E3). The
    /// pocket's shape field for field, minus the `rest_rough` and
    /// `feed_opt` switches the stamp has never been shown to commute
    /// with, plus the twins' pose trio and the verify budget - the
    /// file contract says what the generator reads, and the
    /// generator reads these and nothing else.
    MultiAxisRough {
        /// M11.2 level transitions and links: absent is the kind's earlier shape.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        links: Option<LinksDto>,
        tolerance: f64,
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        /// Absent means hop-to-clearance, the pocket family's law.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: StrategyDto,
        #[serde(default)]
        entry: EntryDto,
        /// Read only when `entry` is Helix; the pocket's law.
        helix_radius: f64,
        /// The pre-M11.4 word: the contact angle in degrees. Still READ
        /// (absent meant the law's 45 deg), converted to `optimal_load`
        /// in memory on load - exact, `crate::load` - and never written
        /// again: a saved file carries `optimal_load` instead (additive,
        /// the format version does not move; M11.4 B).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_engagement: Option<f64>,
        /// M11.4 B: the adaptive optimal load, radial width as a fraction
        /// of the tool diameter (ae/D; the page shows it in % of D).
        /// Absent on every older file, which reads `max_engagement`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        optimal_load: Option<f64>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// Absent means off, the twins' law - here it is the ladder's
        /// floors that answer to it, posed only when the shank speaks.
        #[serde(default)]
        shank_avoid: bool,
    },
    /// Declared at v31 with flowline finishing (M5.1 stage I). The
    /// parallel's shape field for field - the direction field is a
    /// reading of the model, not a parameter, exactly as the ruling
    /// and the rotary axis before it; the rename states the tag for
    /// the familiar reason - the automatic snake_case of `Flow5D`
    /// would be `flow5_d`.
    #[serde(rename = "flow5d")]
    Flow5D {
        tolerance: f64,
        /// Absent follows the stepover cos-law from the tolerance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepover: Option<f64>,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// Absent means off, the twins' law.
        #[serde(default)]
        shank_avoid: bool,
    },
    /// Declared at v32 with the drive-curve contour (M5.1 stage J).
    /// Flow's shape minus the stepover - the picked chains ARE the
    /// path, there is no spacing to author - and the automatic
    /// snake_case needs no rename here: `multi_axis_contour` is
    /// already the tag.
    MultiAxisContour {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// Absent means off, the twins' law.
        #[serde(default)]
        shank_avoid: bool,
    },
    /// Declared at v33 with the turning substrate (M5.2 stage A).
    /// Three heights, a radial infeed and two feeds - the whole
    /// authored shape of the facing ladder; the axis it rides is
    /// model data and needs no key. The automatic snake_case is
    /// already the tag.
    Facing {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// Declared at v34 with the profile kinds (M5.2 stage B): the
    /// station window's three heights, the radial rung spacing, the
    /// leave and two feeds - the whole authored shape of the rough
    /// ladder; the silhouette it eats is model data.
    ProfileRough {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        stock_to_leave: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// Declared at v34 (M5.2 stage B): the window and the feeds -
    /// one ride is the whole pattern.
    ProfileFinish {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
    },
    /// Declared at v35 with the blade kinds (M5.2 stage C): the
    /// window's three heights, the rung infeed, the leave and two
    /// feeds - the blade's width rides the TOOL row, not a number
    /// here; the kerf's width IS the tool.
    GrooveRough {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        stock_to_leave: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// Declared at v35 (M5.2 stage C): the window and the feeds -
    /// one plunge per center is the whole pattern.
    GrooveFinish {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
    },
    /// Declared at v35 (M5.2 stage C): the bar's top and the parting
    /// plane (`bottom_z`'s name in this variant), the rung infeed
    /// and the feeds. The feeder signals grew in at v40 (M5.2 G.2)
    /// with `#[serde(default)]` — an old file steps up silent-off,
    /// no version bump: the same shape every grown field takes. The
    /// handoff step grew in the same way (M5.3 C): absence is a
    /// program that asks no transfer. So did the probe budget
    /// (M5.3 D): absence is 0, silence.
    Part {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        #[serde(default)]
        bar_load: bool,
        #[serde(default)]
        bar_advance: f64,
        #[serde(default)]
        bar_unload: bool,
        #[serde(default)]
        bar_transfer: bool,
        #[serde(default)]
        inspect_tol: f64,
    },
    /// Declared at v36 with the threading kind (M5.2 stage D): the
    /// window of turns' three heights and the wave's own numbers -
    /// pitch, depth, ladder infeed, feeds. The insert's V rides the
    /// TOOL row, the blade's width law again.
    Threading {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        pitch: f64,
        depth: f64,
        infeed: f64,
        feed: f64,
        plunge_feed: f64,
        /// #416 the infeed plan: absent (and written only when not the
        /// default) is the radial constant-depth ladder every earlier
        /// file cut - no version step, the approach precedent.
        #[serde(default, skip_serializing_if = "ThreadInfeedDto::is_default")]
        infeed_plan: ThreadInfeedDto,
        /// #629: the thread's hand, always written. Absent (every file
        /// before the key) reads right-hand - the hand the drawing
        /// meant; those files posted left-hand by the old bug, and
        /// load as the right-hand thread they modeled.
        #[serde(default)]
        hand: ThreadHandDto,
    },
    /// Declared at v37 with the contour kinds (M5.2 stage E): the
    /// ride trio of heights and the kind's own promise-numbers -
    /// the authored angle, the authored diameters, the ladder
    /// numbers and the ride count. The nose that rides them rides
    /// the TOOL row, the profile kinds' law again.
    Chamfering {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        angle_deg: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// See [`Self::Chamfering`] - the taper's way.
    TaperTurning {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        bottom_dia: f64,
        top_dia: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// See [`Self::Chamfering`] - the multi-pass ladder's way.
    MultiPassTurning {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        stock_to_leave: f64,
        passes: u32,
        feed: f64,
        plunge_feed: f64,
    },
    /// See [`Self::Chamfering`] - the crown gauge's way: the window
    /// and the feeds, and no depth to file, because the model
    /// states it (M5.2 stage E2). The crown and the band ride the
    /// TOOL row - `nose_radius` and `width`, keys v37 files already
    /// carry.
    FormTurning {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
    },
    /// See [`Self::Chamfering`] - the boring bar's way (M5.2 stage
    /// F): the window, the tolerance and the two feeds, and no
    /// size to file, because the hole's wall is in the model and
    /// the nose that holds it to size is TOOL steel - `nose_radius`,
    /// a key v38 files already carry.
    BoringTurning {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
    },
    /// Standalone boring (M2.4 C). The optional heights ride the
    /// drill's absent-means-law shape; the hole list is the drill's
    /// `HoleDto` verbatim - recognition data, editable, stamped.
    Bore {
        /// M11.2 level transitions and links: absent is the kind's earlier shape.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        links: Option<LinksDto>,
        clearance_z: HeightDto,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bottom_z: Option<HeightDto>,
        stock_to_leave: f64,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        tolerance: f64,
        holes: Vec<HoleDto>,
    },
    /// Helical thread milling (M2.4 C). The pitch rides no DTO -
    /// it lives on the tool, like every other tool number.
    Thread {
        clearance_z: HeightDto,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bottom_z: Option<HeightDto>,
        infeed: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        holes: Vec<HoleDto>,
        /// #894, additive (no version step): the thread's pitch.
        /// Absent (every older file) is unstated - a multi-form
        /// mill's own pitch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pitch: Option<f64>,
        /// #894, additive: the thread's hand. Absent reads right-hand.
        #[serde(default)]
        hand: ThreadHandDto,
        /// #985, additive (no version step): external threads - the
        /// list holds bosses. Absent (every older file) is internal;
        /// written only when set.
        #[serde(default, skip_serializing_if = "is_false")]
        external: bool,
        /// #988, additive (no version step): pass control - schedule,
        /// finish and spring passes, allowance, spindle and cutter
        /// hand. Flattened keys, each absent when at the #894 default.
        #[serde(flatten)]
        passes: ThreadPassesDto,
    },
    /// The probe (#108): an additive variant in the v50 grammar.
    Probe(ProbeDto),
    /// The plasma profile (#670): additive - no older file names it.
    PlasmaProfile(PlasmaDto),
    /// The wire EDM contour (#532): additive - no older file names it.
    WireContour(WireContourDto),
    /// The trochoidal slot (#220): an appended variant in the v58 grammar,
    /// like the probe - no format rung (a file holding one cannot have
    /// been written by a build without it).
    TrochoidalSlot(TrochoidDto),
    /// The sinker burn (#541): an appended variant, like the probe - no
    /// older file names it.
    SinkerBurn(SinkerDto),
    /// The sinker side burn (#545): appended the same way.
    SinkerSideBurn(SinkerSideDto),
}

/// The angle an absent `ramp_angle` means: 45°, the ground the
/// raster's own law admits - every certifyable v17 chain already
/// descended no faster.
#[must_use]
pub fn default_ramp_angle() -> f64 {
    45.0
}

/// The file mirror of `finishing3d::FinishPattern`; `default` is
/// Waterline, the only pattern a v15 file can have run.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinishPatternDto {
    #[default]
    Waterline,
    Parallel,
    ParallelMulti,
    /// M2.2c: a new word in an existing grammar - v16 files never
    /// carried it and still read unchanged, so no version step-up.
    Scallop,
    /// M2.2d, same law: chords through the footprint center.
    Radial,
    /// M2.2d, same law: one Archimedean curve in from the rim.
    Spiral,
    /// M2.2e, same law: concentric shadows of the terrain, lifted.
    Contour,
    /// M2.2e, same law: the silhouette lifted onto the surface.
    Project,
    /// M2.5 A, same law: the auto-split as a border, each side of
    /// the 45° line with its own spacing number.
    SteepShallow,
    /// M2.5 A, same law: the floors only, one pass per shrink.
    Flat,
    /// M2.5 B, same law: the corner loci a reference ball leaves.
    Pencil,
    /// M2.5 B, same law: the two-face tangent pass, own ball.
    Corner,
    /// M2.5 C, same law: parallel with the sloped entries always on.
    Ramp,
    /// M2.5 C, same law: the spiral with the terrain's own spacing.
    MorphSpiral,
    /// M2.5 C, same law: rings of equal geodesic distance from the
    /// model's open boundary.
    Geodesic,
    /// M2.5 D, same law: both families ride the 30-45° transition,
    /// so the border's staircase is cut from both directions.
    Blend,
    /// M2.5 D, same law: the ladder's levels joined by ruled lofts.
    Morph,
    /// M2.5 E, same law: flowlines along the surface's grain, the
    /// 5-axis field without the axes.
    Flow,
}

impl From<FinishPatternDto> for crate::finishing3d::FinishPattern {
    fn from(v: FinishPatternDto) -> Self {
        match v {
            FinishPatternDto::Waterline => crate::finishing3d::FinishPattern::Waterline,
            FinishPatternDto::Parallel => crate::finishing3d::FinishPattern::Parallel,
            FinishPatternDto::ParallelMulti => crate::finishing3d::FinishPattern::ParallelMulti,
            FinishPatternDto::Scallop => crate::finishing3d::FinishPattern::Scallop,
            FinishPatternDto::Radial => crate::finishing3d::FinishPattern::Radial,
            FinishPatternDto::Spiral => crate::finishing3d::FinishPattern::Spiral,
            FinishPatternDto::Contour => crate::finishing3d::FinishPattern::Contour,
            FinishPatternDto::Project => crate::finishing3d::FinishPattern::Project,
            FinishPatternDto::SteepShallow => crate::finishing3d::FinishPattern::SteepShallow,
            FinishPatternDto::Flat => crate::finishing3d::FinishPattern::Flat,
            FinishPatternDto::Pencil => crate::finishing3d::FinishPattern::Pencil,
            FinishPatternDto::Corner => crate::finishing3d::FinishPattern::Corner,
            FinishPatternDto::Ramp => crate::finishing3d::FinishPattern::Ramp,
            FinishPatternDto::MorphSpiral => crate::finishing3d::FinishPattern::MorphSpiral,
            FinishPatternDto::Geodesic => crate::finishing3d::FinishPattern::Geodesic,
            FinishPatternDto::Blend => crate::finishing3d::FinishPattern::Blend,
            FinishPatternDto::Morph => crate::finishing3d::FinishPattern::Morph,
            FinishPatternDto::Flow => crate::finishing3d::FinishPattern::Flow,
        }
    }
}

impl From<crate::finishing3d::FinishPattern> for FinishPatternDto {
    fn from(v: crate::finishing3d::FinishPattern) -> Self {
        match v {
            crate::finishing3d::FinishPattern::Waterline => FinishPatternDto::Waterline,
            crate::finishing3d::FinishPattern::Parallel => FinishPatternDto::Parallel,
            crate::finishing3d::FinishPattern::ParallelMulti => FinishPatternDto::ParallelMulti,
            crate::finishing3d::FinishPattern::Scallop => FinishPatternDto::Scallop,
            crate::finishing3d::FinishPattern::Radial => FinishPatternDto::Radial,
            crate::finishing3d::FinishPattern::Spiral => FinishPatternDto::Spiral,
            crate::finishing3d::FinishPattern::Contour => FinishPatternDto::Contour,
            crate::finishing3d::FinishPattern::Project => FinishPatternDto::Project,
            crate::finishing3d::FinishPattern::SteepShallow => FinishPatternDto::SteepShallow,
            crate::finishing3d::FinishPattern::Flat => FinishPatternDto::Flat,
            crate::finishing3d::FinishPattern::Pencil => FinishPatternDto::Pencil,
            crate::finishing3d::FinishPattern::Corner => FinishPatternDto::Corner,
            crate::finishing3d::FinishPattern::Ramp => FinishPatternDto::Ramp,
            crate::finishing3d::FinishPattern::MorphSpiral => FinishPatternDto::MorphSpiral,
            crate::finishing3d::FinishPattern::Geodesic => FinishPatternDto::Geodesic,
            crate::finishing3d::FinishPattern::Blend => FinishPatternDto::Blend,
            crate::finishing3d::FinishPattern::Morph => FinishPatternDto::Morph,
            crate::finishing3d::FinishPattern::Flow => FinishPatternDto::Flow,
        }
    }
}

/// The file's adaptive words to the in-memory load (M11.4 B): the new
/// key wins, else the legacy degree word converted exactly (its old door
/// verdict kept - `load_of_legacy_degrees`), else the 45 deg default
/// every pre-v13 file meant, as a load.
pub(super) fn load_from_file(optimal_load: Option<f64>, legacy_deg: Option<f64>) -> f64 {
    match (optimal_load, legacy_deg) {
        (Some(l), _) => l,
        (None, Some(d)) => crate::load::load_of_legacy_degrees(d),
        (None, None) => crate::load::DEFAULT_OPTIMAL_LOAD,
    }
}

/// The seed's helix centerline radius, mm (M5.1 H gave the planar
/// pocket an `entry` field; its files predate the number).
fn default_helix_radius() -> f64 {
    1.0
}

/// Serde's skip test for an opt-in flag (M11.1 A `blend_turns`): a
/// `false` is absent from the file, so legacy saves stay byte-identical.
fn is_false(b: &bool) -> bool {
    !*b
}

/// Serde's skip test for an additive length that is zero by default
/// (#1731 slot end extensions): a zero is absent from the file.
fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

/// The facing hole law (M13.11): `lift` is the default and absent from
/// the file, so a legacy save stays byte-identical and reads as the
/// safe law; `cut_air` and `through` are the user's own choices.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FaceHolesDto {
    #[default]
    Lift,
    CutAir,
    Through,
}

impl FaceHolesDto {
    fn is_default(&self) -> bool {
        *self == FaceHolesDto::Lift
    }
}

impl From<FaceHoles> for FaceHolesDto {
    fn from(h: FaceHoles) -> Self {
        match h {
            FaceHoles::Lift => FaceHolesDto::Lift,
            FaceHoles::CutAir => FaceHolesDto::CutAir,
            FaceHoles::Through => FaceHolesDto::Through,
        }
    }
}

impl From<FaceHolesDto> for FaceHoles {
    fn from(h: FaceHolesDto) -> Self {
        match h {
            FaceHolesDto::Lift => FaceHoles::Lift,
            FaceHolesDto::CutAir => FaceHoles::CutAir,
            FaceHolesDto::Through => FaceHoles::Through,
        }
    }
}

/// The pre-M11.2 contour level-link word, still read and written (`links.rs`
/// migrates it in memory: stay_down_ramp = Keep down + Ramp); `default` is the
/// retract-and-re-enter transition every earlier contour posted.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LevelLinkDto {
    #[default]
    RetractReenter,
    StayDownRamp,
}

impl LevelLinkDto {
    fn is_default(&self) -> bool {
        *self == LevelLinkDto::RetractReenter
    }
}

/// An absent stay-down ramp angle: the generator's default.
fn default_level_ramp_angle() -> f64 {
    crate::machining2d::DEFAULT_LEVEL_RAMP_ANGLE
}

/// Serde's skip test for the ramp angle: the default stays out of the
/// file, so a contour that never touched it saves byte-identically.
fn is_default_level_ramp(a: &f64) -> bool {
    a.to_bits() == crate::machining2d::DEFAULT_LEVEL_RAMP_ANGLE.to_bits()
}

/// The file mirror of `machining3d::Entry3D`; `default` is Plunge, the
/// engagement every pre-3-D op had and the one no legality gate can
/// refuse.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryDto {
    #[default]
    Plunge,
    Ramp,
    Helix,
    /// The circular ramp (M11.1). Appended: the file word is new, so no
    /// earlier file can hold it and every earlier word reads as before.
    Arc,
}

/// One hole of a drill op's list, on the wire: world XY, the model's
/// hole radius, the entry surface and floor it was recognized between,
/// and `through` - stored, because it selects the exit allowance at
/// generation and a live re-query of the mesh would let a model edit
/// silently redefine a hole nobody re-detected.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct HoleDto {
    pub xy: [f64; 2],
    pub radius: f64,
    pub entry_z: f64,
    pub floor_z: f64,
    pub through: bool,
    /// Recognition's counterbore witness (M8.3). Additive: a file
    /// written before the flag reads `false` - the pre-M8.3 truth,
    /// since nothing older than the flag's first commit could carry a
    /// stepped reading. No version bump.
    #[serde(default)]
    pub stepped: bool,
}

impl HoleDto {
    pub(super) fn from_hole(h: &Hole) -> Self {
        HoleDto {
            // The lattice snap is idempotent, and recognition's output is
            // already snapped; this only levels a typed value onto the
            // file's fixed-point law (see MoveDto).
            xy: [lattice(h.xy.x), lattice(h.xy.y)],
            radius: lattice(h.radius),
            entry_z: lattice(h.entry_z),
            floor_z: lattice(h.floor_z),
            through: h.through,
            stepped: h.stepped,
        }
    }

    pub(super) fn to_hole(self) -> Hole {
        Hole {
            xy: Point2::new(self.xy[0], self.xy[1]),
            radius: self.radius,
            entry_z: self.entry_z,
            floor_z: self.floor_z,
            through: self.through,
            stepped: self.stepped,
        }
    }
}

/// One RADIAL hole of a drill op's cross list, on the wire (M5.2 F2):
/// the mouth's world point, the plunge direction (recognition only
/// emits axis-aligned unit dirs), the hole's own radius, and the two
/// travels along that direction - `entry_d` (the mouth convention:
/// 0 at the surface the hole opens on) and `floor_d` (negative =
/// depth). `through` selects the exit allowance at generation, the
/// same stored-not-requeried law [`HoleDto`] keeps.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CrossHoleDto {
    pub origin: [f64; 3],
    pub dir: [f64; 3],
    pub radius: f64,
    pub entry_d: f64,
    pub floor_d: f64,
    pub through: bool,
}

impl CrossHoleDto {
    pub(super) fn from_hole(h: &CrossHole) -> Self {
        CrossHoleDto {
            origin: [
                lattice(h.origin.x),
                lattice(h.origin.y),
                lattice(h.origin.z),
            ],
            dir: [h.dir.x, h.dir.y, h.dir.z],
            radius: lattice(h.radius),
            entry_d: lattice(h.entry_d),
            floor_d: lattice(h.floor_d),
            through: h.through,
        }
    }

    pub(super) fn to_hole(self) -> CrossHole {
        CrossHole {
            origin: Point3::new(self.origin[0], self.origin[1], self.origin[2]),
            dir: Point3::new(self.dir[0], self.dir[1], self.dir[2]),
            radius: self.radius,
            entry_d: self.entry_d,
            floor_d: self.floor_d,
            through: self.through,
        }
    }
}

/// Which canned cycle a drill op runs; the file mirror of
/// `drilling::CycleChoice`. `Auto` resolves at generate time (tool kind
/// first, then the depth-vs-diameter law), so the file stores only the
/// user's ask, like `StrategyDto::Auto` and `SideDto::Auto`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CycleChoiceDto {
    #[default]
    Auto,
    Spot,
    Drill,
    Peck,
    Tap,

    Bore,
    BackBore,
    FloatingTap,
    /// #1445: feed in, feed out (G85). Older files never carry it.
    Ream,
}

/// The reference part of a stored height. Serialized as a plain string;
/// `work_offset` is a one-key object. Before v52 it carried its own
/// `slot`; since #329 the slot is the setup's, so a v52 writer writes
/// `{"work_offset": {}}` and the loader lifts an older file's slots onto
/// their setups (`wcs_migrate`) or refuses by name - the key is read
/// only by that migration, never by the domain conversion below.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HeightRefDto {
    Fixed,
    TopOfStock,
    BottomOfStock,
    TopOfModel,
    BottomOfModel,
    WorkOffset {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        slot: Option<usize>,
    },
    /// The selection references (M11.3 A). Appended words: no earlier
    /// file can hold them and every earlier word reads as before, so
    /// the format version does not move (the EntryDto::Arc law).
    TopOfSelection,
    BottomOfSelection,
}

/// One stored height: `{"reference": ..., "offset": z}`. A bare number
/// still deserializes (`HeightWire`) as a `Fixed` expression carrying
/// exactly that number - the exact v5 value, so every pre-v6 file reads
/// unchanged with its saved z preserved to the bit. Writing always
/// produces the expression form.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
pub struct HeightDto {
    pub reference: HeightRefDto,
    pub offset: f64,
}

impl<'de> Deserialize<'de> for HeightDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        HeightWire::deserialize(deserializer).map(HeightDto::from)
    }
}

/// The two shapes a height takes on the wire. `Expr`'s fields are spelled
/// inline rather than nesting `HeightDto`, whose own `Deserialize` routes
/// back through here - the indirection must stay one-way, or parsing an
/// expression form recurses until the stack is gone.
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum HeightWire {
    Legacy(f64),
    Expr {
        reference: HeightRefDto,
        #[serde(default)]
        offset: f64,
    },
}

impl From<HeightWire> for HeightDto {
    fn from(v: HeightWire) -> Self {
        match v {
            HeightWire::Legacy(z) => HeightDto {
                reference: HeightRefDto::Fixed,
                offset: z,
            },
            HeightWire::Expr { reference, offset } => HeightDto { reference, offset },
        }
    }
}

impl From<crate::operation::Height> for HeightDto {
    fn from(h: crate::operation::Height) -> Self {
        HeightDto {
            reference: h.reference.into(),
            offset: h.offset,
        }
    }
}

impl From<HeightDto> for crate::operation::Height {
    fn from(d: HeightDto) -> Self {
        crate::operation::Height {
            reference: d.reference.into(),
            offset: d.offset,
        }
    }
}

impl From<crate::operation::HeightReference> for HeightRefDto {
    fn from(r: crate::operation::HeightReference) -> Self {
        use crate::operation::HeightReference as R;
        match r {
            R::Fixed => HeightRefDto::Fixed,
            R::TopOfStock => HeightRefDto::TopOfStock,
            R::BottomOfStock => HeightRefDto::BottomOfStock,
            R::TopOfModel => HeightRefDto::TopOfModel,
            R::BottomOfModel => HeightRefDto::BottomOfModel,
            R::WorkOffset => HeightRefDto::WorkOffset { slot: None },
            R::TopOfSelection => HeightRefDto::TopOfSelection,
            R::BottomOfSelection => HeightRefDto::BottomOfSelection,
        }
    }
}

impl From<HeightRefDto> for crate::operation::HeightReference {
    fn from(d: HeightRefDto) -> Self {
        use crate::operation::HeightReference as R;
        match d {
            HeightRefDto::Fixed => R::Fixed,
            HeightRefDto::TopOfStock => R::TopOfStock,
            HeightRefDto::BottomOfStock => R::BottomOfStock,
            HeightRefDto::TopOfModel => R::TopOfModel,
            HeightRefDto::BottomOfModel => R::BottomOfModel,
            HeightRefDto::WorkOffset { .. } => R::WorkOffset,
            HeightRefDto::TopOfSelection => R::TopOfSelection,
            HeightRefDto::BottomOfSelection => R::BottomOfSelection,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LeadDto {
    None,
    Linear {
        length: f64,
    },
    Arc {
        radius: f64,
    },
    /// Added at v9 with the tangential lead: older files carry no key
    /// change at all (`lead` has been required since v1), only this new
    /// tag can appear in newer ones.
    Tangential {
        radius: f64,
    },
    /// M11.3 C: the perpendicular lead - appended, so no earlier file
    /// holds the tag and an older build refuses it as unknown (Corrupt),
    /// never misreads it.
    Perpendicular {
        length: f64,
    },
}

/// The file mirror of `machining2d::AdaptiveMethod` (#1509); `default`
/// is the spiral, the only method a file written before the word could
/// mean.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdaptiveMethodDto {
    #[default]
    Spiral,
    Trochoidal,
}

/// The file mirror of 2D Adaptive Trochoidal's return choice (#1588);
/// `default` is `auto` (0.3 mm microlift when clearance room permits,
/// else retract).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdaptiveTrochReturnDto {
    #[default]
    Auto,
    Microlift,
    Retract,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StrategyDto {
    Zigzag,
    Waterline,
    /// Added at v5 with the auto strategy: the choice itself is not
    /// stored - resolution runs at generate time - only that the user
    /// asked for it.
    Auto,
    /// Added at v13 with the constant-engagement ring strategy (the 3-D
    /// pocket's `Adaptive`; in the 2-D pocket it is the documented
    /// waterline alias - the geometry family has no engagement field to
    /// feed it).
    Adaptive,
    /// Added at v49 (M11.4 C): the pre-C Adaptive ring tiling, kept
    /// as an explicit choice when `adaptive` became the HSM plan.
    OffsetRings,
}

/// Which side of its geometry a contour runs, on the wire. Added at v7
/// with the contour side choice; like `StrategyDto::Auto` the stored
/// value is the user's ask, not the resolution - an absent key reads as
/// `auto` and the selection layer decides at generate time.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SideDto {
    /// Resolved by the selection layer: a lone ring bounding air
    /// machines inside, everything else outside.
    #[default]
    Auto,
    Inside,
    Outside,
}
