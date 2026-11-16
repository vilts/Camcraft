//! The remaining stock a rest Adaptive pass is planned on (issue 2031):
//! what earlier operations are PROVED to have left, floor by floor.
//! Provenance: the in-process workpiece of 2.5D pocketing as a stack of
//! planar regions (M. Held, "On the Computational Geometry of Pocket
//! Machining", LNCS 500, Springer 1991; Z. Yao, S. K. Gupta and D. S.
//! Nau, "Algorithms for selecting cutters in multi-part milling
//! problems", Computer-Aided Design 35(9), 2003, for what a larger
//! cutter leaves a smaller one). The proof contract - what a floor
//! claims, what an unproved depth or an uncertain area means - is this
//! project's decision; no other CAM product's code or output was
//! consulted.
//!
//! Prime Directive: the stock is read large. Where a floor cannot say
//! whether material stands (`uncertain`), it stands. A depth no floor
//! reaches is not planned on a guess: it is refused by name.

use crate::geo2d::Region;
use crate::machining2d::GenError;

/// Height slack: a floor counts as reaching a cut depth a nanometre
/// above it (the level ladder's own snap).
const Z_EPS: f64 = 1e-9;

/// One proved floor of the stock the earlier operations left.
///
/// The claim: inside the pocket, AT AND ABOVE `z`, material stands only
/// inside `remaining` or `uncertain`; everywhere else the column from
/// the top of the stock down to `z` is proved empty. Below `z` the
/// floor says nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct StockFloor {
    /// The depth the earlier operations are proved to have cut to.
    pub z: f64,
    /// Where material is known (or bounded) to stand at or above `z`.
    pub remaining: Region,
    /// Where the proof could not decide. Read as material: never
    /// entered, never crossed by a rapid, cut under the load bound.
    pub uncertain: Region,
}

/// The remaining stock of the earlier operations, as proved floors.
///
/// There is deliberately no way to build one from an entered tool
/// diameter: a diameter typed by the user is no proof that material is
/// absent (issue 2031). Whoever builds it derives the floors from the
/// verified stock of the operations that ran before.
#[derive(Clone, Debug, PartialEq)]
pub struct RestStock {
    floors: Vec<StockFloor>,
    film: f64,
}

/// Residue thinner than this (mm) is not planned for unless the stock
/// says otherwise ([`RestStock::with_film`]): `rest2d`'s film, the band
/// a stock proof read small leaves along every wall the earlier tool
/// rode.
pub const REST_STOCK_FILM: f64 = 0.02;

/// The planner's guards, each on in every build. The unit tests switch
/// one off at a time to show that its pinning test then fails (issue
/// 2031: "remove each stock, depth, and entry guard separately").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Guards {
    /// Uncertain stock stands as material.
    pub stock: bool,
    /// A cut depth below every proved floor is refused.
    pub depth: bool,
    /// A plunge needs the whole footprint proved clear by the stock.
    pub entry: bool,
}

impl Guards {
    pub(super) const ALL: Guards = Guards { stock: true, depth: true, entry: true };
}

impl RestStock {
    /// The stock from floors proved by the earlier operations' verified
    /// stock. Refused by name: no floor at all, or a floor at a depth
    /// that is not finite.
    pub fn from_verified(floors: Vec<StockFloor>) -> Result<Self, GenError> {
        if floors.is_empty() {
            return Err(GenError::Invalid(
                "rest Adaptive needs the remaining stock of the earlier operations: no proved floor was given".into(),
            ));
        }
        if floors.iter().any(|f| !f.z.is_finite()) {
            return Err(GenError::Invalid("the remaining stock has a floor at a depth that is not finite".into()));
        }
        Ok(RestStock { floors, film: REST_STOCK_FILM })
    }

    /// The stock with its proof's resolution stated: residue thinner than
    /// `film` mm is what a bound read large leaves along the walls, and
    /// no pass is planned to cut it. It still STANDS for every safety
    /// reading - no plunge lands on it, and the load of a pass riding it
    /// counts it. Refused by name: a film that is negative or not finite.
    pub fn with_film(mut self, film: f64) -> Result<Self, GenError> {
        if !(film.is_finite() && film >= 0.0) {
            return Err(GenError::Invalid("the remaining stock's film must be a length, zero or more".into()));
        }
        self.film = film;
        Ok(self)
    }

    /// Residue thinner than this is not planned for ([`Self::with_film`]).
    #[must_use]
    pub fn film(&self) -> f64 {
        self.film
    }

    /// The proved floors, as given.
    #[must_use]
    pub fn floors(&self) -> &[StockFloor] {
        &self.floors
    }

    /// The deepest depth any floor is proved to.
    #[must_use]
    pub fn deepest(&self) -> f64 {
        self.floors.iter().map(|f| f.z).fold(f64::INFINITY, f64::min)
    }

    /// A bound on the material standing at or above the cut depth `z`:
    /// the intersection of what every floor proved at least that deep
    /// allows (each is a bound, so their intersection is one). A depth
    /// no floor reaches is refused by name - the depth guard.
    pub(super) fn material_at(&self, z: f64, guards: Guards) -> Result<Region, GenError> {
        let mut speaking: Vec<usize> = (0..self.floors.len()).filter(|&i| self.floors[i].z <= z + Z_EPS).collect();
        if speaking.is_empty() {
            if guards.depth {
                return Err(GenError::Invalid(format!(
                    "the remaining stock is not proved at Z{z:.3}: the earlier operations are proved down to Z{:.3} only, and rest Adaptive does not enter below a proved floor",
                    self.deepest()
                )));
            }
            speaking = (0..self.floors.len()).collect();
        }
        let mut bound: Option<Region> = None;
        for i in speaking {
            let f = &self.floors[i];
            // The stock guard: where the proof could not decide, material stands.
            let m = if guards.stock { f.remaining.union(&f.uncertain)? } else { f.remaining.clone() };
            bound = Some(match bound {
                Some(b) => b.intersect(&m)?,
                None => m,
            });
        }
        Ok(bound.unwrap_or_default())
    }
}
