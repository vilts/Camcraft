//! The cutting fluid an operation asks for, as the stream carries it
//! (issue #112). Provenance: the coolant families of public machine-tool
//! handbooks (flood, mist, through-the-tool delivery) and the RS-274 /
//! ISO 6983 miscellaneous-function convention (M7 mist, M8 flood, M9
//! off); our own names and stream spelling, no vendor material.
//!
//! The stream law: flood is what a section states by saying nothing -
//! every stream written before the field existed, and every legacy
//! `.cl`, posts exactly as it always did. Any other ask rides one
//! `PARAM Coolant <word>` ahead of the section; the post looks the
//! word's code up in the mounted machine's coolant list (the machine
//! is truth) and refuses by name when the machine states none.
//!
//! #869 adds `Air` (an air blast, M7 on a Makera Carvera - not a fluid
//! but a chip-clearing blow; the coolant families of public
//! machine-tool practice list it beside mist) and the machine's default
//! coolant: the mode a new operation starts on, read from the mounted
//! machine's coded coolant list.

use serde::{Deserialize, Serialize};

/// One operation's coolant choice. `Flood` is the default: it is what
/// every post emitted before the choice existed.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum CoolantMode {
    /// No cutting fluid: the section switches nothing on.
    Off,
    /// Flood coolant - the machine's `Flood` entry (M8 on most controls).
    #[default]
    Flood,
    /// Mist (air-carried fluid) - the machine's `Mist` entry (M7 on most).
    Mist,
    /// An air blast (#869) - the machine's `Air` / `Air-blast` entry
    /// (M7 on a Makera Carvera); only machines that state one can post it.
    Air,
    /// Fluid through the tool's own bore - the machine's
    /// `Through-spindle` / `Through-tool` entry; only machines that state
    /// one can post it.
    ThroughTool,
}

impl CoolantMode {
    /// Every mode, in picker order.
    pub const ALL: [CoolantMode; 5] = [
        CoolantMode::Off,
        CoolantMode::Flood,
        CoolantMode::Mist,
        CoolantMode::Air,
        CoolantMode::ThroughTool,
    ];

    /// The picker label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            CoolantMode::Off => "Off",
            CoolantMode::Flood => "Flood",
            CoolantMode::Mist => "Mist",
            CoolantMode::Air => "Air",
            CoolantMode::ThroughTool => "Through tool",
        }
    }

    /// The `PARAM Coolant` word the stream carries, or `None` for flood
    /// (the stream's silent default - see the module law).
    #[must_use]
    pub fn param_word(self) -> Option<&'static str> {
        match self {
            CoolantMode::Off => Some("off"),
            CoolantMode::Flood => None,
            CoolantMode::Mist => Some("mist"),
            CoolantMode::Air => Some("air"),
            CoolantMode::ThroughTool => Some("through-tool"),
        }
    }

    /// Whether this is the default (serde's skip test: an untouched
    /// operation writes no key, so older files round-trip unchanged).
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == CoolantMode::Flood
    }

    /// The mode a machine's coolant NAME serves, read as the posts read
    /// it: case-blind, spaces and underscores as hyphens. `None` for a
    /// shop's own name (the posts take its first coded one as flood).
    #[must_use]
    pub fn of_machine_name(name: &str) -> Option<CoolantMode> {
        let n = name.trim().to_ascii_lowercase().replace([' ', '_'], "-");
        match n.as_str() {
            "flood" => Some(CoolantMode::Flood),
            "mist" => Some(CoolantMode::Mist),
            "air" | "air-blast" => Some(CoolantMode::Air),
            "through-spindle" | "through-tool" => Some(CoolantMode::ThroughTool),
            _ => None,
        }
    }

    /// The machine's default coolant (#869): the mode a new operation
    /// starts on. Flood where the machine can post a flood ask - a coded
    /// `Flood` entry, a shop's own coded name, or no coded entry at all
    /// (no machine, an unstated list: every op before #869); otherwise
    /// the first coded entry's mode, so a machine whose only coolant is
    /// an `Air` M7 starts its operations on Air, never on a Flood its
    /// post would refuse.
    #[must_use]
    pub fn machine_default(coolants: &[crate::machine::Coolant]) -> CoolantMode {
        let coded: Vec<Option<CoolantMode>> = coolants
            .iter()
            .filter(|c| !c.m_code.trim().is_empty())
            .map(|c| CoolantMode::of_machine_name(&c.name))
            .collect();
        if coded.iter().any(|m| matches!(m, None | Some(CoolantMode::Flood))) {
            return CoolantMode::Flood;
        }
        coded.into_iter().flatten().next().unwrap_or(CoolantMode::Flood)
    }
}
