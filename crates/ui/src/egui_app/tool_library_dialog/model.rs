//! #3894 (U3 of #3663): what the tool library window's preview shows of
//! the tool open in its editor - the steel outline (the landed drawn
//! cutter carried onto the draft numbers, else `Tool::profile` up to the
//! shank top), the holder bands the collision check sweeps, the 3D mesh
//! (the steel revolved, then the landed body's own facets or the
//! revolved holder cylinder) and the numbers its dimension lines read.
//! Display only: no law, check or post reads any of it, and nothing here
//! queues a command.
//! #3895 (U4): a holder pick of the window's draft (entry, gauge length,
//! projection) is drawn in place of the landed body, built by the same
//! path as Apply holder (`tool_body::build_library`, then `fits`); a pick
//! that path refuses keeps the landed body and carries the refusal. The
//! Apply-holder refusal is judged on the stored tool, as the command
//! judges it; the draft numbers' own verdict is a separate draft note.
//! #3897 (U6): the model also keeps the tool without its holder (`bare`)
//! for the preview's Holder toggle, built with the rest.
//! #3908: the model also keeps the steel with its helical flutes
//! (`fluted`, `fluted_bare`) for the preview's Flutes toggle, built with
//! the rest from the same steel meridian (`flutes`). Display only: the
//! steel, the bands and the plain meshes do not change with the flutes.
//! Frame cost (`crates/ui/AGENTS.md`): the model is built when its input
//! changes - the draft tool value, the document's tool, the landed cutter
//! and the landed body (its mesh by identity) - never on `doc_version`.
//! A still frame reads the inputs and builds nothing (`builds`).
//! Provenance: our own display model over `app::tool_body` and
//! `app::spun`; the revolve follows `app::tool_body_view::revolve`
//! (general surface-of-revolution practice). No vendor material consulted.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use camcraft_app::cutter_fit;
use camcraft_app::cutter_profile;
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::holder_library::{self, HolderRef};
use camcraft_app::model;
use camcraft_app::schema::ToolBody;
use camcraft_app::spun::SpunProfile;
use camcraft_app::tool_body;
use camcraft_app::tool_cutter;
use camcraft_app::tools::Tool;

use crate::state::{State, is_spun_kind};

/// Facets around the axis per revolved ring (as `tool_body_view`).
const RING: usize = 32;

/// The numbers the side view's dimension lines read, model units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Dims {
    pub diameter: f64,
    pub flute_len: f64,
    pub shank_d: f64,
    pub shank_len: f64,
    /// The tip to the holder face.
    pub stick_out: f64,
    /// The tip to the holder top.
    pub gauge: f64,
    /// The holder's largest diameter and its length, when there is one.
    pub holder: Option<(f64, f64)>,
}

/// The preview's model of one tool.
pub(crate) struct ToolModel {
    /// The draft tool the model was built from.
    pub tool: Tool,
    /// The cutter and shank outline `(z, r)`, tip up, to the shank top.
    pub steel: Vec<(f64, f64)>,
    /// The holder bands `(above, len, r)` the collision check sweeps.
    pub bands: Vec<(f64, f64, f64)>,
    /// The whole tool for the 3D view, tool frame (tip at the origin, +Z up).
    pub mesh: Arc<[Triangle]>,
    /// The mesh's box with the origin (low and high corner), for the view.
    pub bounds: ([f64; 3], [f64; 3]),
    /// #3897: the cutter and the shank only (the Holder toggle off), and
    /// its box.
    pub bare: Arc<[Triangle]>,
    pub bare_bounds: ([f64; 3], [f64; 3]),
    /// #3908: the whole tool and the bare tool with helical flutes cut
    /// into the fluted length, when the tool shows flutes. Display only;
    /// inside the plain meshes' envelope, so the plain boxes fit them.
    pub fluted: Option<Arc<[Triangle]>>,
    pub fluted_bare: Option<Arc<[Triangle]>>,
    /// The landed body's source, when the holder is a body.
    pub body: Option<String>,
    pub dims: Dims,
    /// #3895: why Apply holder refuses the draft's holder pick, by name -
    /// judged on the stored tool, as the command judges it.
    pub holder_err: Option<String>,
    /// What the same checks say of the unapplied draft numbers, when that
    /// differs from `holder_err` - a draft note, not the command's verdict.
    pub draft_err: Option<String>,
}

/// What the model is built from. The body compares by its mesh identity
/// and its placement, never facet by facet.
struct Key {
    tool: Tool,
    doc: Option<Tool>,
    cutter: Option<SpunProfile>,
    body: Option<ToolBody>,
    /// #3895: the draft's holder pick (entry, gauge length, projection).
    holder: Option<(String, f64, f64)>,
}

