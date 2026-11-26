//! The starter catalog as data (#741): `assets/tools/catalog.json`, a
//! `camcraft-tools/1` library (`{format, version, units, tools: [{name,
//! definition}]}`) embedded with `include_str!` so the binary needs no
//! file beside it, and the strict reader that turns it into named tools.
//!
//! Provenance: our own tool-library envelope (#300) and the same
//! definition keys a `.camproj` stores; the values are ordinary metric
//! end-mill / drill / insert sizes from public machine-tool handbooks.
//! The app's `parse_tool_library` reads the same file (a test there
//! pins that both readers agree) - this reader exists because the
//! catalog lives below `app` in the crate layering.

use serde::Deserialize;

use crate::tools::{Tool, ToolKind, ToolMaterial};

/// The envelope tag and version, as `app::tool_library` writes them.
const FORMAT: &str = "camcraft-tools";
const VERSION: u64 = 1;

/// The shipped asset's text.
pub const CATALOG_JSON: &str = include_str!("../../../assets/tools/catalog.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    version: u64,
    #[serde(default)]
    units: Option<String>,
    tools: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    name: String,
    definition: serde_json::Value,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum KindWord {
    EndMill,
    BallNose,
    BullNose,
    Drill,
    Tap,
    Bore,
    Turning,
    Groove,
    Thread,
    Form,
    Taper,
    Probe,
    Torch,
    Waterjet,
    Electrode,
    ThreadMill,
    ThreadMillSingle,
    Wire,
    TSlot,
    Keyseat,
    Dovetail,
    Lollipop,
    FormMill,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum MaterialWord {
    Hss,
    Carbide,
}

/// A tool definition as the file writes it (the `ToolDto` keys).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Def {
    kind: KindWord,
    diameter: f64,
    corner_r: f64,
    #[serde(default)]
    tip_angle: f64,
    #[serde(default)]
    pitch: f64,
    /// A single-form thread mill's finest pitch (#894). #3662: absent on
    /// a single-form mill reads as its one pitch.
    #[serde(default)]
    pitch_min: Option<f64>,
    /// Tap thread hand; older tools default to right-hand.
    #[serde(default)]
    thread_hand: crate::thread_hand::ThreadHand,
    #[serde(default)]
    nose_radius: f64,
    #[serde(default)]
    width: f64,
    flute_len: f64,
    shank_d: f64,
    shank_len: f64,
    holder_d: f64,
    holder_len: f64,
    material_group: String,
    rpm: f64,
    feed: f64,
    #[serde(default)]
    tool_material: Option<MaterialWord>,
    #[serde(default)]
    flutes: Option<u32>,
    #[serde(default = "yes")]
    center_cutting: bool,
    /// Lathe tool orientation (LinuxCNC 1-9); the starter catalog states none.
    #[serde(default)]
    orientation: Option<u8>,
    /// Turning cutting data (#891); the starter catalog states none.
    #[serde(default)]
    turning: Option<crate::turning_data::TurningData>,
    /// A sinker electrode's section (#1152); the starter catalog states none.
    #[serde(default)]
    outline: Vec<[f64; 2]>,
    /// #3368: a chamfer mill's tip form and tip diameter; absent is sharp.
    #[serde(default)]
    tip_form: crate::taper::TaperTip,
    #[serde(default)]
    tip_diameter: f64,
    /// #3369: a form mill's outline; a catalogue entry states its preset.
    #[serde(default)]
    form: Option<crate::form_mill::FormSpec>,
}

fn yes() -> bool {
    true
}

impl Def {
    fn into_tool(self) -> Tool {
        Tool {
            kind: match self.kind {
                KindWord::EndMill => ToolKind::EndMill,
                KindWord::BallNose => ToolKind::BallNose,
                KindWord::BullNose => ToolKind::BullNose,
                KindWord::Drill => ToolKind::Drill,
                KindWord::Tap => ToolKind::Tap,
                KindWord::Bore => ToolKind::Bore,
                KindWord::Turning => ToolKind::Turning,
                KindWord::Groove => ToolKind::Groove,
                KindWord::Thread => ToolKind::Thread,
                KindWord::Form => ToolKind::Form,
                KindWord::Taper => ToolKind::Taper,
                KindWord::Probe => ToolKind::Probe,
                KindWord::Torch => ToolKind::Torch,
                KindWord::Waterjet => ToolKind::Waterjet,
                KindWord::Electrode => ToolKind::Electrode,
                KindWord::ThreadMill => ToolKind::ThreadMill,
                KindWord::ThreadMillSingle => ToolKind::ThreadMillSingle,
                KindWord::Wire => ToolKind::Wire,
                KindWord::TSlot => ToolKind::TSlot,
                KindWord::Keyseat => ToolKind::Keyseat,
                KindWord::Dovetail => ToolKind::Dovetail,
                KindWord::Lollipop => ToolKind::Lollipop,
                KindWord::FormMill => ToolKind::FormMill,
            },
            diameter: self.diameter,
            corner_r: self.corner_r,
            tip_angle: self.tip_angle,
            pitch: self.pitch,
            pitch_min: self.pitch_min.unwrap_or(0.0),
            thread_hand: self.thread_hand,
            nose_radius: self.nose_radius,
            width: self.width,
            flute_len: self.flute_len,
            shank_d: self.shank_d,
            shank_len: self.shank_len,
            holder_d: self.holder_d,
            holder_len: self.holder_len,
            material_group: self.material_group,
            rpm: self.rpm,
            feed: self.feed,
            tool_material: self.tool_material.map(|m| match m {
                MaterialWord::Hss => ToolMaterial::Hss,
                MaterialWord::Carbide => ToolMaterial::Carbide,
            }),
            flutes: self.flutes,
            center_cutting: self.center_cutting,
            orientation: self.orientation,
            catalog: crate::catalog_fields::ToolCatalog::default(),
            cutting: Vec::new(),
            outline: self.outline,
            turning: self.turning,
            tip_form: self.tip_form,
            tip_diameter: self.tip_diameter,
            form: self.form,
        }
    }
}

/// Read a catalog file's text, strictly: bad JSON, a wrong format tag,
/// version or units, an empty or repeated name, an unknown or missing
/// definition key, or a tool `Tool::is_sane` refuses each refuse the whole
/// file, naming the tool. Order is kept - it is the display order.
pub fn parse_catalog(text: &str) -> Result<Vec<(String, Tool)>, String> {
    let env: Envelope = serde_json::from_str(text).map_err(|e| format!("tool catalog: bad JSON ({e})"))?;
    if env.format != FORMAT {
        return Err(format!("tool catalog: format is '{}', not '{FORMAT}'", env.format));
    }
    if env.version != VERSION {
        return Err(format!("tool catalog: version {} is not supported (this build reads {VERSION})", env.version));
    }
    if let Some(u) = env.units.as_deref()
        && u != "mm"
    {
        return Err(format!("tool catalog: units '{u}' are not supported (lengths are mm)"));
    }
    let mut out: Vec<(String, Tool)> = Vec::with_capacity(env.tools.len());
    for (i, row) in env.tools.into_iter().enumerate() {
        let name = row.name.trim().to_owned();
        if name.is_empty() {
            return Err(format!("tool catalog: tool #{} has no name", i.saturating_add(1)));
        }
        if out.iter().any(|(n, _)| *n == name) {
            return Err(format!("tool catalog: tool '{name}' appears twice"));
        }
        let def: Def = serde_json::from_value(row.definition).map_err(|e| format!("tool catalog: tool '{name}': {e}"))?;
        let unstated_range = def.pitch_min.is_none();
        let mut tool = def.into_tool();
        // #3369: a preset form writes its own points.
        crate::form_mill::fill(&mut tool);
        // #3662: a single-form thread mill with no range cuts its one pitch.
        if unstated_range {
            crate::thread_mill::read_unstated_range(&mut tool);
        }
        if let Some(why) = tool.why_not_sane() {
            return Err(format!("tool catalog: tool '{name}' is refused: {why}"));
        }
        out.push((name, tool));
    }
    if out.is_empty() {
        return Err("tool catalog: holds no tools".into());
    }
    Ok(out)
}

/// The parsed shipped catalog, once. A shipped asset that fails to parse
/// is a build bug a test pins; at runtime it is logged and the catalog
/// is empty (callers already treat a missing name as absent) - never a
/// panic.
pub(crate) fn shipped() -> &'static [(String, Tool)] {
    static CATALOG: std::sync::OnceLock<Vec<(String, Tool)>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| match parse_catalog(CATALOG_JSON) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("starter tool catalog unavailable: {e}");
            Vec::new()
        }
    })
}
