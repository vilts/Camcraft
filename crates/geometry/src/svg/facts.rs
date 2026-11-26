//! What the walk needs from one element's `style` and `transform`, read one time per element,
//! and the allow-list of the style properties that cannot change geometry (the inline `style`
//! and the `<style>` sheets use the same list).
//! Provenance: the W3C SVG 1.1 styling chapter (the property index), the SVG 2 geometry
//! chapter (`x`, `y`, `width`, `height`, `cx`, `cy`, `r`, `rx`, `ry` and `d` are CSS
//! properties there), CSS Transforms 1 and 2 and CSS Motion Path 1; original implementation.

use super::style::inline_declarations;
use super::transform::{self, Affine};
use super::xml::Node;
use super::{SvgError, invalid};

/// The elements whose geometry, or whose children's geometry, is imported. The inline style
/// of any other element is not checked: that element is skipped whole and counted.
const IMPORTED: &[&str] = &["g", "a", "use", "path", "rect", "circle", "ellipse", "line", "polyline", "polygon"];

/// Presentation attributes that move geometry and are not applied (CSS Transforms 1).
const UNSUPPORTED_TRANSFORM_ATTRS: &[&str] = &["transform-origin", "transform-box"];

/// The inline style properties that cannot change the geometry of an imported element: paint,
/// text and rendering hints, and the four properties that the walk handles by name (`display`,
/// `visibility`, `clip-path`, `mask`). Each other property is refused by name, so a property
/// that this reader does not know (a size, a transform, a vendor prefix of one) stops the
/// import; it is never ignored. Relative lengths are refused, so a font size scales nothing.
const SAFE_PROPERTIES: &[&str] = &[
    "display", "visibility", "clip-path", "mask",
    "fill", "fill-opacity", "fill-rule", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin",
    "stroke-miterlimit", "stroke-dasharray", "stroke-dashoffset", "stroke-opacity", "opacity", "color",
    "paint-order", "stop-color", "stop-opacity", "clip-rule", "mix-blend-mode", "isolation",
    "marker", "marker-start", "marker-mid", "marker-end", "-inkscape-stroke",
    "shape-rendering", "text-rendering", "image-rendering", "color-rendering", "color-interpolation",
    "color-interpolation-filters", "enable-background", "overflow", "cursor", "pointer-events",
    "font", "font-family", "font-size", "font-style", "font-weight", "font-variant", "font-stretch",
    "font-size-adjust", "font-variant-ligatures", "font-variant-caps", "font-variant-numeric",
    "font-variant-east-asian", "font-variant-position", "font-variant-alternates", "font-feature-settings",
    "font-variation-settings", "font-kerning", "-inkscape-font-specification", "text-anchor", "text-align",
    "text-decoration", "text-decoration-line", "text-decoration-style", "text-decoration-color",
    "text-indent", "text-transform", "text-orientation", "letter-spacing", "word-spacing", "line-height",
    "writing-mode", "direction", "unicode-bidi", "white-space", "dominant-baseline", "alignment-baseline",
    "baseline-shift",
];

/// Properties that are safe only with one of these values (the others can move geometry).
const SAFE_VALUES: &[(&str, &[&str])] = &[("vector-effect", &["none", "non-scaling-stroke"])];

/// The start of a name or value for a message: a hostile name can be megabytes long.
fn short(text: &str) -> String {
    text.chars().take(40).collect()
}

/// A marker property with a value that draws a marker shape. The shape is not imported.
pub(super) fn draws_marker(name: &str, value: &str) -> bool {
    matches!(name, "marker" | "marker-start" | "marker-mid" | "marker-end")
        && !value.trim().eq_ignore_ascii_case("none")
}

/// The refusal of property `name` (lower case) with `value`, or `None` when it is safe.
/// `source` names where the declaration is: `inline CSS` or `style sheet`.
pub(super) fn refused(source: &str, name: &str, value: &str) -> Option<SvgError> {
    if SAFE_PROPERTIES.contains(&name) {
        return None;
    }
    let hint = "a style can hold only paint and text properties";
    match SAFE_VALUES.iter().find(|(n, _)| *n == name) {
        Some((_, values)) if values.iter().any(|v| value.trim().eq_ignore_ascii_case(v)) => None,
        Some(_) => Some(invalid(format!("{source} {name}: {} is not supported", short(value)))),
        None => Some(invalid(format!("{source} {} is not supported: {hint}", short(name)))),
    }
}

/// The facts of one element. The walk reads them one time and keeps them by node index, so a
/// `use` that draws the element again does not parse its style or its transform again.
#[derive(Clone)]
pub(super) struct Facts {
    /// A `visibility` attribute or declaration: the subtree is skipped and counted.
    pub visibility: bool,
    /// `display: none`.
    pub hidden: bool,
    /// A transform source or an inline property that is not applied: the import is refused.
    pub refusal: Option<SvgError>,
    /// The `transform` attribute.
    pub transform: Result<Option<Affine>, SvgError>,
    pub clip_path: bool,
    pub mask: bool,
    /// A marker property that draws a marker: the marker shape is skipped and counted.
    pub marker: bool,
    /// The bytes of the attributes that each visit reads again (all but `style` and
    /// `transform`): the charge of one visit against the work budget.
    pub reread: usize,
}

impl Facts {
    pub fn read(node: &Node, root: bool) -> Facts {
        let imported = root || IMPORTED.contains(&node.name.as_str());
        let mut facts = Facts {
            visibility: node.attr("visibility").is_some(),
            hidden: node.attr("display").is_some_and(|d| d.trim().eq_ignore_ascii_case("none")),
            refusal: UNSUPPORTED_TRANSFORM_ATTRS
                .iter()
                .find(|a| node.attr(a).is_some())
                .map(|attr| invalid(format!("{attr} presentation attribute is not supported"))),
            transform: node.attr("transform").map(transform::parse).transpose(),
            clip_path: node.attr("clip-path").is_some(),
            mask: node.attr("mask").is_some(),
            marker: node.attrs.iter().any(|(name, value)| draws_marker(name, value)),
            reread: node
                .attrs
                .iter()
                .filter(|(name, _)| !matches!(name.as_str(), "style" | "transform"))
                .fold(0usize, |sum, (name, value)| sum.saturating_add(name.len()).saturating_add(value.len())),
        };
        for decl in node.attr("style").map(inline_declarations).unwrap_or_default() {
            // A browser drops a declaration with a bad `url(` token and reads the next one.
            // A style is not read in part: the import is refused.
            if decl.bad_url {
                if imported && facts.refusal.is_none() {
                    facts.refusal = Some(invalid("inline CSS has a bad url( token: a style is not read in part"));
                }
                continue;
            }
            // A name that is empty or has a space in it is not a CSS property: a browser drops
            // that declaration, and so does this reader.
            if decl.property.is_empty() || decl.property.contains(' ') {
                continue;
            }
            // CSS property names and keywords are case-insensitive.
            let name = decl.property.to_ascii_lowercase();
            match name.as_str() {
                "visibility" => facts.visibility = true,
                // An importance suffix must never make a hidden contour eligible for cutting.
                "display" => {
                    facts.hidden |= decl.value.eq_ignore_ascii_case("none")
                        && (!decl.has_bang || decl.importance.eq_ignore_ascii_case("important"));
                }
                "clip-path" => facts.clip_path = true,
                "mask" => facts.mask = true,
                _ => facts.marker |= draws_marker(&name, &decl.value),
            }
            if imported && facts.refusal.is_none() {
                facts.refusal = refused("inline CSS", &name, &decl.value);
            }
        }
        facts
    }
}
