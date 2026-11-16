//! How 2D Adaptive clears its region (issue #1509): the HSM spiral it
//! always cut, or trochoidal loops. The choice is ours; trochoidal
//! milling as a constant-engagement roughing strategy is Otkur & Lazoglu
//! 2007, "Trochoidal milling", Int. J. Mach. Tools Manuf. 47, and Rauch,
//! Duc & Hascoet 2009, IJMTM 49 (the planner is `adaptive_troch`).

/// The clearing method of a 2D Adaptive operation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AdaptiveMethod {
    /// The HSM plan (`pocket_hsm`): a spiral out of the entry bore, then
    /// engagement fronts. Every 2D Adaptive made before the choice
    /// existed, byte for byte.
    #[default]
    Spiral,
    /// Trochoidal loops along the region's medial line
    /// (`adaptive_troch`), confined to the region.
    Trochoidal,
}

impl AdaptiveMethod {
    /// Every method, in the order the page lists them.
    pub const ALL: [AdaptiveMethod; 2] = [AdaptiveMethod::Spiral, AdaptiveMethod::Trochoidal];

    /// The name the page and the notes use.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            AdaptiveMethod::Spiral => "Spiral",
            AdaptiveMethod::Trochoidal => "Trochoidal",
        }
    }
}
