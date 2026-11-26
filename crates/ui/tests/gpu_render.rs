//! GPU smoke tests for the viewport pipelines, plus the tutorial pictures.
//!
//! Provenance: our own test rig; nothing here runs in the app path.
//!
//! Headless UI tests build frames with no GPU, so buffer/pipeline/draw code
//! is otherwise unverified. These tests instantiate a real `View3D`, render
//! known geometry, and assert it paints pixels. The device comes from
//! `headless_gpu`: the software adapter, else the hardware one (#615). On
//! a machine with no adapter at all they print a loud SKIP (not a pass) -
//! `CAMCRAFT_REQUIRE_GPU=1` makes that a failure.
//!
//! The docs test at the bottom lives here rather than in its own binary on
//! purpose: every fallback device in this crate shares the one `GPU_TEST_LOCK`
//! below, and a fifth GPU process racing device creation across process
//! boundaries is what made full-suite runs flake.
//!
//! Gate map (M4.4): covers ui::state, ui::viewport, app::stock.

use std::collections::HashSet;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use egui_wgpu::wgpu;
use image::{ImageFormat, RgbaImage};
use camcraft_app::geometry::transform_point;
use camcraft_app::stock::{StockShape, stock_mesh};
use camcraft_app::{model, setup, setup_align};
use camcraft_ui::interact;
use camcraft_ui::state::{CommandKind, DisplayMode, State};
use camcraft_ui::viewport::{Camera, GpuPart, ProjectionMode, View3D};

/// #2958: the wide overlay lines and MSAA, on this binary's device.
#[path = "gpu_render/wide_lines.rs"]
mod wide_lines;
/// #2834 + #2958: the shade passes follow the 3D pass's sample count.
#[path = "gpu_render/shade_msaa.rs"]
mod shade_msaa;

const SIDE: u32 = 256;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
// Concurrent fallback-device creation and teardown can crash in software
// Vulkan drivers; keep the entire render/readback lifecycle on one test at
// a time while leaving the rest of the test suite parallel.
static GPU_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Software adapter first, else the hardware one (#615); no adapter at
/// all prints a loud SKIP, and `CAMCRAFT_REQUIRE_GPU=1` fails instead.
fn device_and_queue() -> Option<(wgpu::Device, wgpu::Queue)> {
    match camcraft_ui::headless_gpu::device_for_test("gpu_render") {
        Ok(gpu) => gpu.map(|g| (g.device, g.queue)),
        Err(err) => panic!("{err}"),
    }
}

/// This binary renders every frame on one fallback device. Device
/// creation and teardown are the cross-process crash point (sibling GPU
/// test binaries and egui_kittest's harness devices build their own in
/// parallel processes), so a per-frame device made this binary the
/// suite's loudest racer. One `OnceLock` creation per run, held until
/// process exit - the OS closes it, no in-process device drop races a
/// sibling's creation. Callers hold `GPU_TEST_LOCK` across use.
fn shared_device() -> Option<(Arc<wgpu::Device>, Arc<wgpu::Queue>)> {
    static SHARED: OnceLock<Option<(Arc<wgpu::Device>, Arc<wgpu::Queue>)>> = OnceLock::new();
    SHARED
        .get_or_init(|| device_and_queue().map(|(d, q)| (Arc::new(d), Arc::new(q))))
        .clone()
}

/// Triangle soup of a stock shape, world-identity placed.
fn soup_of(shape: &StockShape) -> Vec<[f32; 3]> {
    let tris = stock_mesh(shape).expect("sane shape meshes");
    let mut v = Vec::with_capacity(tris.len() * 3);
    for t in &tris {
        for p in [t.a, t.b, t.c] {
            v.push([p.x as f32, p.y as f32, p.z as f32]);
        }
    }
    v
}

fn stock_part(shape: &StockShape) -> GpuPart {
    GpuPart {
        verts: soup_of(shape),
        selected: false,
        stock: true,
        fill: false,
        classes: Vec::new(),
    }
}

fn model_part(shape_d: f64) -> GpuPart {
    model_part_at(shape_d, [0.0, 0.0, 0.0])
}

fn model_part_at(shape_d: f64, off: [f32; 3]) -> GpuPart {
    let mut verts = soup_of(&StockShape::Box {
        dx: shape_d,
        dy: shape_d,
        dz: shape_d,
    });
    for v in &mut verts {
        for k in 0..3 {
            v[k] += off[k];
        }
    }
    GpuPart {
        verts,
        selected: false,
        stock: false,
        fill: false,
        classes: Vec::new(),
    }
}

/// A flat CCW square sheet at height `z`, half-width `h`: the shape a
/// region fill part carries (a triangle list, no lighting, no wire).
fn fill_square(h: f32, z: f32) -> GpuPart {
    GpuPart {
        verts: vec![
            [-h, -h, z],
            [h, -h, z],
            [h, h, z],
            [-h, -h, z],
            [h, h, z],
            [-h, h, z],
        ],
        selected: false,
        stock: false,
        fill: true,
        classes: Vec::new(),
    }
}

fn render(parts: &[GpuPart], cam: &Camera) -> Option<(Vec<u8>, u32, u32)> {
    render_mode(parts, cam, DisplayMode::ShadedEdges)
}

/// One frame of `parts` under an explicit display mode.
fn render_mode(
    parts: &[GpuPart],
    cam: &Camera,
    display: DisplayMode,
) -> Option<(Vec<u8>, u32, u32)> {
    let _gpu_guard = GPU_TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let (device, queue) = shared_device()?;
    let (device, queue) = (device.as_ref(), queue.as_ref());
    let mut view = View3D::new(device, FORMAT, SIDE, SIDE);
    view.set_document(device, queue, parts);
    let mvp = cam.mvp(1.0);
    let eye = cam.eye_f32();
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("gpu-test-frame"),
    });
    view.render(queue, &mut enc, &mvp, eye, display);
    queue.submit([enc.finish()]);
    view.snapshot_rgba(device, queue)
}

