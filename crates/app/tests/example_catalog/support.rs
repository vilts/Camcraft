// Provenance: original CamCraft catalog assertions, split for per-case results (#2857).
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use camcraft_app::commands::{GenerateOperation, History};
use camcraft_app::operation::OpStatus;
use camcraft_app::{model, operation, posting, project, setup, world};

pub(super) struct CatalogCase {
    pub(super) family: String,
    pub(super) name: String,
    pub(super) dir: PathBuf,
}

pub(super) fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

pub(super) fn discover_cases(catalog_root: &Path) -> Vec<CatalogCase> {
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
            cases.push(CatalogCase {
                family: family.clone(),
                name,
                dir,
            });
        }
    }
    cases
}

pub(super) fn assert_case_layout(case: &CatalogCase) {
    let model_dir = case.dir.join("model");
    assert!(
        model_dir.is_dir(),
        "{}/{}: missing model/",
        case.family,
        case.name
    );
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
    // #2856: a refusal case records its refusal lines, not a program.
    let refused = super::refusal::expects_refusal(&case.dir);
    let result = if refused { "refusal.txt".to_owned() } else { format!("{}.nc", case.name) };
    if refused {
        let nc = case.dir.join(format!("{}.nc", case.name));
        assert!(!nc.exists(), "{}/{}: a refusal case has no {}", case.family, case.name, nc.display());
    }
    for req in [
        "build.camscript".to_owned(),
        format!("{}.camproj", case.name),
        result,
        "README.md".to_owned(),
    ] {
        let p = case.dir.join(&req);
        assert!(
            p.is_file(),
            "{}/{}: missing {}",
            case.family,
            case.name,
            req
        );
        assert!(
            fs::metadata(&p).map(|m| m.len()).unwrap_or(0) > 0,
            "{}/{}: empty {}",
            case.family,
            case.name,
            req
        );
    }
}

pub(super) fn copy_dir_all(src: &Path, dst: &Path) {
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

// Portable examples embed their meshes. A relative source link makes the UI
// reopen a file from the process directory instead of the case directory.
pub(super) fn assert_portable_models(case: &CatalogCase, w: &flecs_ecs::prelude::World) {
    let names: &[&str] = match (case.family.as_str(), case.name.as_str()) {
        ("2d", "contour-tabs") => &["contour-tabs"],
        ("turning", "cone-thread") => &["cone-blank", "cone-thread"],
        ("2d", "drill-overfeed") => &["drill-overfeed"],
        _ => return,
    };
    for name in names {
        let entity = model::find_by_name(w, name).expect("catalog model must be present");
        assert!(
            camcraft_app::model_source::of(w, &entity).is_none(),
            "{}/{}: {name} must open without a link to the script working directory",
            case.family,
            case.name
        );
        assert!(
            model::facets_of(w, &entity).is_some_and(|facets| !facets.is_empty()),
            "{}/{}: {name} must keep its embedded mesh",
            case.family,
            case.name
        );
    }
}

pub(super) fn regenerate(case: &CatalogCase) {
    if super::refusal::expects_refusal(&case.dir) {
        return super::refusal::regenerate(case);
    }
    let label = format!("{}/{}", case.family, case.name);
    let proj_path = case.dir.join(format!("{}.camproj", case.name));
    let nc_path = case.dir.join(format!("{}.nc", case.name));

    let w = world::new_world();
    let notes = project::load_noted(&w, &proj_path)
        .unwrap_or_else(|e| panic!("{label}: load {}: {e}", proj_path.display()));
    assert!(
        notes.is_empty(),
        "{label}: unexpected load notes: {notes:?}"
    );
    assert_portable_models(case, &w);

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
        assert_eq!(
            status,
            OpStatus::Ok,
            "{label}: {op_name} status after regenerate"
        );
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

    let posted =
        posting::post_setup_unwritten(&w).unwrap_or_else(|e| panic!("{label}: post failed: {e}"));
    let want =
        fs::read(&nc_path).unwrap_or_else(|e| panic!("{label}: read {}: {e}", nc_path.display()));
    assert_eq!(
        posted.program.as_bytes(),
        want.as_slice(),
        "{label}: regenerated NC differs from {}",
        nc_path.display()
    );
}

pub(super) fn rebuild(case: &CatalogCase) {
    if super::refusal::expects_refusal(&case.dir) {
        return super::refusal::rebuild(case);
    }
    let label = format!("{}/{}", case.family, case.name);
    let work = std::env::temp_dir()
        .join(format!("camcraft-catalog-test-{}", std::process::id()))
        .join(&case.family)
        .join(&case.name);
    let _ = fs::remove_dir_all(&work);
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
    assert!(
        proj.is_file(),
        "{label}: build.camscript did not write {}",
        proj.display()
    );
    assert!(
        nc.is_file(),
        "{label}: build.camscript did not write {}",
        nc.display()
    );

    // Load outside the script's directory. Pin the rebuilt example too;
    // checking the committed project alone cannot catch a linked rebuild.
    if matches!(
        (case.family.as_str(), case.name.as_str()),
        ("2d", "contour-tabs") | ("turning", "cone-thread")
    ) {
        let w = world::new_world();
        let notes = project::load_noted(&w, &proj)
            .unwrap_or_else(|e| panic!("{label}: load rebuilt project: {e}"));
        assert!(
            notes.is_empty(),
            "{label}: rebuilt project load notes: {notes:?}"
        );
        assert_portable_models(case, &w);
    }

    let want_nc = fs::read(case.dir.join(format!("{}.nc", case.name)))
        .unwrap_or_else(|e| panic!("{label}: read committed .nc: {e}"));
    let got_nc = fs::read(&nc).unwrap_or_else(|e| panic!("{label}: read rebuilt .nc: {e}"));
    assert_eq!(
        got_nc, want_nc,
        "{label}: build.camscript NC differs from committed .nc"
    );
    let _ = fs::remove_dir_all(&work);
}

// Run the same bounded worker pattern as import_step_guard. Keep a panic's
// original payload, so a failed case is not reported as a timeout.
pub(super) fn within(label: &str, budget: std::time::Duration, f: impl FnOnce() + Send + 'static) {
    use std::io::Write;
    use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
    let start = std::time::Instant::now();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(catch_unwind(AssertUnwindSafe(f)));
    });
    let result = rx.recv_timeout(budget.saturating_sub(start.elapsed()));
    // Direct stderr keeps successful timings visible with libtest capture on.
    // One write for the whole line (#3043): stderr has no buffer, so a
    // formatted write goes out in pieces, and the panic message of a parallel
    // case thread can split the line between those pieces.
    let line = timing_line(label, start.elapsed(), budget);
    let _ = std::io::stderr().lock().write_all(line.as_bytes());
    match result {
        Ok(Ok(())) => {}
        Ok(Err(payload)) => resume_unwind(payload),
        Err(_) => panic!("{label}: did not finish within {budget:?}; move this case to app heavy: example_catalog or reduce its model"),
    }
}

