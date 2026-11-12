//! The light-viewport inks (#698): on a light ground the viewport's
//! overlay, wire and palette inks flip to darker sets that clear 3:1
//! (WCAG 1.4.11) against the ground, while the dark set stays the house
//! look byte for byte. The dark values are the consts they already were
//! (interact.rs, boundary_viz.rs, colorvision's EDGE_WIRE); the light
//! values are ours, pinned by `viewport_ground_tests`.
//! Provenance: our own palette, specced from the WCAG 2.x contrast
//! definition `contrast` implements; no HSMWorks material consulted.

use egui::Color32;

use super::colorvision::{EDGE_WIRE, Semantic};
use super::{contrast, theme};

/// The luminance past which a ground counts as light (mid-grey sits at
/// 0.18; dark's ink is 0.02, light's 0.85).
const LIGHT_GROUND: f64 = 0.18;

/// Whether a ground reads light.
#[must_use]
pub fn light_ground(ground: Color32) -> bool {
    contrast::luma(ground) > LIGHT_GROUND
}

/// The wireframe line on a light viewport: the dark ground's cool gray
/// vanishes on paper, so the light ground gets a dark ink (8:1 on the
/// light viewport, pinned).
const EDGE_WIRE_LIGHT: [f32; 3] = [0.20, 0.23, 0.28];

/// The wireframe edge ink for a ground (viewport `fs_edge`'s `gray`).
#[must_use]
pub const fn wire_ink(light: bool) -> [f32; 3] {
    if light { EDGE_WIRE_LIGHT } else { EDGE_WIRE }
}

impl Semantic {
    /// The house palette on a light viewport: the move kinds and the
    /// boundary roles as tuned on screen (#698). Verdict and selection
    /// inks are unchanged: they paint the stock and the selected part,
    /// not lines on the ground.
    pub const OFF_LIGHT_GROUND: Semantic = Semantic {
        feed: [0.08, 0.36, 0.12],
        rapid: [0.01, 0.31, 0.90],
        plunge: [0.90, 0.13, 0.13],
        bound_include: [0.10, 0.80, 1.00],
        bound_exclude: [1.00, 0.20, 0.70],
        bound_open: [1.00, 0.65, 0.05],
        ..Semantic::OFF
    };

    /// Protan/deutan on a light viewport: the same blue-yellow axis as
    /// `RED_GREEN`, darkened to clear 3:1 - gold feed and exclude against
    /// the blue rapid and include; plunge and open go near-black, the
    /// lightness end the dark ground's white open and pale inks held.
    pub const RED_GREEN_LIGHT_GROUND: Semantic = Semantic {
        feed: [0.55, 0.42, 0.00],
        rapid: [0.01, 0.31, 0.90],
        plunge: [0.22, 0.08, 0.00],
        bound_include: [0.00, 0.30, 0.85],
        bound_exclude: [0.55, 0.42, 0.00],
        bound_open: [0.12, 0.12, 0.12],
        ..Semantic::RED_GREEN
    };

    /// Tritan on a light viewport: red-cyan survives - the deep green
    /// feed, red plunge and exclude, a teal include; rapid and open go
    /// near-black so lightness parts them from the green.
    pub const TRITAN_LIGHT_GROUND: Semantic = Semantic {
        feed: [0.08, 0.36, 0.12],
        rapid: [0.05, 0.05, 0.12],
        plunge: [0.90, 0.13, 0.13],
        bound_include: [0.00, 0.40, 0.45],
        bound_exclude: [0.80, 0.05, 0.10],
        bound_open: [0.12, 0.12, 0.12],
        ..Semantic::TRITAN
    };
}

/// The viewport overlay inks that were tuned against the dark ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    Region,
    Retract,
    Cut,
    Pick,
    Hover,
}

/// An overlay ink on a ground: the dark consts as they were, or the
/// light set (each clears 3:1 on the light viewport; the test pins it).
#[must_use]
pub fn overlay_ink(o: Overlay, light: bool) -> [f32; 3] {
    match (o, light) {
        (Overlay::Region, false) => crate::interact::REGION_RGB,
        (Overlay::Region, true) => [0.70, 0.35, 0.00],
        (Overlay::Retract, false) => crate::interact::RETRACT_RGB,
        (Overlay::Retract, true) => [0.05, 0.40, 0.75],
        (Overlay::Cut, false) => crate::interact::CUT_RGB,
        (Overlay::Cut, true) => [0.70, 0.12, 0.10],
        (Overlay::Pick, false) => crate::interact::PICK_RGB,
        (Overlay::Pick, true) => [0.45, 0.15, 0.62],
        (Overlay::Hover, false) => crate::boundary_viz::HOVER_RGB,
        (Overlay::Hover, true) => [0.70, 0.40, 0.00],
    }
}