impl Key {
    fn same(&self, o: &Self) -> bool {
        let body = match (&self.body, &o.body) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                Arc::ptr_eq(&a.facets, &b.facets)
                    && a.frame == b.frame
                    && a.library == b.library
                    && a.machine_default == b.machine_default
                    && a.source == b.source
            }
            _ => false,
        };
        body && self.tool == o.tool && self.doc == o.doc && self.cutter == o.cutter && self.holder == o.holder
    }
}

thread_local! {
    static MEMO: RefCell<Option<(Key, Rc<ToolModel>)>> = const { RefCell::new(None) };
    static BUILDS: Cell<u64> = const { Cell::new(0) };
}

/// The model builds made on this thread - the counter of the frame-cost
/// rule: a still frame adds none.
#[cfg(test)]
pub(crate) fn builds() -> u64 {
    BUILDS.with(Cell::get)
}

/// The last model built on this thread, for the tests.
#[cfg(test)]
pub(crate) fn last() -> Option<Rc<ToolModel>> {
    MEMO.with(|m| m.borrow().as_ref().map(|(_, v)| Rc::clone(v)))
}

/// The model of the tool in `state.tool_ui` (the window's draft while the
/// editor draws). It reads the document's landed cutter and body by the
/// draft's name, and never syncs Properties' shape drafts.
pub(crate) fn of(state: &State) -> Rc<ToolModel> {
    let tool = state.tool_ui.to_tool();
    let e = model::find_by_name(&state.world, &state.tool_ui.name);
    let d = &state.tool_profile_ui;
    let holder = (d.tool == state.tool_ui.name && is_spun_kind(tool.kind))
        .then(|| d.library.clone().map(|n| (n, d.gauge, d.projection)))
        .flatten();
    let key = Key {
        holder,
        doc: e.as_ref().and_then(|e| model::tool_of(&state.world, e)),
        cutter: e.as_ref().and_then(|e| tool_cutter::cutter_of(&state.world, e)),
        body: e.as_ref().and_then(|e| tool_body::body_of(&state.world, e)),
        tool,
    };
    MEMO.with(|m| {
        let mut m = m.borrow_mut();
        if let Some((k, v)) = m.as_ref()
            && k.same(&key)
        {
            return Rc::clone(v);
        }
        BUILDS.with(|b| b.set(b.get().saturating_add(1)));
        let v = Rc::new(build(&key));
        *m = Some((key, Rc::clone(&v)));
        v
    })
}

/// The body Apply holder would land for the pick `(entry, gauge length,
/// projection)` on `t`, or its refusal by name - the command's own checks
/// (`SetToolBody::library`): the entry, the shank, the lengths, the fit.
pub(crate) fn draft_body(t: &Tool, entry: &str, gauge: f64, projection: f64) -> Result<ToolBody, String> {
    let e = holder_library::resolve(entry, t.shank_d).map_err(|e| e.to_string())?;
    e.fits_shank(t.shank_d).map_err(|e| e.to_string())?;
    let body = tool_body::build_library(&HolderRef::to(&e, gauge, projection))?;
    tool_body::fits(t, &body)?;
    Ok(body)
}

/// A mesh's box: the low and the high corner.
pub(crate) type Bounds = ([f64; 3], [f64; 3]);

impl ToolModel {
    /// #3908: the mesh the 3D view paints for the Holder and the Flutes
    /// toggles, and the box it is fitted by. The box is the plain mesh's
    /// box (the flutes only take radius away), so a toggle never jumps
    /// the view.
    pub(crate) fn shown(&self, holder: bool, flutes: bool) -> (&Arc<[Triangle]>, Bounds) {
        let (plain, fluted, bounds) = if holder {
            (&self.mesh, &self.fluted, self.bounds)
        } else {
            (&self.bare, &self.fluted_bare, self.bare_bounds)
        };
        let mesh = fluted.as_ref().filter(|_| flutes).unwrap_or(plain);
        (mesh, bounds)
    }
}

