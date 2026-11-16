//! The program ahead of one operation: who machines before it in its
//! setup, and the digest of the paths and tool atoms its rest depends on.
//!
//! Provenance: our own design (M3.1 stage D), split out of `rest.rs` for
//! AGENTS.md's 400-line limit (issue #127); general machining knowledge,
//! no vendor implementation inspected. The laws live in `rest.rs`'s header.

use crate::cl::Move;
use crate::model;
use crate::stock_sim;
use flecs_ecs::core::EntityViewGet;
use flecs_ecs::prelude::World;

/// One operation ahead of the consumer: name and the hash of its
/// recorded path signature (both digest terms), its tool atom (the grid depends
/// on it), and the cached moves themselves when it has cut anything.
pub(super) struct Ahead {
    pub(super) name: String,
    /// The operation's kind: on a lathe only the Facing kind is read
    /// ([`super::prior_plan`], #89 review).
    pub(super) kind: crate::operation::OpType,
    path: u64,
    pub(super) radius: f64,
    pub(super) ball: bool,
    /// Cot of the half-angle, Some for a chamfer mill (M2.4 B) and a
    /// twist drill's point (#89 review: a pilot's cone stands):
    /// the rest grid must sweep the same cone the program sim does, or
    /// a predecessor's chamfer would evaporate a full disc of
    /// material that is really still standing.
    pub(super) cone: Option<f64>,
    /// The bull nose's corner radius (M13.1, `stock_sim::bull_corner`):
    /// the rest grid sweeps the same corner the program sim does, so a
    /// bull predecessor's fillet ring reads as the rest it is.
    pub(super) corner: Option<f64>,
    /// A turning insert's nose radius (issue #89): its moves carry the
    /// nose CENTRE, so the prior plan cuts it as a ball of that radius
    /// (tip = centre - nose) - the insert's real cutting edge, never
    /// the flat disc of its whole size. Only [`super::PriorPlan`] reads it.
    pub(super) nose: Option<f64>,
    pub(super) cut: bool,
    pub(super) moves: Vec<Move>,
    /// The operation's effective setup (#318: the chain across setups
    /// groups the program by it).
    pub(super) setup: Option<String>,
}

/// The operations in program order ahead of `name` in the consumer's
/// setup. The ordering law is shared with the post BY CALL - both walk
/// [`crate::setup::operation_order`] - and the sequence is cut at the
/// consumer, same-setup filtered (module header). `posting::program_ops`
/// itself is not called: it refuses whole programs over a `Never`
/// operation, and an operation later in the program is nobody's
/// business here.
/// `with_moves` clones each cutting predecessor's moves; the digest and
/// the stale story pass `false` and read only the path signature stored
/// with the cache (#109 review B1: `status_of` asks every frame).
pub(super) fn ahead_of(world: &World, name: &str, consumer_setup: Option<&str>, with_moves: bool) -> Vec<Ahead> {
    let consumer = crate::setup::effective(world, consumer_setup);
    // another setup's cut is not this block's rest
    collect(world, name, with_moves, |s| *s == consumer)
}

/// Every operation ahead of `name` in program order, whatever its setup
/// (#318): the chain across setups (`super::across`) cuts it into
/// per-setup segments.
pub(super) fn all_ahead(world: &World, name: &str, with_moves: bool) -> Vec<Ahead> {
    collect(world, name, with_moves, |_| true)
}

