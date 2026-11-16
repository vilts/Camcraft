//! Which shape of a modelled tool body each law judges (#835). The
//! verdict laws - reach (c), the mid-turn sweep (c3), the solid (b2),
//! containment and rotary laws, the fixture laws and (#842) the drill
//! reach, cross-drill and touch-probe stylus laws - read the body's
//! band stack, the shape the simulation draws. Only a body whose stack
//! does not fit the tool's record ([`falls_back`]) is judged by the
//! cylinder enclosing it, and a refusal naming the holder, or the probe
//! path, then says so by name. The stepped
//! reading of a solid of revolution is machine-tool handbook practice;
//! our own design, no vendor source consulted.

use camcraft_tools::holder_profile::HolderProfile;

use crate::machining2d::GenError;
use crate::tools::Tool;
use crate::verify::BodyBands;

/// Does a tool carrying `body` fall back to the cylinder enclosing it
/// (#842)? Its band stack does not fit the judged record `tool`
/// ([`BodyBands::of`]), so every stack law reads that cylinder instead.
#[must_use]
pub fn falls_back(tool: &Tool, body: &HolderProfile) -> bool {
    let g = tool.geom();
    BodyBands::of(&g, &crate::tool_body::stack(&g, Some(body))).is_none()
}

/// The words every kept-cylinder law adds for a tool with a body.
#[must_use]
pub fn enclosure_words(tool: &Tool) -> String {
    format!(
        "the tool body is judged here by the cylinder enclosing it, Ø{:.3} mm to {:.3} mm above the tip",
        tool.holder_d,
        tool.flute_len + tool.shank_len + tool.holder_len
    )
}

/// A generator refusal that names the holder, for a tool carrying a
/// body (`tool` is the judged record, its holder the enclosing
/// cylinder): the refusal says the body was judged by that cylinder.
/// Every other refusal, and every refusal of a tool with no body, is
/// returned unchanged.
#[must_use]
pub fn name_enclosure(e: GenError, tool: &Tool, body: Option<&HolderProfile>) -> GenError {
    if body.is_none() {
        return e;
    }
    let say = |m: String| if m.contains("holder") { format!("{m} ({})", enclosure_words(tool)) } else { m };
    // #871: `map_msg` rewrites a located or ghost-carrying refusal's
    // sentence alike, its spot and ghost riding along.
    e.map_msg(say)
}

/// The touch probe's note for a tool whose body its stylus reach reads
/// as one enclosing band (#842: a body that does not fit the stylus).
#[must_use]
pub fn probe_note(tool: &Tool, body: Option<&HolderProfile>) -> Option<String> {
    body.map(|_| format!("probe reach: {}", enclosure_words(tool)))
}