/// True when the pixel differs from the frame's corner (background).
fn off_background(img: &[u8], ix: usize) -> bool {
    let bg: [u8; 4] = img[..4].try_into().expect("frame wider than 4 px");
    let p = &img[ix * 4..ix * 4 + 4];
    p.iter()
        .zip(bg)
        .any(|(a, b)| i32::from(*a) - i32::from(b) > 6 || i32::from(b) - i32::from(*a) > 6)
}

/// Pixels differing from the corner (background) color by a small epsilon.
fn changed_pixels(img: &[u8]) -> usize {
    let bg: [u8; 4] = img[..4].try_into().expect("frame wider than 4 px");
    img.chunks(4)
        .filter(|p| {
            p.iter().zip(bg).any(|(a, b)| i32::from(*a) - i32::from(b) > 6 || i32::from(b) - i32::from(*a) > 6)
        })
        .count()
}

/// Coarse color clusters among the changed pixels: how many distinct
/// surfaces the frame distinguishes.
fn distinct_colors(img: &[u8]) -> usize {
    let bg: [u8; 4] = img[..4].try_into().expect("frame wider than 4 px");
    let mut hues = HashSet::new();
    for p in img.chunks(4) {
        if p.iter().zip(bg).any(|(a, b)| (i32::from(*a) - i32::from(b)).abs() > 6) {
            hues.insert((p[0] >> 4, p[1] >> 4, p[2] >> 4));
        }
    }
    hues.len()
}

/// A camera that frames the stock shapes below (centered-ish on z∈[0,h]).
fn test_cam() -> Camera {
    Camera {
        target: [0.0, 0.0, 20.0],
        yaw: 0.9,
        pitch: 0.5,
        dist: 160.0,
        ..Camera::default()
    }
}

#[test]
fn box_stock_paints_steel_over_background() {
    let Some((img, w, h)) = render(
        &[stock_part(&StockShape::Box {
            dx: 60.0,
            dy: 60.0,
            dz: 40.0,
        })],
        &test_cam(),
    ) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let changed = changed_pixels(&img);
    let total = (w * h) as usize;
    assert!(
        changed > total / 20 && changed < total - total / 20,
        "box stock should cover a solid but partial band: {changed}/{total}"
    );
}

#[test]
fn cylinder_stock_paints_steel_over_background() {
    // The feedback-4 regression: the parametric cylinder must render just
    // like the box does.
    let Some((img, w, h)) = render(
        &[stock_part(&StockShape::Cylinder {
            diameter: 60.0,
            height: 40.0,
        })],
        &test_cam(),
    ) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let changed = changed_pixels(&img);
    let total = (w * h) as usize;
    assert!(
        changed > total / 20 && changed < total - total / 20,
        "cylinder stock should cover a solid but partial band: {changed}/{total}"
    );
}