/// The walk both readings share: the program in order up to `name`,
/// keeping the operations whose effective setup `keep` accepts.
fn collect(world: &World, name: &str, with_moves: bool, keep: impl Fn(&Option<String>) -> bool) -> Vec<Ahead> {
    let ops = crate::setup::operation_order(world);
    let Some(self_ix) = ops
        .iter()
        .position(|(n, _)| n == name)
    else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(self_ix);
    for (n, e) in &ops[..self_ix] {
        let Some(op) = model::operation_of(world, e) else {
            continue; // in the operation list; lost a beat later
        };
        let setup = crate::setup::effective(world, op.setup.as_deref());
        if !keep(&setup) {
            continue;
        }
        let tool = model::find_by_name(world, &op.tool)
            .and_then(|t| model::tool_of(world, &t));
        // The cached PATH is the truth for the digest: an edited op's
        // moved parameters are its OWN staleness, and its cache still
        // says exactly what the program would post today. Cutting
        // nothing - never generated, or an empty path - is what
        // contributes no mask.
        // A probe cuts nothing (#108): it contributes no mask, and no
        // path signature - it is a never-cutting operation here.
        // A torch (#670) cuts through along a kerf no mask models: the
        // plasma table runs no milling rest after it. A sinker
        // electrode (#541) burns a box no disc mask models: counting
        // it as nothing leaves the material standing for a later op -
        // the safe side (air, never a gouge).
        let probe = tool.as_ref().is_some_and(|t| {
            matches!(
                t.kind,
                crate::tools::ToolKind::Probe | crate::tools::ToolKind::Torch | crate::tools::ToolKind::Electrode
            )
        });
        let (cut, sig, moves) = if probe {
            (false, 0, Vec::new())
        } else {
            e.entity_view(world)
                .try_get::<&crate::schema::Toolpath>(|t| {
                    let cut = !t.path.moves.is_empty();
                    (cut, t.path_sig(), if cut && with_moves { t.path.moves.clone() } else { Vec::new() })
                })
                .unwrap_or((false, 0, Vec::new()))
        };
        out.push(Ahead {
            name: n.clone(),
            kind: op.params.kind(),
            path: if cut { sig } else { 0 },
            radius: tool.as_ref().map_or(0.0, |t| t.diameter * 0.5),
            ball: tool
                .as_ref()
                .is_some_and(|t| t.kind == crate::tools::ToolKind::BallNose),
            cone: tool
                .as_ref()
                .filter(|t| matches!(t.kind, crate::tools::ToolKind::Taper | crate::tools::ToolKind::Drill))
                .map(|t| (t.tip_angle.to_radians() * 0.5).tan())
                .filter(|x| x.is_finite() && *x > 0.0)
                .map(|x| 1.0 / x),
            // A nonsense corner falls back to the disc: over-reported
            // removal, the side `Tool::is_sane` keeps unreachable.
            corner: tool
                .as_ref()
                .and_then(|t| stock_sim::bull_corner(t).ok().flatten()),
            nose: tool
                .as_ref()
                // A boring bar's moves carry its nose centre too (#146
                // review B1): the feed law credits its whole nose kerf.
                .filter(|t| matches!(t.kind, crate::tools::ToolKind::Turning | crate::tools::ToolKind::Bore))
                .map(|t| t.nose_radius)
                .filter(|n| n.is_finite() && *n > 0.0),
            cut,
            moves,
            setup,
        });
    }
    out
}

/// One predecessor's contribution to the digest: a never-cutting
/// operation names itself and nothing else (its tool's size is as
/// irrelevant to this stamp as its moves - inert data law), a cutting
/// one contributes the path it would post and the atom that would
/// carve it.
pub(super) fn digest_of(ahead: &[Ahead]) -> u64 {
    use std::hash::Hasher;
    let mut h = std::hash::DefaultHasher::new();
    h.write(b"camcraft-rest-v1");
    for a in ahead {
        h.write(a.name.as_bytes());
        // The kind (review N3): the prior plan and the pilot and turned
        // credits branch on it (Facing, Drill, the turning family).
        h.write_u8(a.kind.info().tag);
        if a.cut {
            h.write_u8(1);
            h.write_u64(a.path);
            h.write(&a.radius.to_bits().to_le_bytes());
            h.write_u8(u8::from(a.ball));
            // The cone's slope is part of the atom the grid depends
            // on, so a re-coned chamfer mill churns the stamp.
            match a.cone {
                Some(c) => {
                    h.write_u8(1);
                    h.write(&c.to_bits().to_le_bytes());
                }
                None => h.write_u8(0),
            }
            // The bull's corner (M13.1) is atom too; only a bull
            // writes the byte, so every other stamp hashes unchanged.
            if let Some(c) = a.corner {
                h.write_u8(2);
                h.write(&c.to_bits().to_le_bytes());
            }
            // The insert's nose is atom too (#89 review).
            if let Some(n) = a.nose {
                h.write_u8(3);
                h.write(&n.to_bits().to_le_bytes());
            }
        } else {
            h.write_u8(0);
        }
    }
    h.finish()
}
