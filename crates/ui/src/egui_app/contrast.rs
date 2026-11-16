//! The contrast arithmetic (M6.1 D): WCAG 2.x ratios over sRGB, the
//! readable text-on-fill choice, the follow law that derives a theme
//! file's unspelled accent states from its new accent, and the
//! readability gate a file must pass to land. Pure colour maths - no
//! chrome, no egui types beyond `Color32`; `theme::visuals` and
//! `theme_files::parse` are the consumers, and both built-in columns
//! stay hand-tuned (the dark-verbatim law keeps the snapshots
//! meaningful). The law this module enforces: a theme that cannot be
//! read is a visible refusal, not a style preference.
//! Provenance: the contrast formula is WCAG 2.1's public definition
//! (W3C REC); the derive-don't-trust-hand-typed-states architecture is
//! the sibling project's pattern at the user's direction; the
//! thresholds, the follow law's linear-light form and every number are
//! ours.

use super::theme::Tokens;
use egui::Color32;

/// Body-text contrast floor (WCAG 1.4.3 AA for normal text): every
/// reading pair must clear this.
pub const BODY: f64 = 4.5;
/// The floor for meaningful non-text (WCAG 1.4.11): the selection
/// stroke against its ground.
pub const UI: f64 = 3.0;

/// sRGB gamma -> linear-light per channel (WCAG 2.1's definition).
fn lin(v: u8) -> f64 {
    let s = f64::from(v) / 255.0;
    if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

/// linear-light -> sRGB gamma, clamped to the gamut. A negative
/// linear input (a translated step that undershot black) lands on 0,
/// never NaN: the multiply branch below is where negatives die.
fn gamma(v: f64) -> u8 {
    let s = if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.max(0.0).powf(1.0 / 2.4) - 0.055 };
    (s.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The three linear-light channels of an opaque sRGB colour.
fn to(c: Color32) -> [f64; 3] {
    [lin(c.r()), lin(c.g()), lin(c.b())]
}

/// The WCAG 2.x relative luminance of an opaque sRGB colour, 0.0 to
/// 1.0. Public for the golden: "a lift" is a sentence about luminance,
/// and the tests say it without re-doing the maths.
#[must_use]
pub fn luma(c: Color32) -> f64 {
    let [r, g, b] = to(c);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// The WCAG contrast ratio of two colours, 1.0 (identical) to 21.0
/// (black on white). Alpha is not part of the reading pair - the shell
/// composites opaquely - so the r/g/b channels answer alone.
#[must_use]
pub fn ratio(a: Color32, b: Color32) -> f64 {
    let (x, y) = (luma(a), luma(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The readable ink for a filled widget: whichever of white or black
/// carries the better ratio. The derivation replaces the hand-typed
/// `Color32::WHITE` the first tables wore - a pale user accent gets
/// dark text on its buttons, not invisible white.
#[must_use]
pub fn on_fill(fill: Color32) -> Color32 {
    if ratio(Color32::WHITE, fill) >= ratio(Color32::BLACK, fill) {
        Color32::WHITE
    } else {
        Color32::BLACK
    }
}

/// The follow law: a theme file that changes the ACCENT but leaves an
/// accent-state UNSPELLED gets the base column's own step - the
/// per-channel linear-light difference `base_state - base.accent` -
/// translated onto the new accent. Hover still lifts because dark's
/// hover lifted; active still sinks toward the field because that is
/// what the base's active did. Linear-light arithmetic so the step
/// survives both a lift and a sink, and the sum clamps instead of
/// wrapping when it leaves the gamut. This is the sentence the ROADMAP
/// asked for: states generated from the accent, not trusted from a
/// hand.
#[must_use]
pub fn follow(new_accent: Color32, base: &Tokens, base_state: Color32) -> Color32 {
    let (n, o, b) = (to(new_accent), to(base.accent), to(base_state));
    Color32::from_rgb(gamma(n[0] + b[0] - o[0]), gamma(n[1] + b[1] - o[1]), gamma(n[2] + b[2] - o[2]))
}

/// Derive the unspelled accent states of a file's table in place,
/// returning one honest note per state that moved (the derivation must
/// never be a surprise on screen). `spelled` answers whether the
/// file's `colors` map named a token. A file that changed no accent
/// keeps the base's whole family hand-tuned and untouched; a file that
/// changed the accent drags every state it did not spell along.
pub fn follow_states(t: &mut Tokens, base: &Tokens, spelled: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut notes = Vec::new();
    if spelled("accent") {
        if !spelled("accent_hover") {
            t.accent_hover = follow(t.accent, base, base.accent_hover);
            notes.push("accent_hover followed the new accent (not spelled)".to_owned());
        }
        if !spelled("field_active") {
            t.field_active = follow(t.accent, base, base.field_active);
            notes.push("field_active followed the new accent (not spelled)".to_owned());
        }
        if !spelled("selection_bg") {
            t.selection_bg = follow(t.accent, base, base.selection_bg);
            notes.push("selection_bg followed the new accent (not spelled)".to_owned());
        }
    }
    if !spelled("selection_stroke") {
        // Both built-in columns store the stroke AT the hover; the law
        // rides through files too, spelled or derived, so the focus
        // line always matches the button the eye just followed.
        t.selection_stroke = t.accent_hover;
        if t.selection_stroke != base.selection_stroke {
            notes.push("selection_stroke rides accent_hover (not spelled)".to_owned());
        }
    }
    notes
}

/// The readability gate: can this table render the text it will be
/// asked to render? `text` sits on every ground the shell owns, the
/// dim tier on the two strips, the selection stroke must be visible
/// against the window ground, and whatever ink `on_fill` chooses must
/// actually clear body contrast on both accent fills (the pair
/// visuals() paints). `None` = the table reads; `Some(sentence)` = the
/// first failure, named with its ratio. Two exemptions, both honest:
/// the faint tier is disabled-state ink, the one WCAG itself excuses;
/// and the on-fill pairs cannot fail by the formula's shape (the
/// better of white and black is always at least 4.58:1), so they are
/// written here as the law they guard, checkable should `on_fill`
/// ever grow a third ink.
#[must_use]
pub fn readable(t: &Tokens) -> Option<String> {
    let grounds = [("ink", t.ink), ("ink_alt", t.ink_alt), ("well", t.well), ("field_bg", t.field_bg)];
    for (ground, bg) in grounds {
        let r = ratio(t.text, bg);
        if r < BODY {
            return Some(format!(
                "text on {ground} reads {r:.1}:1, under the {BODY}:1 floor - an unreadable theme is refused, not rendered"
            ));
        }
    }
    for (ground, bg) in [("ink", t.ink), ("ink_alt", t.ink_alt)] {
        let r = ratio(t.text_dim, bg);
        if r < BODY {
            return Some(format!(
                "text_dim on {ground} reads {r:.1}:1, under the {BODY}:1 floor"
            ));
        }
    }
    for (name, fill) in [("accent", t.accent), ("accent_hover", t.accent_hover)] {
        let r = ratio(on_fill(fill), fill);
        if r < BODY {
            return Some(format!(
                "no ink is readable on {name} at {r:.1}:1 - spell a stronger {name}"
            ));
        }
    }
    let r = ratio(t.ribbon_ink, t.strip_fill());
    if r < BODY {
        return Some(format!(
            "ribbon_ink on the ribbon ground reads {r:.1}:1, under the {BODY}:1 floor - the ribbon glyphs would vanish"
        ));
    }
    let r = ratio(t.selection_stroke, t.ink_alt);
    if r < UI {
        return Some(format!(
            "selection_stroke on ink_alt reads {r:.1}:1, under the {UI}:1 floor for meaningful lines"
        ));
    }
    None
}
