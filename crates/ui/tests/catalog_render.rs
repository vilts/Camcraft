//! Off-screen renderer hook for the example catalog (#2392, #2393), matching
//! the `View3D` + `headless_gpu` pipeline used by `gpu_render` and #1902.
//!
//! Provenance: our own off-screen capture harness over `camcraft_ui::viewport::View3D`,
//! `camcraft_ui::interact::toolpath_lines`, and `camcraft_ui::headless_gpu`.
//! Gate map (#2393): covers ui::viewport, ui::interact, ui::headless_gpu,
//! ui::state, app::project, app::model, app::setup, app::setup_align.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use camcraft_app::geometry::transform_point;
use camcraft_app::{model, setup, setup_align};
use camcraft_ui::interact;
use camcraft_ui::state::{CommandKind, DisplayMode, State};
use camcraft_ui::viewport::{Camera, GpuPart, View3D};
use egui_wgpu::wgpu;
use image::{ImageFormat, RgbaImage};

const SHOT_W: u32 = 1200;
const SHOT_H: u32 = 760;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

struct CatalogProject {
    family: String,
    name: String,
    project: PathBuf,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn discover_projects(root: &Path) -> Vec<CatalogProject> {
    let catalog = root.join("examples/catalog");
    if !catalog.is_dir() {
        return Vec::new();
    }
    let mut families: Vec<PathBuf> = fs::read_dir(&catalog)
        .unwrap_or_else(|e| panic!("read {}: {e}", catalog.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    families.sort();
    let mut out = Vec::new();
    for fam in families {
        let family = fam.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
        let mut cases: Vec<PathBuf> = fs::read_dir(&fam)
            .unwrap_or_else(|e| panic!("read {}: {e}", fam.display()))
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        cases.sort();
        for case_dir in cases {
            let name = case_dir.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
            let project = case_dir.join(format!("{name}.camproj"));
            if project.is_file() {
                out.push(CatalogProject { family: family.clone(), name, project });
            }
        }
    }
    out
}

fn ftoz(v: f64) -> f32 {
    if v.is_finite() { v as f32 } else { 0.0 }
}

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

fn scene_parts(state: &State) -> Vec<GpuPart> {
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

fn open_and_frame(project: &Path) -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::OpenProjectFile(project.to_path_buf()));
    state.drain_actions();
    assert!(
        state.status.starts_with("opened"),
        "{} failed to open: {}",
        project.display(),
        state.status
    );
    state.show_all_paths = true;
    state
}

fn resolve_out_dir(root: &Path) -> PathBuf {
    let dir = std::env::var_os("CAMCRAFT_CATALOG_SCREENSHOTS_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("camcraft-catalog-screenshots"));
    let check = dir.canonicalize().unwrap_or_else(|_| dir.clone());
    assert!(
        !check.starts_with(root),
        "catalog screenshots must write outside the repository tree (got {})",
        check.display()
    );
    dir
}

fn render_png(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    parts: &[GpuPart],
    overlay: &[[f32; 6]],
    cam: &Camera,
) -> Option<Vec<u8>> {
    let mut view = View3D::new(device, FORMAT, SHOT_W, SHOT_H);
    view.set_document(device, queue, parts);
    view.set_overlay(device, queue, overlay);
    let mvp = cam.mvp(f64::from(SHOT_W) / f64::from(SHOT_H));
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("catalog-screenshot-frame"),
    });
    view.render(queue, &mut enc, &mvp, cam.eye_f32(), DisplayMode::ShadedEdges);
    queue.submit([enc.finish()]);
    let (rgba, w, h) = view.snapshot_rgba(device, queue)?;
    let bg = rgba.first_chunk::<4>().copied()?;
    let non_bg = rgba.as_chunks::<4>().0.iter().filter(|px| **px != bg).count();
    assert!(non_bg > 200, "rendered catalog screenshot is blank ({non_bg} non-bg pixels)");
    let img = RgbaImage::from_raw(w, h, rgba)?;
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png).expect("encode PNG");
    Some(buf.into_inner())
}

#[test]
fn catalog_cases_open_and_frame_cleanly() {
    let root = repo_root();
    let cases = discover_projects(&root);
    assert!(!cases.is_empty(), "expected at least one catalog project under examples/catalog/");
    for case in &cases {
        let state = open_and_frame(&case.project);
        assert!(state.cam.dist.is_finite() && state.cam.dist > 0.0, "{}/{}: invalid cam", case.family, case.name);
        assert!(!scene_parts(&state).is_empty(), "{}/{}: empty scene parts", case.family, case.name);
        assert!(!interact::toolpath_lines(&state).is_empty(), "{}/{}: empty toolpath overlay", case.family, case.name);
    }
}

#[test]
#[ignore = "off-screen WGPU catalog screenshot renderer invoked by `cargo xtask catalog --screenshots`"]
fn render_all_catalog_screenshots() {
    let root = repo_root();
    let out_dir = resolve_out_dir(&root);
    let filter = std::env::var("CAMCRAFT_CATALOG_CASE").ok();
    let cases: Vec<CatalogProject> = discover_projects(&root)
        .into_iter()
        .filter(|c| filter.as_deref().is_none_or(|want| format!("{}/{}", c.family, c.name) == want))
        .collect();
    assert!(!cases.is_empty(), "no catalog cases matched filter {filter:?}");

    let gpu = match camcraft_ui::headless_gpu::device_for_test("catalog_render") {
        Ok(Some(g)) => g,
        Ok(None) => return,
        Err(e) => panic!("{e}"),
    };

    for case in &cases {
        let state = open_and_frame(&case.project);
        let parts = scene_parts(&state);
        let overlay = interact::toolpath_lines(&state);
        let bytes = render_png(&gpu.device, &gpu.queue, &parts, &overlay, &state.cam)
            .unwrap_or_else(|| panic!("readback failed for {}/{}", case.family, case.name));
        let dest = out_dir.join(&case.family).join(&case.name).join(format!("{}.png", case.name));
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).expect("create screenshot output dir");
        }
        fs::write(&dest, bytes).unwrap_or_else(|e| panic!("write {}: {e}", dest.display()));
        println!("rendered {}/{} -> {}", case.family, case.name, dest.display());
    }
}
