//! The words of the display modes (#2818, #2636): the menu label, the tooltip
//! and the key that the view settings file stores for each mode.
//! Provenance: our own wording for the display modes that the issue lists;
//! no vendor text.

use crate::state::DisplayMode;

impl DisplayMode {
    /// The modes in menu order.
    pub const ALL: [DisplayMode; 5] = [
        DisplayMode::ShadedEdges,
        DisplayMode::Shaded,
        DisplayMode::Wireframe,
        DisplayMode::WireframeSeeThrough,
        DisplayMode::ShadedHiddenEdges,
    ];

    /// The menu label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            DisplayMode::ShadedEdges => "Shaded, with edges",
            DisplayMode::Shaded => "Shaded",
            DisplayMode::Wireframe => "Wireframe",
            DisplayMode::ShadedHiddenEdges => "Shaded, with hidden edges",
            DisplayMode::WireframeSeeThrough => "Wireframe, see-through",
        }
    }

    /// What the mode does, for the tooltip.
    #[must_use]
    pub const fn tooltip(self) -> &'static str {
        match self {
            DisplayMode::ShadedEdges => "Solid faces with the edges drawn on them.",
            DisplayMode::Shaded => "Solid faces only. The edges are not drawn.",
            DisplayMode::Wireframe => "Edges only. The edges behind the model are not drawn.",
            DisplayMode::ShadedHiddenEdges => {
                "Solid faces with the edges drawn on them. The edges behind the model show in a lighter tone."
            }
            DisplayMode::WireframeSeeThrough => {
                "Edges only, front and back. The model hides nothing behind it. A click picks hidden edges too."
            }
        }
    }

    /// The key in the view settings file.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            DisplayMode::ShadedEdges => "shaded-edges",
            DisplayMode::Shaded => "shaded",
            DisplayMode::Wireframe => "wireframe",
            DisplayMode::ShadedHiddenEdges => "shaded-hidden-edges",
            DisplayMode::WireframeSeeThrough => "wireframe-see-through",
        }
    }

    /// True in the two modes that draw edges and no lit faces.
    #[must_use]
    pub const fn is_wireframe(self) -> bool {
        matches!(self, DisplayMode::Wireframe | DisplayMode::WireframeSeeThrough)
    }

    /// True in the mode that shows hidden edges with no face in front
    /// (#2636). A click then picks them: nearest to the cursor first, then
    /// nearest to the camera (`camcraft_app::pick::pick_bodies`).
    #[must_use]
    pub const fn sees_through(self) -> bool {
        matches!(self, DisplayMode::WireframeSeeThrough)
    }

    /// The mode of a stored key. An unknown key gives the default mode, so a
    /// file from a newer build still loads.
    #[must_use]
    pub fn from_key(key: &str) -> DisplayMode {
        DisplayMode::ALL.into_iter().find(|m| m.key() == key).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use crate::state::DisplayMode;

    #[test]
    fn each_mode_has_its_own_label_tooltip_and_key() {
        for (i, a) in DisplayMode::ALL.into_iter().enumerate() {
            assert!(!a.tooltip().is_empty());
            assert_eq!(DisplayMode::from_key(a.key()), a, "the key reads back");
            for b in DisplayMode::ALL.into_iter().skip(i + 1) {
                assert_ne!(a.label(), b.label());
                assert_ne!(a.key(), b.key());
            }
        }
        assert_eq!(DisplayMode::from_key("no such mode"), DisplayMode::ShadedEdges);
        assert_eq!(DisplayMode::from_key(""), DisplayMode::default());
    }
}
