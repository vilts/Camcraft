//! A tool's catalog facts (#898): part number, vendor, coating, helix
//! angle and notes. Provenance: our own data model; the fields are the
//! ordinary columns of a public cutting-tool catalog listing. Pure
//! bookkeeping: no generator, collision check or post reads them, so
//! they never move a toolpath.

/// Longest part number, vendor or coating, in characters.
pub const MAX_SHORT: usize = 80;
/// Longest note, in characters.
pub const MAX_NOTES: usize = 500;

/// Optional catalog facts of one tool. `None` = not stated; a stated
/// text is trimmed and non-empty (see [`ToolCatalog::normalized`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolCatalog {
    pub part_number: Option<String>,
    pub vendor: Option<String>,
    pub coating: Option<String>,
    /// Flute helix angle in degrees, 0..=90.
    pub helix_deg: Option<f64>,
    pub notes: Option<String>,
}

fn clean(s: &Option<String>) -> Option<String> {
    s.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

impl ToolCatalog {
    /// Nothing stated.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == ToolCatalog::default()
    }

    /// The same facts trimmed, blank text read as not stated.
    #[must_use]
    pub fn normalized(&self) -> ToolCatalog {
        ToolCatalog {
            part_number: clean(&self.part_number),
            vendor: clean(&self.vendor),
            coating: clean(&self.coating),
            helix_deg: self.helix_deg,
            notes: clean(&self.notes),
        }
    }

    /// The refusal text for facts that cannot be stored, or `None`
    /// when they are fine. Checked on the facts as stated: a text
    /// that is not trimmed is refused here, so callers normalize first.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        let texts = [
            ("part number", &self.part_number, MAX_SHORT),
            ("vendor", &self.vendor, MAX_SHORT),
            ("coating", &self.coating, MAX_SHORT),
            ("notes", &self.notes, MAX_NOTES),
        ];
        for (what, v, max) in texts {
            let Some(s) = v else { continue };
            if s.trim() != s || s.is_empty() {
                return Some(format!("{what} must be trimmed, non-blank text"));
            }
            if s.chars().count() > max {
                return Some(format!("{what} is longer than {max} characters"));
            }
            // A note may span lines; the short fields may not.
            if s.chars().any(|c| c.is_control() && !(what == "notes" && c == '\n')) {
                return Some(format!("{what} holds a control character"));
            }
        }
        if let Some(h) = self.helix_deg
            && !(h.is_finite() && (0.0..=90.0).contains(&h))
        {
            return Some(format!("helix angle must be 0 to 90 degrees (it states {h})"));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_refuses() {
        let c = ToolCatalog {
            part_number: Some("  A-1 ".into()),
            vendor: Some("   ".into()),
            helix_deg: Some(35.0),
            ..ToolCatalog::default()
        };
        let n = c.normalized();
        assert_eq!(n.part_number.as_deref(), Some("A-1"));
        assert_eq!(n.vendor, None);
        assert_eq!(n.refusal(), None);
        assert!(c.refusal().is_some(), "untrimmed text is refused");
        for h in [-1.0, 90.5, f64::NAN, f64::INFINITY] {
            assert!(ToolCatalog { helix_deg: Some(h), ..n.clone() }.refusal().is_some(), "{h}");
        }
        let long = Some("x".repeat(MAX_SHORT + 1));
        assert!(ToolCatalog { coating: long, ..n.clone() }.refusal().is_some());
        assert!(ToolCatalog { vendor: Some("a\tb".into()), ..n.clone() }.refusal().is_some());
        assert_eq!(ToolCatalog { notes: Some("a\nb".into()), ..n }.refusal(), None);
    }
}
