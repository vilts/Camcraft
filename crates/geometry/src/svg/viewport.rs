//! The root `<svg>` viewport: its physical size, the `viewBox` mapping and the Y flip.
//! Provenance: the W3C SVG 1.1 coordinate-systems chapter (`viewBox` and
//! `preserveAspectRatio`, user units as CSS px without a viewBox) and the CSS absolute-length
//! table (96 px to the inch); original implementation.

use super::number::{LenUnit, length, list};
use super::transform::Affine;
use super::xml::Node;
use super::{SvgError, invalid};

/// Where user space goes.
pub(super) struct Viewport {
    /// User space to output space: millimetres when `physical`, else viewport units. Y is up.
    pub map: Affine,
    /// The file states a physical size.
    pub physical: bool,
}

fn dimension(node: &Node, name: &str) -> Result<Option<(f64, LenUnit)>, SvgError> {
    let Some(text) = node.attr(name) else { return Ok(None) };
    let (v, unit) = length(text, &format!("<svg> {name}"))?;
    if unit == LenUnit::Relative {
        return Ok(None); // "100%": no size of its own
    }
    if v <= 0.0 {
        return Err(invalid(format!("<svg> {name} \"{text}\" is a zero or negative size")));
    }
    Ok(Some((v, unit)))
}

/// The `preserveAspectRatio` placement of a `vw` x `vh` viewBox at (`vx`, `vy`) in a
/// `w` x `h` viewport. A value that is not in the SVG grammar is refused: a guess of its
/// alignment moves the drawing.
fn fit(node: &Node, [vx, vy, vw, vh]: [f64; 4], w: f64, h: f64) -> Result<Affine, SvgError> {
    let par = node.attr("preserveAspectRatio").unwrap_or("xMidYMid meet");
    let bad = || {
        let short: String = par.chars().take(40).collect();
        invalid(format!("<svg> preserveAspectRatio \"{short}\" is not a known value"))
    };
    let mut words = par.split_whitespace().peekable();
    if words.peek() == Some(&"defer") {
        words.next();
    }
    let align = words.next().ok_or_else(bad)?;
    let slice = match words.next() {
        None | Some("meet") => false,
        Some("slice") => true,
        Some(_) => return Err(bad()),
    };
    if words.next().is_some() {
        return Err(bad());
    }
    let (sx, sy) = (w / vw, h / vh);
    if align == "none" {
        return Ok(Affine::scale(sx, sy).then(Affine::translate(-vx, -vy)));
    }
    // The share of the free space before the viewBox: `xMinYMid` is (0, 1/2).
    let share = |word: &str| match word {
        "Min" => Some(0.0),
        "Mid" => Some(0.5),
        "Max" => Some(1.0),
        _ => None,
    };
    let (fx, fy) = align
        .strip_prefix('x')
        .and_then(|rest| rest.split_once('Y'))
        .and_then(|(x, y)| Some((share(x)?, share(y)?)))
        .ok_or_else(bad)?;
    let s = if slice { sx.max(sy) } else { sx.min(sy) };
    let (tx, ty) = (fx * (w - vw * s), fy * (h - vh * s));
    Ok(Affine::translate(tx, ty).then(Affine::scale(s, s)).then(Affine::translate(-vx, -vy)))
}

/// Read the root element's size and viewBox.
pub(super) fn read(root: &Node) -> Result<Viewport, SvgError> {
    // SVG 2 permits `transform` on the root `<svg>`; refuse it rather than drop it silently.
    if root.attr("transform").is_some() {
        return Err(invalid("transform attribute on the root <svg> is not supported"));
    }
    let view_box = match root.attr("viewBox") {
        None => None,
        Some(text) => {
            let v = list(text, "<svg> viewBox")?;
            match *v.as_slice() {
                [x, y, w, h] if w > 0.0 && h > 0.0 => Some([x, y, w, h]),
                [_, _, _, _] => return Err(invalid(format!("<svg> viewBox \"{text}\" has a zero or negative size"))),
                _ => return Err(invalid(format!("<svg> viewBox \"{text}\" must be four numbers"))),
            }
        }
    };
    let (width, height) = (dimension(root, "width")?, dimension(root, "height")?);
    let stated = |d: Option<(f64, LenUnit)>| d.is_some_and(|(_, u)| u != LenUnit::None);
    let physical = stated(width) || stated(height);
    // A unitless number beside a stated unit is CSS px; alone it is no physical size.
    let value = |d: Option<(f64, LenUnit)>| {
        d.map(|(v, u)| if physical { v * u.mm().unwrap_or(25.4 / 96.0) } else { v })
    };
    let (w, h) = match (value(width), value(height), view_box) {
        (Some(w), Some(h), _) => (Some(w), Some(h)),
        (Some(w), None, Some([.., vw, vh])) => (Some(w), Some(w * vh / vw)),
        (None, Some(h), Some([.., vw, vh])) => (Some(h * vw / vh), Some(h)),
        (Some(_), None, None) => return Err(invalid("<svg> states a width but no height and no viewBox")),
        (None, Some(_), None) => return Err(invalid("<svg> states a height but no width and no viewBox")),
        (None, None, _) => (None, None),
    };
    // User space to the viewport, Y still down.
    let (to_viewport, page_height) = match (view_box, w, h) {
        (Some(vb), Some(w), Some(h)) => (fit(root, vb, w, h)?, Some(h)),
        (Some([vx, vy, _, vh]), _, _) => (Affine::translate(-vx, -vy), Some(vh)),
        // No viewBox: a user unit is one CSS px.
        (None, _, h) if physical => (Affine::scale(25.4 / 96.0, 25.4 / 96.0), h),
        (None, _, h) => (Affine::IDENTITY, h),
    };
    // Y up, the page's bottom edge on y = 0 (or the user-space origin when the page has no
    // height).
    let flip = Affine { d: -1.0, f: page_height.unwrap_or(0.0), ..Affine::IDENTITY };
    Ok(Viewport { map: flip.then(to_viewport), physical })
}
