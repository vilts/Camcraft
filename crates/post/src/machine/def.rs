//! The machine-definition types: what a `.mch` file states, as data.
//!
//! Provenance: our own reader for the machine-definition JSON layout
//! (field semantics ours, nothing from any vendor parser); the limit
//! shapes are general machine-tool practice. The unstated-limit law
//! (absent reads as "not stated", never as zero) lives with the types.
//!
//! Crate-boundary home (M4.1 R0): pure data - no flecs, no world. The
//! document glue and the program checker stay in the parent module.

use super::direction::AxisDirection;
// #669: the kind moved to its own seam; re-exported so `def::*` (and
// every `machine::Kind` path) reads as before.
pub use super::kind::{can_tilt, kind, Kind};
use serde::{Deserialize, Serialize};

/// One axis as the machine file describes it - prismatic or, since
/// M3.3 stage C, rotary (`type: "rotary"`): the same name/spaces
/// reading, with travel in degrees and rates in degrees per minute.
/// Nothing judges a rotary yet (no move in the stream turns a part),
/// but the kind inventory and the setup sheet need them stated.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AxisLimits {
    pub name: String,
    /// True for a rotary part (M3.3 C). Stated, not guessed from the
    /// letter: a linear axis named A is a slide, and a rotary named Q
    /// still turns.
    #[serde(default, skip_serializing_if = "is_false")]
    pub rotary: bool,
    /// G1 ceiling, mm/min. The feed a program asks for is checked
    /// against the SLOWEST moving axis of a coordinated move - an
    /// interpolation has to feed every axis it drives.
    pub feed: Option<f64>,
    /// G0 rate, mm/min. Programs never ask for a rapid rate, so this
    /// is carried for the setup sheet (stage E), not for a check.
    pub rapid: Option<f64>,
    /// Stroke in program coordinates, [min, max]. `None` = the file
    /// does not say (which is every stock vendor file - see the
    /// module's travel note). Rotary: the turning range, degrees.
    pub travel: Option<(f64, f64)>,
    /// Positioning resolution (stage C's "the numbers"): the smallest
    /// step the axis states it can hold, mm or degrees. A display
    /// and sheet number; nothing in a toolpath is judged against it -
    /// a move either reaches its coordinate or the file lied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<f64>,
    /// The rotary's axis of rotation in machine coordinates (M3.3
    /// stage D, RTCP): the part's stated `axis` vector, kept as
    /// written - a direction is a ratio, never scaled by the file's
    /// length units. A table trunnion says [1,0,0] (an A that tilts);
    /// a spindle-column C says [0,0,1] (a turn that never tilts).
    /// Absent = the file states no geometry, and no post may claim
    /// one for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<[f64; 3]>,
    /// Which way a linear axis moves for a positive command (#373), the
    /// file's `"direction": 1 | -1`; the program frame never changes.
    #[serde(default, skip_serializing_if = "AxisDirection::is_standard")]
    pub direction: AxisDirection,
    /// A rotary that turns the WORK, not the spindle (#804): the file
    /// places it outside the head's chain (no `head` part below it), as
    /// a 4th-axis table bolted to the bed is. Read by the index-swing
    /// clause of the program gate. `Some(false)` for a head's own tilt,
    /// `None` for every linear axis - and for a rotary read back from a
    /// project saved before #804, which never stated it (#813: the load
    /// names such a machine so it is reloaded, never silently unjudged).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turns_part: Option<bool>,
    /// How far the spindle nose sits above this rotary's centerline
    /// with Z at the post's `safe_retract`, mm (#804): the file's
    /// `"nose_to_axis"`. With WCS Y0 Z0 on the centerline (the indexed
    /// post's setup law) it puts the retracted tool tip in program Z.
    /// `None` = unstated, and an index on a work-turning A is refused
    /// by name rather than guessed clear.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nose_to_axis: Option<f64>,
    /// How far this work-turning rotary's centerline stands above the
    /// face it is bolted to, mm (#1107): the file's `"axis_to_table"`
    /// (its centre height). The index-swing clause judges the swept work
    /// against it - the table, plate or subplate the rotary stands on is
    /// this far under the line, whatever stack lifts the rotary. `None`
    /// = unstated: the work-vs-table swing is named not judged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis_to_table: Option<f64>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A coolant the machine can deliver, and the M-code that asks for