// The full timing line of one case, with its newline, for one stderr write.
fn timing_line(
    label: &str,
    elapsed: std::time::Duration,
    budget: std::time::Duration,
) -> String {
    format!(
        "catalog {label} elapsed={:.3}s budget={:.3}s\n",
        elapsed.as_secs_f64(),
        budget.as_secs_f64()
    )
}

pub(super) fn run_case(family: &str, name: &str, action: &str, run: fn(&CatalogCase)) {
    let root = std::env::var_os("CAMCRAFT_TEST_CATALOG_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo("examples/catalog"));
    let case = CatalogCase {
        family: family.into(),
        name: name.into(),
        dir: root.join(family).join(name),
    };
    within(
        &format!("{family}/{name} {action}"),
        std::time::Duration::from_secs(60),
        move || {
            assert_case_layout(&case);
            run(&case);
        },
    );
}

#[test]
fn a_case_over_its_budget_fails_by_name() {
    let result = std::panic::catch_unwind(|| {
        within(
            "slow-case regenerates",
            std::time::Duration::from_millis(1),
            || {
                std::thread::sleep(std::time::Duration::from_millis(50));
            },
        );
    });
    let payload = result.expect_err("a case over its budget must fail");
    let message = payload
        .downcast_ref::<String>()
        .expect("named budget failure");
    assert!(
        message.contains("slow-case regenerates: did not finish within 1ms"),
        "{message}"
    );
    assert!(message.contains("app heavy: example_catalog"), "{message}");
}

#[test]
fn timing_line_is_one_whole_line() {
    let line = timing_line(
        "2d/first-part regenerates",
        std::time::Duration::from_millis(1234),
        std::time::Duration::from_secs(60),
    );
    assert_eq!(
        line,
        "catalog 2d/first-part regenerates elapsed=1.234s budget=60.000s\n"
    );
    assert_eq!(line.matches('\n').count(), 1, "{line:?}");
}