#[test]
fn model_reads_through_translucent_stock() {
    // Opaque cube first, translucent cylinder stock around it: the frame
    // must distinguish background, bare model, and stock-blended regions.
    let parts = [
        model_part(20.0),
        stock_part(&StockShape::Cylinder {
            diameter: 60.0,
            height: 40.0,
        }),
    ];
    let Some((img, _w, _h)) = render(&parts, &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let hues = distinct_colors(&img);
    assert!(hues >= 3, "want >=3 color clusters, got {hues}");
}

/// M13.1 owed: the machined stock's extracted mesh (the STL export's
/// triangles) draws through the stock pipeline like the box it stands
/// in for - a 12x12x20 box faced to 18, at the app suite's coarse cell.
#[test]
fn machined_mesh_paints_as_stock() {
    use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
    use camcraft_app::geometry::{cube, IDENTITY_Q, Point3};
    use camcraft_app::operation::{Height, OpParams, SrcRef};
    use camcraft_app::schema::Location;
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let loc = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    let shape = StockShape::Box { dx: 12.0, dy: 12.0, dz: 20.0 };
    history.execute(&world, SetStock::new(&world, shape, loc).expect("box stock"));
    let tool = camcraft_app::tools::catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t);
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool.expect("catalog entry")).expect("tool"));
    let part = cube(Point3::new(-6.0, -6.0, 0.0), Point3::new(6.0, 6.0, 18.0));
    history.execute(&world, ImportModel::from_mesh("part", part));
    let params = OpParams::Face { holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(18.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false, blend_radius: None,
        picks: Default::default(),
    };
    history.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    let fv = camcraft_app::stock_view::field_view_with(&world, 12.0 * 12.0 * 20.0 / 3.6).expect("field view");
    let tris = camcraft_app::stock_export::machined_mesh(&fv).expect("a closed shell");
    let verts = tris.iter().flat_map(|t| [t.a, t.b, t.c].map(|p| [p.x as f32, p.y as f32, p.z as f32])).collect();
    let part = GpuPart { verts, selected: false, stock: true, fill: false, classes: Vec::new() };
    let cam = Camera { target: [0.0, 0.0, 10.0], yaw: 0.9, pitch: 0.5, dist: 50.0, ..Camera::default() };
    let Some((img, w, h)) = render(&[part], &cam) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let changed = changed_pixels(&img);
    let total = (w * h) as usize;
    assert!(changed > total / 20 && changed < total - total / 20, "the machined shell paints a band: {changed}/{total}");
}

#[test]
fn empty_document_clears_to_background_only() {
    let Some((img, w, h)) = render(&[], &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let changed = changed_pixels(&img);
    assert_eq!(changed, 0, "empty document on {w}x{h} should be uniform");
}

#[test]
fn wireframe_paints_wire_not_faces() {
    // The M1.4 display-mode law, pixel half: the SAME buffers render two
    // ways. Shaded floods the silhouette with painted faces; wireframe
    // leaves the face interiors as background and keeps only the (bright,
    // over the dark background - `u.wire` lifts the wire color) wire.
    let cube = [model_part(60.0)];
    let Some((shaded, w, h)) = render(&cube, &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let Some((wire, _, _)) = render_mode(&cube, &test_cam(), DisplayMode::Wireframe) else {
        return; // the shaded render found an adapter; this one cannot differ
    };
    let total = (w * h) as usize;
    let s = changed_pixels(&shaded);
    let t = changed_pixels(&wire);
    assert!(s > total / 16, "shaded should flood the silhouette: {s}/{total}");
    assert!(t > 100, "wireframe must still draw its (brightened) wire: {t} px");
    assert!(t * 3 < s, "wireframe paints lines, not faces: {t} vs {s}");
}

/// #2818: the two new display modes on a real GPU. "Shaded" fills the same
/// silhouette as "Shaded, with edges" but draws no model wire. "Shaded,
/// with hidden edges" fills it too and adds the wire behind the faces, so
/// it differs from the two others inside the silhouette.
#[test]
fn shaded_drops_the_wire_and_hidden_edges_add_the_far_wire() {
    let cube = [model_part(60.0)];
    let Some((edges, _, _)) = render(&cube, &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let Some((plain, _, _)) = render_mode(&cube, &test_cam(), DisplayMode::Shaded) else {
        return;
    };
    let Some((hidden, _, _)) = render_mode(&cube, &test_cam(), DisplayMode::ShadedHiddenEdges) else {
        return;
    };
    let differ = |a: &[u8], b: &[u8]| a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p != q).count();
    let fill = changed_pixels(&edges);
    for (name, img) in [("Shaded", &plain), ("Shaded, with hidden edges", &hidden)] {
        let n = changed_pixels(img);
        assert!(n * 10 > fill * 9 && n * 9 < fill * 10, "{name} fills the same silhouette: {n} vs {fill}");
    }
    let wire = differ(&edges, &plain);
    assert!(wire > 100, "the front wire is gone in Shaded: {wire} px differ");
    let far = differ(&edges, &hidden);
    assert!(far > 50, "the hidden edges add wire behind the faces: {far} px differ");
    assert!(far < fill / 4, "the hidden edges are lines, not faces: {far} of {fill}");
    assert!(differ(&plain, &hidden) > wire, "hidden edges keep the front wire and add the far wire");
}

/// One frame of `parts` with the shade data of #2834 uploaded and the
/// shading switches `fx`.
fn render_shade(
    parts: &[GpuPart],
    cam: &Camera,
    display: DisplayMode,
    shade: &camcraft_ui::shade_pass::Shade,
    fx: camcraft_ui::draw_list::Effects,
) -> Option<(Vec<u8>, u32, u32)> {
    let _gpu_guard = GPU_TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let (device, queue) = shared_device()?;
    let (device, queue) = (device.as_ref(), queue.as_ref());
    let mut view = View3D::new(device, FORMAT, SIDE, SIDE);
    view.set_document(device, queue, parts);
    view.set_shade(device, queue, shade);
    view.set_effects(fx);
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("gpu-test-shade") });
    view.render(queue, &mut enc, &cam.mvp(1.0), cam.eye_f32(), display);
    queue.submit([enc.finish()]);
    view.snapshot_rgba(device, queue)
}