/// it (M3.3 C). Provenance: none/mist/flood/high-pressure/through-
/// spindle are the public option families of machine-tool coolant
/// practice; the CODE is whatever the machine file states - nothing
/// here invents one, and a name with no code posts nothing (the
/// posts' guards read the code, and silence is the honest output).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Coolant {
    pub name: String,
    /// The M word, `M8`-shaped. Empty means the file named the
    /// option without a code - visible on the setup sheet, silent
    /// in the program.
    #[serde(rename = "mCode", skip_serializing_if = "String::is_empty")]
    pub m_code: String,
}

/// The coolant names the shipped posts read (#150), in editor order:
/// `Flood` serves a Flood op, `Mist` a Mist op, `Air` an Air op (#869;
/// `Air blast` reads too), `Through-spindle` or
/// `Through-tool` a Through-tool op. The posts compare case-blind with
/// spaces and underscores read as hyphens ("Through spindle" reads); any
/// other name is a shop's own flood (the first coded one serves).
pub const COOLANT_KINDS: [&str; 5] = ["Flood", "Mist", "Air", "Through-spindle", "Through-tool"];

/// A machine, as parsed from a `.mch` file. The identity strings are
/// display data; every limit is optional by the unstated-limits law.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Machine {
    pub model: String,
    pub vendor: String,
    pub spindle_min: Option<f64>,
    pub spindle_max: Option<f64>,
    /// In X, Y, Z order (the file's `coordinate` index decides);
    /// empty when the file names no linear axes.
    pub axes: Vec<AxisLimits>,
    pub max_tool_length: Option<f64>,
    pub max_tool_diameter: Option<f64>,
    /// The spindle BODY (M3.2 stage B): diameter, and how far the
    /// housing stands proud above the tool holder, mm. Not the speed
    /// range above - the geometry the rotating house occupies, for the
    /// collision band that runs past the holder. Unstated by the
    /// unstated-limit law (files that name nothing check nothing);
    /// when both are stated the band is a cylinder of `spindle_d`
    /// rising `spindle_len` above the holder top.
    pub spindle_d: Option<f64>,
    pub spindle_len: Option<f64>,
    /// The declared kind word (M3.3 stage C): the file's own answer to
    /// "what is this machine". `None` = the file says nothing, and the
    /// axis inventory decides (M3.2 stage E's law, kept as the
    /// fallback). The WORD is stored, not the enum, so a project file
    /// round-trips exactly what was written and `is_sane` is the
    /// single place that decides what is statable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Spindle motor power, kW (stage C's numbers). Since M13.2 the
    /// machine check judges each op's estimated cutting power (the
    /// document material's Kienzle constants over the op's removal
    /// rate, `app::cutting`) against it; unstated checks nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spindle_power: Option<f64>,
    /// Tool changer capacity, slots (stage C): `tooling`'s
    /// `number_of_tools`. A program asking for more distinct tools
    /// than the drum holds is a limit violation the checker names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_count: Option<usize>,
    /// The coolant the machine states it can deliver (stage C) - the
    /// set the posts' `machine.coolants` reads, so a mounted `.mch`
    /// speaks its own M-codes instead of the default's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coolants: Vec<Coolant>,
    /// How many spindles the file states (M5.3 C): the count of
    /// `type: "head"` parts carrying a spindle. The limit numbers
    /// still read the first head (the first-match law); this count
    /// answers a different question - whether a program's handoff
    /// step (`PART TRANSFER 1 2`) has a second spindle to talk
    /// about at all. Unstated (no spindle-carrying head anywhere)
    /// reads `None`, and unstated checks nothing: an empty machine
    /// claims no inventory of steel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spindles: Option<usize>,
    /// The length unit the file states (#131): `Some("inch")` when its
    /// kinematics are in inches, `None` for mm (the default). Every
    /// number above is mm whatever this says - the parser scales on the
    /// way in - so this only tells a post which unit the machine is
    /// programmed in (`machine.units`, G20 vs G21).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    /// The spindle's speed ranges, rpm, `(low, high)` each (#170): the
    /// belt or gear positions a machine changes by hand. A post names
    /// the range a speed needs when only one serves it. Empty = unstated
    /// (one range, or the file says nothing): no range is named.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spindle_ranges: Vec<(f64, f64)>,
    /// Whether the spindle can rigid-tap (#170): `Some(false)` when the
    /// file says it cannot, and a post refuses a rigid tap cycle by name.
    /// Unstated reads as "not limited", the unstated-limit law.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rigid_tapping: Option<bool>,
    /// The builder's codes for C-axis live-tool work (issue #33), or
    /// None - unstated, and a post refuses a C-axis section by name
    /// rather than guess a builder-specific M word.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_tool: Option<LiveToolCodes>,
    /// The Z a post retracts to in MACHINE coordinates (`G53 G0 Z..`)
    /// before a tool change and at program end (#495), mm: the Z axis
    /// part's `safe_retract`. A stated 0 is a statement (machine Z0 is
    /// the top of travel, Tormach's convention), so this is the one
    /// number where zero is not "unstated". `None` = unstated, and a
    /// post that needs it refuses rather than guess where up is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_retract_z: Option<f64>,
    /// The `edm` section of a wire or sinker EDM machine (#530), or
    /// None: unstated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edm: Option<super::edm::Edm>,
    /// The EDM control's own words, `edm.codes` (#533, #290 D5): name ->
    /// the text the control reads. Empty = none stated; a post that
    /// needs one refuses by its name.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub edm_codes: std::collections::BTreeMap<String, String>,
    /// The holder system the spindle takes (#828): `tooling.spindle_mount`,
    /// one of [`super::SPINDLE_MOUNTS`]. Unstated reads `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spindle_mount: Option<String>,
    /// The built-in library holder a NEW tool on this machine starts
    /// with (#828): `tooling.default_holder`, the library's own name
    /// ("BT30 ER20"). Never applied to an existing tool; `None` adds a
    /// tool with no holder body, as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_holder: Option<String>,
}