/// The overlay ink for the live theme's viewport ground.
#[must_use]
pub fn overlay(o: Overlay) -> [f32; 3] {
    overlay_ink(o, light_ground(theme::current().viewport))
}

/// The viewport furniture's greys: the view cube, its nav buttons, the
/// corner hint and the playback tool marker. On the dark ground they
/// are the values the cube always drew; on a light ground the faces are
/// a grey a step darker than the viewport, the edges darker still, and
/// the labels and button glyphs dark ink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubeInk {
    /// Face grey at the darkest head-light angle, and its span.
    pub face_base: f32,
    pub face_span: f32,
    /// Added to a hovered face (or button) grey.
    pub hover_lift: u8,
    /// Face and button outline.
    pub edge: Color32,
    /// Face label.
    pub label: Color32,
    /// Button fill, and whether the button wears the edge outline.
    pub button: u8,
    pub outline_buttons: bool,
    /// Button glyph when its toggle is on, and when off.
    pub glyph_on: Color32,
    pub glyph_off: Color32,
    /// The viewport's corner control hint.
    pub hint: Color32,
    /// The tool-position marker (stem, ring, centre dot) of playback.
    pub marker: [Color32; 3],
    /// The Axes button's X, Y, Z triad.
    pub triad: [Color32; 3],
}

/// The cube greys for a ground.
#[must_use]
pub fn cube_ink(light: bool) -> CubeInk {
    if light {
        CubeInk {
            face_base: 165.0,
            face_span: 40.0,
            hover_lift: 30,
            edge: Color32::from_gray(90),
            label: Color32::from_gray(20),
            button: 185,
            outline_buttons: true,
            glyph_on: Color32::from_gray(10),
            glyph_off: Color32::from_gray(60),
            hint: Color32::from_gray(60),
            marker: [Color32::from_gray(50), Color32::from_gray(40), Color32::from_gray(20)],
            triad: [Color32::from_rgb(215, 20, 20), Color32::from_rgb(0, 140, 30), Color32::from_rgb(20, 70, 230)],
        }
    } else {
        CubeInk {
            face_base: 46.0,
            face_span: 46.0,
            hover_lift: 52,
            edge: Color32::from_white_alpha(170),
            label: Color32::from_white_alpha(230),
            button: 46,
            outline_buttons: false,
            glyph_on: Color32::WHITE,
            glyph_off: Color32::from_white_alpha(160),
            hint: Color32::from_white_alpha(200),
            marker: [Color32::from_gray(225), Color32::from_gray(240), Color32::WHITE],
            triad: [Color32::from_rgb(245, 70, 70), Color32::from_rgb(60, 220, 80), Color32::from_rgb(90, 140, 255)],
        }
    }
}

/// The cube greys for the live theme's viewport ground.
#[must_use]
pub fn cube() -> CubeInk {
    cube_ink(light_ground(theme::current().viewport))
}

/// How much of its own colour an unselected path keeps on a light
/// ground: it fades toward the ground (a dimmed line on paper is a
/// paler one), never toward black, which turns every hue to ink.
const LIGHT_DIM_KEEP: f32 = 0.6;

/// A path colour dimmed by `dim` (1.0 = selected, untouched). The dark
/// ground scales toward black as it always did; a light ground fades
/// toward the ground itself, keeping the hue readable.
#[must_use]
pub fn dimmed(base: [f32; 3], dim: f32, ground: Color32) -> [f32; 3] {
    if dim >= 1.0 {
        return base;
    }
    if !light_ground(ground) {
        return [base[0] * dim, base[1] * dim, base[2] * dim];
    }
    faded(base, LIGHT_DIM_KEEP, ground)
}

/// `base` faded toward `ground`, keeping `keep` of its own colour: the
/// light ground's half of [`dimmed`], open to a caller that needs its own
/// share (#871: the refusal ghost keeps more, so it still clears 3:1).
#[must_use]
pub fn faded(base: [f32; 3], keep: f32, ground: Color32) -> [f32; 3] {
    let g = [ground.r(), ground.g(), ground.b()].map(|c| f32::from(c) / 255.0);
    let k = keep.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| base[i] * k + g[i] * (1.0 - k))
}
