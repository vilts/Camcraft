//! #2018: one Weldon pick for each taper ("a weldon is a weldon"). The
//! picker lists "BT40 Weldon" once; the bore is the library's own
//! standard bore for the tool's shank (DIN 1835 form B / DIN 6359, the
//! inch bores, the TTS set-screw bores), and with it that bore's body
//! diameter. The data keeps one entry for each bore: a real side-lock
//! holder has a fixed bore, and its body diameter is a collision shape,
//! so a grouped pick always resolves to an existing entry and draws
//! exactly its outline. Our own lookup over the library's tables.

use super::{find, library, Clamp, HolderEntry, LibraryError, Taper};

/// One row of the editor's holder picker: a library entry as it is, or
/// one Weldon row standing for every Weldon bore of a taper.
#[derive(Clone, Debug, PartialEq)]
pub enum PickerRow {
    Entry(HolderEntry),
    Weldon { taper: Taper, bores: Vec<f64> },
}

impl PickerRow {
    /// The name the picker lists and [`resolve`] takes back: an entry's
    /// own name, or "BT40 Weldon".
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::Entry(e) => e.name(),
            Self::Weldon { taper, .. } => weldon_name(taper),
        }
    }
}

/// The grouped Weldon pick's name for `taper`: "BT40 Weldon".
#[must_use]
pub fn weldon_name(taper: &Taper) -> String {
    format!("{} Weldon", taper.name)
}

/// The holder picker's rows, in the library's order: every ER chuck as
/// it is, and each taper's Weldon bores as one row where the first of
/// them stands.
#[must_use]
pub fn picker_rows() -> Vec<PickerRow> {
    let mut rows: Vec<PickerRow> = Vec::new();
    for e in library() {
        match (e.taper, e.clamp) {
            (Some(t), Clamp::Weldon { bore, .. }) => {
                let row = rows.iter_mut().find_map(|r| match r {
                    PickerRow::Weldon { taper, bores } if taper.name == t.name => Some(bores),
                    _ => None,
                });
                match row {
                    Some(bores) => bores.push(bore),
                    None => rows.push(PickerRow::Weldon { taper: t, bores: vec![bore] }),
                }
            }
            _ => rows.push(PickerRow::Entry(e)),
        }
    }
    rows
}

/// The `taper`'s Weldon entry whose bore takes a shank of `shank_d`.
/// A shank no standard bore of the taper takes is refused by name with
/// the fit check's own text, listing the bores there are.
pub fn weldon_for(taper: &str, shank_d: f64) -> Result<HolderEntry, LibraryError> {
    let weldons: Vec<HolderEntry> = library()
        .into_iter()
        .filter(|e| matches!(e.clamp, Clamp::Weldon { .. }) && e.taper.is_some_and(|t| t.name.eq_ignore_ascii_case(taper)))
        .collect();
    if weldons.is_empty() {
        return Err(LibraryError::Unknown(format!("{taper} Weldon")));
    }
    if let Some(e) = weldons.iter().find(|e| e.fits_shank(shank_d).is_ok()) {
        return Ok(*e);
    }
    let bores: Vec<String> = weldons
        .iter()
        .filter_map(|e| match e.clamp {
            Clamp::Weldon { bore, .. } => Some(format!("Ø{bore:.3}")),
            Clamp::Er(_) => None,
        })
        .collect();
    let name = weldons.first().and_then(|e| e.taper).map_or_else(|| taper.to_owned(), |t| t.name.to_owned());
    Err(LibraryError::Shank { shank_d, takes: format!("a {name} Weldon bore: {} mm", bores.join(", ")) })
}

/// The library entry a pick names, for a tool whose shank is `shank_d`:
/// a grouped Weldon pick ("BT40 Weldon", case and spacing ignored) is
/// the taper's bore for that shank; any other name is [`find`]'s entry.
pub fn resolve(name: &str, shank_d: f64) -> Result<HolderEntry, LibraryError> {
    let words: Vec<&str> = name.split_whitespace().collect();
    match words.as_slice() {
        [taper, w] if w.eq_ignore_ascii_case("weldon") => weldon_for(taper, shank_d),
        _ => find(name),
    }
}

#[cfg(test)]
#[path = "holder_library_weldon_tests.rs"]
mod tests;