/// The words a control needs around C-axis live-tool work (issue
/// #33): engage the spindle as a positioned C axis and release it,
/// start the live tool (the post appends `S<rpm>`) and stop it. They
/// are BUILDER-specific - no public standard fixes them - so they are
/// the machine file's statement (`tool_station.live_tool`), never a
/// post's default. All four or none: a partial set reads as unstated.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveToolCodes {
    pub c_axis_on: String,
    pub c_axis_off: String,
    pub spindle_on: String,
    pub spindle_off: String,
}

impl Machine {
    /// Is every stated limit statable: finite rates, and strokes that
    /// run somewhere. The project reader gates on this BEFORE writing
    /// the world - a hand-edited machine that means nothing is a
    /// corrupt file, refused, never a checker that fails backwards.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.tooling_refusal(None).is_none() && self.limits_sane()
    }

    /// #828: why the stored `tooling` keys cannot be meant, as
    /// `"key: rule"` (a default holder that no longer resolves in
    /// `lookup`, the caller's holder library; one off the stated mount),
    /// so a refusal names the key, not "a limit". With no lookup only
    /// the words are judged ([`Self::is_sane`]); every app door that
    /// stores a machine passes its library.
    #[must_use]
    pub fn tooling_refusal(&self, lookup: Option<super::HolderLookup>) -> Option<String> {
        super::mount_check(self.spindle_mount.as_deref(), self.default_holder.as_deref(), lookup).err()
    }

    fn limits_sane(&self) -> bool {
        let rate_ok = |r: Option<f64>| r.is_none_or(|x| x.is_finite() && x > 0.0);
        rate_ok(self.spindle_min)
            && rate_ok(self.spindle_max)
            && rate_ok(self.spindle_power)
            && rate_ok(self.max_tool_length)
            && rate_ok(self.max_tool_diameter)
            && rate_ok(self.spindle_d)
            && rate_ok(self.spindle_len)
            && self
                .kind
                .as_deref()
                .is_none_or(|w| Kind::from_word(w).is_some())
            && self.coolants.iter().all(|c| !c.name.is_empty())
            && self.plasma_sane()
            && self.edm_sane()
            && self.axes.iter().all(|a| {
                rate_ok(a.feed) && rate_ok(a.rapid) && rate_ok(a.resolution) && a.travel.is_none_or(|(lo, hi)| lo.is_finite() && hi.is_finite() && hi > lo)
                    && a.axis.is_none_or(|d| d.iter().all(|x| x.is_finite()))
                    && rate_ok(a.nose_to_axis)
                    && rate_ok(a.axis_to_table)
            })
            && match (self.spindle_min, self.spindle_max) {
                (Some(lo), Some(hi)) => hi >= lo,
                _ => true,
            }
            && self.edm.as_ref().is_none_or(super::edm::Edm::is_sane)
            && super::edm_codes::codes_sane(&self.edm_codes)
    }

    /// The one-line identity for the Setup pane and the stage E sheet.
    /// The declared kind leads when the file states one (M3.3 C) -
    /// the word the ribbon's families follow, so a name that promises
    /// "turning" cannot be a surprise three lines down.
    #[must_use]
    pub fn summary(&self) -> String {
        let n = self.axes.iter().filter(|a| a.travel.is_some()).count();
        let lead = self
            .kind
            .as_deref()
            .and_then(Kind::from_word)
            .map_or(String::new(), |k| format!("{} machine - ", k.label()));
        // #669: a plasma table has a torch, not a spindle - "spindle ?-?"
        // would read as an unstated range rather than an absent spindle.
        let head = if kind(Some(self)) == Kind::Plasma {
            "torch, no spindle".to_owned()
        } else {
            format!(
                "spindle {}-{} rpm",
                self.spindle_min.map_or_else(|| "?".into(), |x| format!("{x:.0}")),
                self.spindle_max.map_or_else(|| "?".into(), |x| format!("{x:.0}")),
            )
        };
        let mut s = format!(
            "{lead}{} {} - {head}, {} axes ({} with a known stroke)",
            self.vendor,
            self.model,
            self.axes.len(),
            n,
        );
        if let Some(p) = self.spindle_power {
            s.push_str(&format!(", {p:.1} kW"));
        }
        if let Some(n) = self.tool_count {
            s.push_str(&format!(", {n}-slot changer"));
        }
        if !self.coolants.is_empty() {
            let list: Vec<&str> = self.coolants.iter().map(|c| c.name.as_str()).collect();
            s.push_str(&format!(", coolant {}", list.join("/")));
        }
        s
    }

    /// The limits of the coordinate `k` (0=X, 1=Y, 2=Z), found BY NAME.
    /// Positional indexing would silently check the wrong stroke when a
    /// file names a subset of axes or leads with a U - the table is
    /// ordered for display, names are what make a limit mean something.
    pub fn axis(&self, k: usize) -> Option<&AxisLimits> {
        let n = ["X", "Y", "Z"].get(k)?.to_ascii_uppercase();
        self.axes
            .iter()
            .find(|a| a.name.to_ascii_uppercase() == n)
    }

    /// The feed ceiling of a coordinated move: the slowest stated
    /// ceiling among the axes the move actually drives. `None` when
    /// no driven axis states one - nothing to violate.
    pub fn feed_ceiling(&self, d: [f64; 3]) -> Option<f64> {
        let mut best: Option<f64> = None;
        for (k, moved) in d.map(|x| x.abs() > 0.0).iter().enumerate() {
            if !*moved {
                continue;
            }
            if let Some(f) = self.axis(k).and_then(|a| a.feed) {
                best = Some(best.map_or(f, |b: f64| b.min(f)));
            }
        }
        best
    }
}

