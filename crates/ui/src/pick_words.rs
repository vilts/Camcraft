//! The words a pick shows on the Setups page (#738): a count and a kind -
//! "1 face selected", "2 points selected", "1 edge selected" - never the
//! welded feature's internal id ("face 0xccc7c..."), which means nothing
//! to the person at the machine. [`plain`] takes such an id out of a
//! refusal the app words for its own callers, so a reason shown on the
//! page reads the same way.
//!
//! Provenance: our own UI wording; no vendor material consulted.

use camcraft_app::operation::{FeatureSel, GeoSel};

/// "1 face selected", "2 points selected", "1 face and 1 edge selected";
/// "nothing selected" for none. Plurals follow the count.
#[must_use]
pub fn selected(picks: &[GeoSel]) -> String {
    let mut counts = [0usize; 3]; // faces, edges, points
    for g in picks {
        let k = match g.feature {
            FeatureSel::Face(_) | FeatureSel::Patch(_) => 0,
            FeatureSel::Chain(_) => 1,
            FeatureSel::Vertex(_) => 2,
        };
        if let Some(c) = counts.get_mut(k) {
            *c = c.saturating_add(1);
        }
    }
    let parts: Vec<String> = ["face", "edge", "point"]
        .into_iter()
        .zip(counts)
        .filter(|(_, n)| *n > 0)
        .map(|(word, n)| count(n, word))
        .collect();
    if parts.is_empty() { "nothing selected".to_owned() } else { format!("{} selected", parts.join(" and ")) }
}

/// "1 point", "3 points".
#[must_use]
pub fn count(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

/// The kind of one pick in plain words, with its article.
#[must_use]
pub fn kind(f: FeatureSel) -> &'static str {
    match f {
        FeatureSel::Face(_) => "a face",
        FeatureSel::Patch(_) => "a curved face",
        FeatureSel::Chain(_) => "an edge",
        FeatureSel::Vertex(_) => "a point",
    }
}

/// `msg` with every internal feature id taken out: "face 0x1a2b of
/// 'block' is degenerate" reads "face of 'block' is degenerate".
#[must_use]
pub fn plain(msg: &str) -> String {
    let mut out = String::with_capacity(msg.len());
    let mut rest = msg;
    while let Some(i) = rest.find(" 0x") {
        let (head, tail) = rest.split_at(i);
        out.push_str(head);
        let digits = tail.get(3..).unwrap_or("");
        let n = digits.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(digits.len());
        if n == 0 {
            out.push_str(" 0x"); // not an id: keep it as written
        }
        rest = digits.get(n..).unwrap_or("");
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(f: FeatureSel) -> GeoSel {
        GeoSel { entity: "block".into(), feature: f }
    }

    #[test]
    fn counts_read_with_plurals_and_no_ids() {
        assert_eq!(selected(&[]), "nothing selected");
        assert_eq!(selected(&[g(FeatureSel::Face(0xccc7c))]), "1 face selected");
        assert_eq!(selected(&[g(FeatureSel::Vertex(1)), g(FeatureSel::Vertex(2))]), "2 points selected");
        assert_eq!(selected(&[g(FeatureSel::Chain(7))]), "1 edge selected");
        assert_eq!(selected(&[g(FeatureSel::Face(1)), g(FeatureSel::Chain(2))]), "1 face and 1 edge selected");
        for p in [FeatureSel::Face(0xccc7c), FeatureSel::Patch(0xab), FeatureSel::Chain(0xff), FeatureSel::Vertex(0x10)] {
            assert!(!selected(&[g(p)]).contains("0x"));
            assert!(!kind(p).contains("0x"));
        }
    }

    #[test]
    fn plain_takes_the_ids_out_of_a_reason() {
        let why = format!("{} of 'block' is degenerate - pick another face", FeatureSel::Face(0xccc7c));
        assert_eq!(plain(&why), "face of 'block' is degenerate - pick another face");
        assert_eq!(plain("nothing to strip"), "nothing to strip");
        assert_eq!(plain("x 0xg"), "x 0xg");
        assert_eq!(plain("a 0x1 b 0xFF"), "a b");
    }
}