/// The sum of the colour channels of pixel `ix`.
fn brightness(img: &[u8], ix: usize) -> i32 {
    img[ix * 4..ix * 4 + 3].iter().map(|c| i32::from(*c)).sum()
}

/// #2834: the ground shadow pass on a real GPU. The shadow darkens the
/// ground beside the model, the model hides the part below it, and the
/// pass draws only while its switch is on and the mode is shaded.
#[test]
fn the_ground_shadow_darkens_the_ground_and_not_the_model() {
    use camcraft_ui::draw_list::Effects;
    let cube = [model_part(40.0)];
    let floor = cube[0].verts.iter().map(|v| v[2]).fold(f32::INFINITY, f32::min);
    let shade = camcraft_ui::shade_pass::Shade {
        shadow: Arc::new(camcraft_ui::ground_shadow::mesh(&[&cube[0].verts], floor)),
        ..Default::default()
    };
    assert!(!shade.shadow.is_empty(), "the cube has a shadow mesh");
    let on = Effects { ground_shadow: true, ambient_occlusion: false };
    let Some((plain, w, h)) = render(&cube, &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let Some((shadowed, _, _)) = render_shade(&cube, &test_cam(), DisplayMode::ShadedEdges, &shade, on) else {
        return;
    };
    let total = (w * h) as usize;
    let (mut ground, mut model, mut lighter) = (0usize, 0usize, 0usize);
    for i in 0..total {
        if plain[i * 4..i * 4 + 4] == shadowed[i * 4..i * 4 + 4] {
            continue;
        }
        if off_background(&plain, i) { model += 1 } else { ground += 1 }
        if brightness(&shadowed, i) > brightness(&plain, i) {
            lighter += 1;
        }
    }
    assert!(ground > 100, "the shadow paints a rim on the ground beside the model: {ground} px");
    assert_eq!(lighter, 0, "a shadow never makes a pixel lighter");
    let silhouette = changed_pixels(&plain);
    assert!(model * 50 < silhouette, "the model hides the shadow below it: {model} of {silhouette} px changed");
    // The switch gates the pass: the same buffers, the switch off.
    let Some((off, _, _)) = render_shade(&cube, &test_cam(), DisplayMode::ShadedEdges, &shade, Effects::default()) else {
        return;
    };
    assert!(off == plain, "the shadow pass does not draw while its switch is off");
    // Wireframe draws no shadow.
    let Some((wire, _, _)) = render_mode(&cube, &test_cam(), DisplayMode::Wireframe) else { return };
    let Some((wire_on, _, _)) = render_shade(&cube, &test_cam(), DisplayMode::Wireframe, &shade, on) else { return };
    assert!(wire == wire_on, "wireframe draws no ground shadow");
}

/// #2834: the occlusion pass on a real GPU. A slab with a wall: the baked
/// values darken faces of the model and change nothing outside it; the
/// pass draws only while its switch is on.
#[test]
fn the_occlusion_pass_darkens_model_faces_only() {
    use camcraft_app::geometry::{Point3, Triangle};
    use camcraft_ui::draw_list::Effects;
    let mut verts = soup_of(&StockShape::Box { dx: 60.0, dy: 60.0, dz: 8.0 });
    let mut wall = soup_of(&StockShape::Box { dx: 8.0, dy: 60.0, dz: 40.0 });
    for v in &mut wall {
        v[0] -= 26.0;
        v[2] += 8.0;
    }
    verts.extend(wall);
    let point = |v: &[f32; 3]| Point3::new(f64::from(v[0]), f64::from(v[1]), f64::from(v[2]));
    let tris: Vec<Triangle> = verts.as_chunks::<3>().0.iter().map(|t| Triangle::new(point(&t[0]), point(&t[1]), point(&t[2]))).collect();
    let values = camcraft_ui::occlusion::vertex_occlusion(&tris);
    assert_eq!(values.len(), verts.len());
    assert!(values.iter().any(|v| *v < 0.9), "the foot of the wall is in a corner");
    let part = [GpuPart { verts, selected: false, stock: false, fill: false, classes: Vec::new() }];
    let shade = camcraft_ui::shade_pass::Shade { occlusion: Arc::new(values), ..Default::default() };
    let on = Effects { ground_shadow: false, ambient_occlusion: true };
    let Some((plain, w, h)) = render(&part, &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let Some((occluded, _, _)) = render_shade(&part, &test_cam(), DisplayMode::ShadedEdges, &shade, on) else {
        return;
    };
    let total = (w * h) as usize;
    let (mut ground, mut darker, mut lighter) = (0usize, 0usize, 0usize);
    for i in 0..total {
        if plain[i * 4..i * 4 + 4] == occluded[i * 4..i * 4 + 4] {
            continue;
        }
        if !off_background(&plain, i) {
            ground += 1;
        }
        match brightness(&occluded, i) - brightness(&plain, i) {
            d if d < 0 => darker += 1,
            d if d > 0 => lighter += 1,
            _ => {}
        }
    }
    assert_eq!(ground, 0, "occlusion changes no pixel outside the model");
    assert!(darker > 300, "the corner faces are darker: {darker} px");
    assert_eq!(lighter, 0, "occlusion never makes a pixel lighter");
    let Some((off, _, _)) = render_shade(&part, &test_cam(), DisplayMode::ShadedEdges, &shade, Effects::default()) else {
        return;
    };
    assert!(off == plain, "the occlusion pass does not draw while its switch is off");
    // Values that do not fit the model vertices are never read.
    let short = camcraft_ui::shade_pass::Shade { occlusion: Arc::new(vec![0.5; 9]), ..Default::default() };
    let Some((unread, _, _)) = render_shade(&part, &test_cam(), DisplayMode::ShadedEdges, &short, on) else {
        return;
    };
    assert!(unread == plain, "a buffer of the wrong length is not drawn");
}

#[test]
fn wireframe_hides_wire_behind_solids() {
    // The hidden-line half of the law: the wireframe faces write depth, so
    // a bigger cube farther down the view line may add wire OUTSIDE the
    // front cube's silhouette but never pierce through it. The front
    // cube's shaded render is the mask - faces are exactly its footprint.
    let cam = test_cam();
    let e = cam.eye_f32();
    let t = [0.0f32, 0.0, 20.0]; // the same target test_cam orbits
    let k = 140.0 / 160.0; // push the back cube 140 units past the target
    let back = [t[0] + (t[0] - e[0]) * k, t[1] + (t[1] - e[1]) * k, t[2] + (t[2] - e[2]) * k];
    let front = [model_part(60.0)];
    let both = [model_part(60.0), model_part_at(90.0, back)];
    let Some((mask, w, h)) = render(&front, &cam) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let Some((a, _, _)) = render_mode(&front, &cam, DisplayMode::Wireframe) else {
        return;
    };
    let Some((b, _, _)) = render_mode(&both, &cam, DisplayMode::Wireframe) else {
        return;
    };
    let total = (w * h) as usize;
    let mut inside = 0usize;
    let mut pierced = 0usize;
    let mut outside_added = 0usize;
    for i in 0..total {
        let in_face = off_background(&mask, i);
        if in_face {
            inside += 1;
            if off_background(&a, i) != off_background(&b, i) {
                pierced += 1;
            }
        } else if !off_background(&a, i) && off_background(&b, i) {
            outside_added += 1;
        }
    }
    assert!(inside > total / 16, "the front silhouette should be a real mask");
    assert!(pierced * 200 < inside, "wire must not bleed through solids: {pierced}/{inside}");
    assert!(outside_added > 100, "the far cube must still draw around the near one: {outside_added}");
}

#[test]
fn fill_only_document_uses_no_empty_edge_buffer_slice() {
    let Some((image, _, _)) = render(&[fill_square(12.0, 0.0)], &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    assert!(changed_pixels(&image) > 0, "the fill-only scene should paint");
}

#[test]
fn cut_region_fill_is_translucent() {
    // The same model twice, once with a fill sheet riding just above its
    // top face: the sheet must change pixels (it paints) and every pixel
    // it paints must keep most of what was under it (it blends, never
    // replaces) - the plan's "translucent, not a paint-over".
    let Some((a_img, _, _)) = render(&[model_part(40.0)], &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let Some((b_img, _, _)) = render(&[model_part(40.0), fill_square(12.0, 41.0)], &test_cam())
    else {
        return; // first render found an adapter, this one cannot fail differently
    };
    let mut moved = 0usize;
    let mut see_through = 0usize;
    for (pa, pb) in a_img.chunks(4).zip(b_img.chunks(4)) {
        let d = (pb[0] as i32 - pa[0] as i32).abs()
            + (pb[1] as i32 - pa[1] as i32).abs()
            + (pb[2] as i32 - pa[2] as i32).abs();
        if d > 12 {
            moved += 1;
            // alpha 0.22: a channel can shift at most ~0.22*255 across a
            // full-range boundary, and 40 keeps the test honest against
            // an opaque plate, which would repaint channels far harder.
            if (pb[0] as i32 - pa[0] as i32).abs() < 40
                && (pb[1] as i32 - pa[1] as i32).abs() < 40
                && (pb[2] as i32 - pa[2] as i32).abs() < 40
            {
                see_through += 1;
            }
        }
    }
    assert!(moved > 200, "the fill sheet painted {moved} pixels; want a visible footprint");
    assert!(
        see_through * 2 > moved,
        "fill must blend over what it covers, not replace it: {see_through}/{moved} gentle"
    );
}

// ---------------------------------------------------------------------------
// The tutorial pictures in `docs/images/`, kept honest by rendering them.
//
// The law (#291's docs lane): every screenshot a tutorial carries is a
// rendered frame of a committed example project - `examples/first-part.
// camproj` opened through the real open command, drawn by the real
// viewport pipelines with the real toolpath display (`interact::
// toolpath_lines`), never a hand-captured picture that rots in silence.
// Each step along `setup::operation_order` reveals one more operation's
// path, so the sequence reads exactly like the app's Show-all order.
//
// The pictures are golden files: a normal test run compares the rendered
// frames against the committed PNGs and fails on any difference (same
// per-channel epsilon the software-driver pixel checks above use);
// `UPDATE_DOCS_IMAGES=1 cargo test -p camcraft-ui --test gpu_render`
// re-renders and rewrites them, the same workflow as the kittest snapshot
// gate. That is the whole refresh story: regenerate, eyeball the diff in
// the PR, commit.

const DOC_W: u32 = 800;
const DOC_H: u32 = 600;

/// Per-channel tolerance against the committed picture.
const DOC_EPS: i32 = 6;

/// The app's own f64→f32 guard (`interact::ftoz`, duplicated because it
/// is crate-private; a bare cast would differ only on non-finite input).
fn ftoz(v: f64) -> f32 {
    if v.is_finite() { v as f32 } else { 0.0 }
}

/// World-space triangle soup for one entity - `egui_app::world.rs`'s
/// `world_verts` law: facets, then the active setup's alignment, then
/// the z-forward cast the viewport expects.
fn soup(state: &State, entity: &flecs_ecs::prelude::Entity) -> Vec<[f32; 3]> {
    let tris = model::facets_of(&state.world, entity).unwrap_or_default();
    let loc = setup_align::view_pose(&state.world, entity).unwrap_or_default();
    let mut verts = Vec::with_capacity(tris.len() * 3);
    for t in tris.iter() {
        for p in [t.a, t.b, t.c] {
            let p = transform_point(p, loc.pos, loc.rot);
            verts.push([ftoz(p.x), ftoz(p.y), ftoz(p.z)]);
        }
    }
    verts
}

/// The frame's parts in draw order: the models, then the active setup's
/// stock block (the no-sim, no-field-view arm of `collect_parts` - the
/// docs steps have neither by construction). No fills: no region is
/// picked on screen, so there is no fill to show.
fn doc_parts(state: &State) -> Vec<GpuPart> {
    let mut parts: Vec<GpuPart> = model::model_entities(&state.world)
        .into_iter()
        .filter(|(name, _)| !state.hidden_models.iter().any(|h| h == name))
        .map(|(_, entity)| GpuPart {
            verts: soup(state, &entity),
            selected: false,
            stock: false,
            fill: false,
            classes: Vec::new(),
        })
        .collect();
    let stock_name = setup::stock_entity_name(&state.world, setup::active(&state.world).as_deref());
    if let Some(entity) = model::find_by_name(&state.world, &stock_name)
        && model::is_stock(&state.world, &entity)
    {
        parts.push(GpuPart {
            verts: soup(state, &entity),
            selected: false,
            stock: true,
            fill: false,
            classes: Vec::new(),
        });
    }
    parts
}

/// One docs frame, rendered and read back as an RGBA image.
fn render_docs_frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    parts: &[GpuPart],
    overlay: &[[f32; 6]],
    cam: &Camera,
) -> Option<RgbaImage> {
    let mut view = View3D::new(device, FORMAT, DOC_W, DOC_H);
    view.set_document(device, queue, parts);
    view.set_overlay(device, queue, overlay);
    let mvp = cam.mvp(f64::from(DOC_W) / f64::from(DOC_H));
    let eye = cam.eye_f32();
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("docs-image-frame"),
    });
    view.render(queue, &mut enc, &mvp, eye, DisplayMode::ShadedEdges);
    queue.submit([enc.finish()]);
    let (rgba, w, h) = view.snapshot_rgba(device, queue)?;
    RgbaImage::from_raw(w, h, rgba)
}

/// A file-name-safe step label from an operation name ("2D Pocket1" →
/// "2d-pocket1").
fn slug(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

/// Encode like a PNG file writes one (same encoder, same defaults, so the
/// committed golden and the fresh frame differ only in what was drawn).
fn png_bytes(img: &RgbaImage) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png).expect("encode PNG");
    buf.into_inner()
}

