//! Example catalog contract (#2392, #2393): every case under
//! `examples/catalog/<family>/<case>/` carries its model generator/mesh,
//! `build.camscript`, `<case>.camproj`, `<case>.nc`, and `README.md`.
//! Each case opens, regenerates every operation, verifies clean, and posts
//! the committed `.nc` byte for byte; the seed case rebuilds end to end
//! from `build.camscript`.
//!
//! Gate map (#2393): covers app::commands, app::model, app::operation,
//! app::posting, app::project, app::script, app::mcp, app::cli,
//! app::setup, app::world, post::post, sim::verify.
//! Provenance: clean-room test suite over CamCraft's own commands, script
//! host, and example catalog cases.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use camcraft_app::commands::{GenerateOperation, History};
use camcraft_app::operation::OpStatus;
use camcraft_app::{model, operation, posting, project, setup, world};

struct CatalogCase {
    family: String,
    name: String,
    dir: PathBuf,
}

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

fn discover_cases(catalog_root: &Path) -> Vec<CatalogCase> {
    assert!(
        catalog_root.is_dir(),
        "example catalog directory missing: {}",
        catalog_root.display()
    );
    let mut families: Vec<PathBuf> = fs::read_dir(catalog_root)
        .unwrap_or_else(|e| panic!("read {}: {e}", catalog_root.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    families.sort();

    let mut cases = Vec::new();
    for fam_dir in families {
        let family = fam_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_owned();
        let mut children: Vec<PathBuf> = fs::read_dir(&fam_dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", fam_dir.display()))
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        children.sort();
        for dir in children {
            let name = dir
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_owned();
            cases.push(CatalogCase { family: family.clone(), name, dir });
        }
    }
    cases
}

fn assert_case_layout(case: &CatalogCase) {
    let model_dir = case.dir.join("model");
    assert!(model_dir.is_dir(), "{}/{}: missing model/", case.family, case.name);
    let model_entries: Vec<_> = fs::read_dir(&model_dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", model_dir.display()))
        .filter_map(Result::ok)
        .collect();
    assert!(
        !model_entries.is_empty(),
        "{}/{}: model/ must hold generator and mesh",
        case.family,
        case.name
    );
    for req in [
        "build.camscript".to_owned(),
        format!("{}.camproj", case.name),
        format!("{}.nc", case.name),
        "README.md".to_owned(),
    ] {
        let p = case.dir.join(&req);
        assert!(p.is_file(), "{}/{}: missing {}", case.family, case.name, req);
        assert!(
            fs::metadata(&p).map(|m| m.len()).unwrap_or(0) > 0,
            "{}/{}: empty {}",
            case.family,
            case.name,
            req
        );
    }
}

fn copy_dir_all(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap_or_else(|e| panic!("mkdir {}: {e}", dst.display()));
    for entry in fs::read_dir(src).unwrap_or_else(|e| panic!("read {}: {e}", src.display())) {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &target);
        } else {
            fs::copy(&path, &target)
                .unwrap_or_else(|e| panic!("copy {} -> {}: {e}", path.display(), target.display()));
        }
    }
}

#[test]
fn every_catalog_case_opens_regenerates_verifies_and_posts_byte_for_byte() {
    let cases = discover_cases(&repo("examples/catalog"));
    assert!(
        cases.iter().any(|c| c.family == "2d" && c.name == "first-part"),
        "seed case 2d/first-part must be present under examples/catalog/"
    );
    for case in &cases {
        assert_case_layout(case);
        let label = format!("{}/{}", case.family, case.name);
        let proj_path = case.dir.join(format!("{}.camproj", case.name));
        let nc_path = case.dir.join(format!("{}.nc", case.name));

        let w = world::new_world();
        let notes = project::load_noted(&w, &proj_path)
            .unwrap_or_else(|e| panic!("{label}: load {}: {e}", proj_path.display()));
        assert!(notes.is_empty(), "{label}: unexpected load notes: {notes:?}");

        let ops = setup::operation_order(&w);
        assert!(!ops.is_empty(), "{label}: project has no operations");

        let mut history = History::default();
        for (op_name, _) in &ops {
            let cmd = GenerateOperation::new(&w, op_name)
                .unwrap_or_else(|e| panic!("{label}: regenerate {op_name}: {e}"));
            history.execute(&w, cmd);
        }

        for (op_name, entity) in setup::operation_order(&w) {
            let status = operation::status_of(&w, &entity);
            assert_eq!(status, OpStatus::Ok, "{label}: {op_name} status after regenerate");
            if let Some(tp) = model::toolpath_of(&w, &entity) {
                let blocking: Vec<String> = tp
                    .warnings
                    .iter()
                    .filter(|warn| warn.blocks_post())
                    .map(ToString::to_string)
                    .collect();
                assert!(
                    blocking.is_empty(),
                    "{label}: {op_name} holds blocking warnings: {blocking:?}"
                );
            }
        }

        let report = posting::verification_report(&w);
        assert!(
            report.problems.is_empty(),
            "{label}: verification failed: {:?}",
            report.problems
        );

        let posted = posting::post_setup_unwritten(&w)
            .unwrap_or_else(|e| panic!("{label}: post failed: {e}"));
        let want = fs::read(&nc_path)
            .unwrap_or_else(|e| panic!("{label}: read {}: {e}", nc_path.display()));
        assert_eq!(
            posted.program.as_bytes(),
            want.as_slice(),
            "{label}: regenerated NC differs from {}",
            nc_path.display()
        );
    }
}

#[test]
fn every_catalog_case_rebuilds_from_camscript() {
    let cases = discover_cases(&repo("examples/catalog"));
    assert!(!cases.is_empty(), "at least one catalog case required");

    let scratch = std::env::temp_dir().join(format!("camcraft-catalog-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);

    for case in &cases {
        let label = format!("{}/{}", case.family, case.name);
        let work = scratch.join(&case.family).join(&case.name);
        copy_dir_all(&case.dir, &work);

        let proj = work.join(format!("{}.camproj", case.name));
        let nc = work.join(format!("{}.nc", case.name));
        let _ = fs::remove_file(&proj);
        let _ = fs::remove_file(&nc);

        let output = Command::new(env!("CARGO_BIN_EXE_camcraft-cli"))
            .arg("script")
            .arg("build.camscript")
            .current_dir(&work)
            .output()
            .unwrap_or_else(|e| panic!("{label}: run camcraft-cli: {e}"));

        assert!(
            output.status.success(),
            "{label}: build.camscript failed (exit {:?}):\nstdout: {}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(proj.is_file(), "{label}: build.camscript did not write {}", proj.display());
        assert!(nc.is_file(), "{label}: build.camscript did not write {}", nc.display());

        let want_nc = fs::read(case.dir.join(format!("{}.nc", case.name)))
            .unwrap_or_else(|e| panic!("{label}: read committed .nc: {e}"));
        let got_nc = fs::read(&nc)
            .unwrap_or_else(|e| panic!("{label}: read rebuilt .nc: {e}"));
        assert_eq!(
            got_nc, want_nc,
            "{label}: build.camscript NC differs from committed .nc"
        );
    }

    let _ = fs::remove_dir_all(&scratch);
}