fn build(k: &Key) -> ToolModel {
    let t = &k.tool;
    let judge = |tool: &Tool| k.holder.as_ref().map(|(n, g, p)| draft_body(tool, n, *g, *p));
    let refusal = |r: &Option<Result<ToolBody, String>>| r.as_ref().and_then(|r| r.as_ref().err().cloned());
    let drafted = judge(t);
    let draft_refusal = refusal(&drafted);
    // Apply holder (`SetToolBody::library`) judges the STORED tool, not
    // the unapplied draft numbers: its verdict comes from `k.doc`. The
    // draft's own verdict is only a separate note (review of #3931).
    let holder_err = match &k.doc {
        Some(doc) if doc != t => refusal(&judge(doc)),
        Some(_) => draft_refusal.clone(),
        None => None,
    };
    let draft_err = draft_refusal.filter(|e| holder_err.as_ref() != Some(e));
    let body = drafted.and_then(Result::ok).or_else(|| k.body.clone());
    let zs = t.flute_len.max(0.0) + t.shank_len.max(0.0);
    let rs = t.shank_d.max(0.0) * 0.5;
    // The landed drawn cutter, carried onto the draft numbers the way the
    // editor's Apply carries it (`State::apply_tool_numbers`).
    let drawn = k.cutter.as_ref().and_then(|c| {
        let points = match &k.doc {
            Some(was) if !cutter_fit::same_shape(was, t) => cutter_fit::refit(was, t, c.points()),
            _ => c.points().to_vec(),
        };
        cutter_profile::check(t, points).ok().map(|s| s.outline())
    });
    let steel = drawn.unwrap_or_else(|| Tool { holder_len: 0.0, ..t.clone() }.profile());
    let profile = body.as_ref().and_then(|b| b.profile());
    let bands = match &body {
        Some(b) => tool_body::checked_bands(t, b).unwrap_or_default(),
        None => tool_body::stack(&t.geom(), None).bands.iter().map(|q| (q.above, q.len, q.r)).collect(),
    };
    // The shank runs up to the body's foot when the body stands clear of
    // it, as the fixture laws fill that gap (`tool_body_view::assembly`).
    let top = profile.map_or(zs, |p| zs.max(p.bottom()));
    let mut prof: Vec<(f64, f64)> =
        steel.iter().copied().filter(|&(z, r)| z.is_finite() && r.is_finite() && z < zs - 1e-9).collect();
    prof.extend([(zs, rs), (top, rs), (top, 0.0)]);
    let mut mesh = revolve(&prof);
    let bare_bounds = crate::egui_app::tool_model_view::bounds(&mesh);
    let bare: Arc<[Triangle]> = Arc::from(mesh.clone());
    let fluted_steel = super::flutes::fluted(t, &prof);
    let mut holder_tris = Vec::new();
    let holder = match (&body, profile) {
        (Some(b), p) => {
            holder_tris.extend(b.facets.iter().copied());
            p.map(|p| (bands.iter().map(|q| q.2).fold(0.0, f64::max) * 2.0, (p.top() - p.bottom()).max(0.0)))
        }
        (None, _) => SpunProfile::holder_of(t).map(|h| {
            holder_tris.extend(revolve(&h.outline()));
            (t.holder_d, t.holder_len)
        }),
    };
    mesh.extend(holder_tris.iter().copied());
    let (fluted, fluted_bare) = match fluted_steel {
        Some(f) => {
            let alone: Arc<[Triangle]> = Arc::from(f.clone());
            let mut all = f;
            all.extend(holder_tris);
            (Some(Arc::from(all)), Some(alone))
        }
        None => (None, None),
    };
    let gauge = match (profile, holder) {
        (Some(p), _) => p.top().max(top),
        (None, Some((_, len))) => zs + len,
        (None, None) => zs,
    };
    let dims = Dims {
        diameter: t.diameter,
        flute_len: t.flute_len,
        shank_d: t.shank_d,
        shank_len: t.shank_len,
        // A library holder states its nose face (#3895).
        stick_out: body.as_ref().and_then(|b| b.library.as_ref()).map_or(top, |r| r.projection),
        gauge,
        holder,
    };
    let bounds = crate::egui_app::tool_model_view::bounds(&mesh);
    ToolModel {
        tool: t.clone(),
        steel,
        bands,
        bounds,
        mesh: Arc::from(mesh),
        bare,
        bare_bounds,
        fluted,
        fluted_bare,
        body: body.as_ref().map(|b| b.source.clone()),
        dims,
        holder_err,
        draft_err,
    }
}

/// A `(z, r)` meridian spun about +Z into triangles; rings at r = 0 close
/// to a point, flat steps make annuli. Non-finite points are skipped.
fn revolve(prof: &[(f64, f64)]) -> Vec<Triangle> {
    let at = |z: f64, r: f64, k: usize| {
        let a = std::f64::consts::TAU * k as f64 / RING as f64;
        Point3::new(r * a.cos(), r * a.sin(), z)
    };
    let mut out = Vec::new();
    for w in prof.windows(2) {
        let ((z0, r0), (z1, r1)) = (w[0], w[1]);
        let finite = [z0, r0, z1, r1].iter().all(|v| v.is_finite());
        if !finite || ((z0 - z1).abs() < 1e-12 && (r0 - r1).abs() < 1e-12) {
            continue;
        }
        for k in 0..RING {
            let (a, b, c, d) = (at(z0, r0, k), at(z0, r0, k + 1), at(z1, r1, k + 1), at(z1, r1, k));
            if r0 > 1e-12 {
                out.push(Triangle::new(a, b, c));
            }
            if r1 > 1e-12 {
                out.push(Triangle::new(a, c, d));
            }
        }
    }
    out
}