/// Compare a fresh frame against the committed picture, counting pixels
/// that differ beyond `DOC_EPS` per channel. `None` = no committed picture.
fn diff_against(committed: &Path, fresh: &RgbaImage) -> Option<Result<(), (u64, String)>> {
    let Ok(golden) = image::open(committed) else { return None };
    let golden = golden.to_rgba8();
    if golden.dimensions() != fresh.dimensions() {
        return Some(Err((
            u64::from(DOC_W) * u64::from(DOC_H),
            format!(
                "size {}x{} vs committed {}x{}",
                DOC_W, DOC_H, golden.width(), golden.height()
            ),
        )));
    }
    let mut bad = 0_u64;
    let (fresh, golden) = (fresh.as_raw(), golden.as_raw());
    for (a, b) in fresh.as_chunks::<4>().0.iter().zip(golden.as_chunks::<4>().0.iter()) {
        let differs = a.iter().zip(b).any(|(x, y)| {
            i32::from(*x) - i32::from(*y) > DOC_EPS || i32::from(*y) - i32::from(*x) > DOC_EPS
        });
        if differs {
            bad += 1;
        }
    }
    if bad == 0 { Some(Ok(())) } else { Some(Err((bad, String::new()))) }
}

/// The example project opened through the real command, plus the step series
/// (label, operations whose eyes are off) shared by the structural test and
/// the pixel snapshot. Returns the picture directory too.
fn docs_steps() -> (State, Vec<(String, Vec<String>)>, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = root.join("examples/first-part.camproj");
    assert!(project.is_file(), "the example project is committed at {project:?}");
    let out_dir = root.join("docs/images");

    // Open the example through the real command - the same code path the
    // File > Open menu runs.
    let mut state = State::new();
    state.pending.push_back(CommandKind::OpenProjectFile(project.clone()));
    state.drain_actions();
    assert!(
        state.status.starts_with("opened"),
        "the example opens clean: {}",
        state.status
    );

    let ops: Vec<String> = setup::operation_order(&state.world)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(!ops.is_empty(), "the example carries operations to step through");
    for name in &ops {
        let entity = model::find_by_name(&state.world, name).expect("op exists");
        assert!(
            model::toolpath_of(&state.world, &entity).is_some(),
            "{name} carries a generated toolpath in the committed project"
        );
    }


    // Step law: step 0 shows the model and the stock blank; step n reveals
    // the first n operations' paths, in operation order. Paths appear
    // through the app's own display state (Show all + per-op eyes), never
    // by editing geometry.
    let steps: Vec<(String, Vec<String>)> = (0..=ops.len())
        .map(|shown| {
            let label = if shown == 0 { "blank".to_string() } else { ops[shown - 1].clone() };
            (label, ops[shown..].to_vec())
        })
        .collect();
    (state, steps, out_dir)
}

/// Non-pixel half of the docs pictures (#854): the example opens, every
/// operation carries a toolpath, and each step draws its overlay. Runs in
/// the default suite on every OS; the pixel compare is the snapshot below.
#[test]
fn docs_example_steps_through_operations() {
    let (mut state, steps, _out_dir) = docs_steps();
    assert!(steps.len() > 1, "the series has the blank plus one step per operation");
    for (n, (_label, hidden)) in steps.iter().enumerate() {
        state.show_all_paths = n > 0;
        state.hidden_paths = hidden.clone();
        if n > 0 {
            assert!(
                !interact::toolpath_lines(&state).is_empty(),
                "step {n} draws at least one toolpath"
            );
        }
    }
}

/// Pixel goldens rendered on one OS/GPU drift on another adapter, so this is
/// release-lane (#255, #854): `#[ignore]`d out of quick/batch, run by the
/// release tier's `gpu_render` snapshot step (`--ignored snapshot`).
#[test]
#[ignore = "pixel goldens: release-tier ui snapshot lane (--test gpu_render --ignored snapshot)"]
fn docs_images_snapshot_match_the_rendered_steps() {
    let (mut state, steps, out_dir) = docs_steps();
    // One fixed camera for the whole series: the 100x60x20 mm stock fills
    // most of an 800x600 frame, and the raised target keeps the rapids'
    // clearance height (25 mm over the stock top) inside the picture.
    let cam = Camera {
        target: [0.0, 0.0, 10.0],
        yaw: 0.9,
        pitch: 0.42,
        dist: 150.0,
        // The goldens were rendered with the house perspective; #434's
        // field must stay spelled out here so the pictures keep their framing.
        projection: ProjectionMode::Perspective,
        frame: camcraft_ui::view_frame::ViewFrame::Mill,
    };

    let update = std::env::var_os("UPDATE_DOCS_IMAGES").is_some_and(|v| !v.is_empty());
    if update {
        fs::create_dir_all(&out_dir).expect("create docs/images");
    }

    // All frames draw on this binary's one shared device, under the GPU
    // lock - the same reason the frames above hold it.
    let _gpu_guard = GPU_TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some((device, queue)) = shared_device() else {
        eprintln!("docs images: no usable GPU adapter (even software) - skipping");
        return;
    };
    let (device, queue) = (device.as_ref(), queue.as_ref());

    let mut failures: Vec<String> = Vec::new();
    for (n, (after_op, hidden)) in steps.iter().enumerate() {
        state.show_all_paths = n > 0;
        state.hidden_paths = hidden.clone();
        let parts = doc_parts(&state);
        let overlay = interact::toolpath_lines(&state);
        if n > 0 {
            assert!(!overlay.is_empty(), "step {n} draws at least one toolpath");
        }
        let Some(frame) = render_docs_frame(device, queue, &parts, &overlay, &cam) else {
            eprintln!("docs images: readback failed - skipping");
            return;
        };
        let name = format!("first-part-{n}-{}.png", slug(after_op));
        let path = out_dir.join(&name);
        match diff_against(&path, &frame) {
            Some(Ok(())) => {}
            Some(Err((bad, why))) => {
                if update {
                    fs::write(&path, png_bytes(&frame)).expect("write the picture");
                } else if why.is_empty() {
                    failures.push(format!("{name}: {bad} pixels differ"));
                } else {
                    failures.push(format!("{name}: {why}"));
                }
            }
            None => {
                if update {
                    fs::write(&path, png_bytes(&frame)).expect("write the picture");
                } else {
                    failures.push(format!("{name}: no committed picture ({path:?})"));
                }
            }
        }
    }
    drop(_gpu_guard);

    assert!(
        failures.is_empty(),
        "docs pictures drifted from the rendered steps (regenerate with \
         UPDATE_DOCS_IMAGES=1 cargo test -p camcraft-ui --test gpu_render -- --ignored snapshot):\n{}",
        failures.join("\n")
    );
}

#[test]
fn orthographic_paints_the_band_perspective_does() {
    // #600: the ortho matrix mapped depth OpenGL-style, into negative
    // clip z, and WebGPU's 0 <= z <= w rule discarded every triangle -
    // the viewport went blank the moment Orthographic was clicked.
    // The same part and camera must paint a comparable band in both
    // modes; the framing is built to match at the target plane.
    let parts = [stock_part(&StockShape::Box {
        dx: 60.0,
        dy: 60.0,
        dz: 40.0,
    })];
    let Some((persp_img, w, h)) = render(&parts, &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let ortho_cam = Camera {
        projection: ProjectionMode::Orthographic,
        ..test_cam()
    };
    let Some((ortho_img, _, _)) = render(&parts, &ortho_cam) else {
        return; // the perspective render found an adapter; this one cannot differ
    };
    let total = (w * h) as usize;
    let p = changed_pixels(&persp_img);
    let o = changed_pixels(&ortho_img);
    assert!(o > total / 20 && o < total - total / 20, "ortho should paint a solid band, not {o}/{total}");
    assert!(o * 2 < p * 3 && p * 2 < o * 3, "the modes frame alike at dist: {o} ortho vs {p} perspective");
}
