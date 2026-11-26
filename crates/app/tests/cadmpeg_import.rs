//! The cadmpeg runner (#1873): a native CAD file is converted to STEP by
//! the external `cadmpeg` program inside a private folder, under a time
//! limit and size limits, and the STEP text then passes the STEP importer.
//! Every refusal is by name and leaves no folder behind. The stand-in
//! program is `testdata/stubs/cadmpeg_stub.sh`; no cadmpeg is needed.
//!
//! Provenance: our own tests, specced from the issue's "Done means" and
//! the public command line of cadmpeg 0.6.12 (its `--help` text).
//!
//! Gate map (M4.4): covers app::geometry::import (cadmpeg, native),
//! app::commands (import_model).

use std::path::Path;

use camcraft_app::commands::{CommandError, ImportModel};
use camcraft_app::geometry::import::native::{self, Route};

/// The qualified table is empty until #1875 enables a format, so every
/// native extension is refused by its own name - `.prt` included - and
/// never by a failed read.
#[test]
fn unqualified_native_extensions_are_refused_by_name() {
    assert!(native::QUALIFIED.is_empty(), "no format is qualified before #1875");
    for (file, word) in [("a.iges", "IGES"), ("a.IGS", "IGS"), ("a.prt", "PRT"), ("a.sldprt", "SLDPRT"), ("a.f3d", "F3D")] {
        assert_eq!(native::route(Path::new(file)), Route::Unqualified, "{file}");
        let err = ImportModel::new(Path::new(file)).err();
        assert!(matches!(err, Some(CommandError::UnsupportedType(ref t)) if t == word), "{file}: {err:?}");
        let bytes = ImportModel::from_bytes(file, b"anything", None).err();
        assert!(matches!(bytes, Some(CommandError::UnsupportedType(ref t)) if t == word), "{file}: {bytes:?}");
    }
    for file in ["a.step", "a.stl", "a.obj", "a.sc", "a.txt", "a"] {
        assert_eq!(native::route(Path::new(file)), Route::Direct, "{file}");
    }
}

/// A table with an entry routes that extension to the converter, in any
/// letter case, and leaves the other native extensions refused.
#[test]
fn a_qualified_extension_routes_to_the_converter() {
    let table = ["sldprt"];
    assert_eq!(native::route_in(&table, Path::new("Bracket.SLDPRT")), Route::Converted);
    assert_eq!(native::route_in(&table, Path::new("bracket.prt")), Route::Unqualified);
    assert_eq!(native::route_in(&table, Path::new("bracket.step")), Route::Direct);
}

#[cfg(unix)]
mod stub {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use camcraft_app::commands::CommandError;
    use camcraft_app::geometry::ImportError;
    use camcraft_app::geometry::import::cadmpeg::{self, Settings};

    /// One test's folders: `bin` holds the stand-in and its files, `tmp`
    /// is where the runner makes its private folders.
    struct Bench {
        root: PathBuf,
    }

    impl Bench {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir().join(format!("camcraft_cadmpeg_test_{tag}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("bin")).expect("bin folder");
            std::fs::create_dir_all(root.join("tmp")).expect("tmp folder");
            let stub = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/stubs/cadmpeg_stub.sh");
            std::os::unix::fs::symlink(stub, root.join("bin/cadmpeg")).expect("link the stand-in");
            let bench = Bench { root };
            bench.put("version", "cadmpeg 0.6.12\n");
            bench.put("check.json", r#"{"check_report":{"findings":[{"severity":"warning","message":"ring order"}]}}"#);
            std::fs::copy(model("box.step"), bench.root.join("bin/convert.step")).expect("the STEP the stand-in writes");
            std::fs::write(bench.input(), b"not a real part").expect("the input file");
            bench
        }

        fn put(&self, name: &str, text: &str) {
            std::fs::write(self.root.join("bin").join(name), text).expect("a stand-in file");
        }

        fn input(&self) -> PathBuf {
            self.root.join("my part.sldprt")
        }

        /// The stand-in found by its stated path.
        fn settings(&self) -> Settings {
            Settings {
                path: Some(self.root.join("bin/cadmpeg")),
                search_path: None,
                timeout: Duration::from_secs(20),
                max_step_bytes: 1 << 20,
                temp_root: self.root.join("tmp"),
            }
        }

        fn calls(&self) -> String {
            std::fs::read_to_string(self.root.join("bin/calls")).unwrap_or_default()
        }

        /// The private folders the runner left behind: always zero.
        fn left_behind(&self) -> usize {
            std::fs::read_dir(self.root.join("tmp")).expect("tmp folder").count()
        }
    }

    impl Drop for Bench {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn model(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
    }

    /// The refusal text of a run that must not import.
    fn refusal(bench: &Bench, settings: &Settings) -> String {
        let err = cadmpeg::load_open_within(&bench.input(), 1_000_000, settings, None).expect_err("must refuse");
        assert_eq!(bench.left_behind(), 0, "a refusal leaves no folder: {err}");
        err.to_string()
    }

    #[test]
    fn a_converted_part_imports_through_the_step_importer() {
        let bench = Bench::new("ok");
        let direct = camcraft_app::geometry::import::load_model(&model("box.step")).expect("box.step");
        let graded = cadmpeg::load_open_within(&bench.input(), 1_000_000, &bench.settings(), None).expect("imports");
        assert_eq!(graded.mesh.len(), direct.len(), "the same mesh as the STEP file itself");
        assert!(graded.step_source.is_some(), "the converted STEP text is kept");
        assert_eq!(bench.left_behind(), 0, "the private folder is removed after success");
        // The exact commands, and the path with a space as ONE argument.
        let calls = bench.calls();
        let input = bench.input().display().to_string();
        assert!(calls.starts_with("--version\n\n"), "{calls}");
        assert!(calls.contains(&format!("check\n--json\n{input}\n\n")), "{calls}");
        assert!(calls.contains(&format!("convert\n{input}\n-o\n")), "{calls}");
        assert!(calls.contains("\n--to\nstep\n--reject-lossy=export\n\n"), "{calls}");
        assert!(!calls.contains("--allow-errors"), "{calls}");
    }

    #[test]
    fn the_program_is_found_on_the_search_path_when_no_path_is_set() {
        let bench = Bench::new("path");
        let mut settings = bench.settings();
        settings.path = None;
        settings.search_path = Some(std::env::join_paths([bench.root.join("nowhere"), bench.root.join("bin")]).expect("join"));
        cadmpeg::load_open_within(&bench.input(), 1_000_000, &settings, None).expect("found on the search path");
    }

    #[test]
    fn a_missing_program_is_refused_naming_cadmpeg_and_the_extension() {
        let bench = Bench::new("missing");
        let mut settings = bench.settings();
        settings.path = None;
        settings.search_path = Some(bench.root.join("nowhere").into_os_string());
        let err = cadmpeg::load_open_within(&bench.input(), 1_000_000, &settings, None).expect_err("no program");
        assert!(matches!(err, CommandError::Needs(_)), "{err:?}");
        let text = err.to_string();
        assert!(text.contains("cadmpeg") && text.contains(".sldprt"), "{text}");
        // A stated path that is not a file is refused by that path.
        settings.path = Some(bench.root.join("bin/absent"));
        let text = refusal(&bench, &settings);
        assert!(text.contains("bin/absent") && text.contains("preferences"), "{text}");
        assert_eq!(bench.calls(), "", "nothing ran");
    }

    #[test]
    fn an_old_version_is_refused_by_number() {
        let bench = Bench::new("old");
        bench.put("version", "cadmpeg 0.6.11\n");
        let err = cadmpeg::load_open_within(&bench.input(), 1_000_000, &bench.settings(), None).expect_err("too old");
        assert!(matches!(err, CommandError::Needs(_)), "{err:?}");
        let text = err.to_string();
        assert!(text.contains("0.6.11") && text.contains("0.6.12"), "{text}");
        assert_eq!(bench.calls(), "--version\n\n", "an old program converts nothing");
        // A newer one passes; a pre-release of the minimum does not.
        bench.put("version", "cadmpeg 0.10.0\n");
        cadmpeg::load_open_within(&bench.input(), 1_000_000, &bench.settings(), None).expect("0.10.0 is newer than 0.6.12");
        bench.put("version", "cadmpeg 0.6.12-rc.1\n");
        assert!(refusal(&bench, &bench.settings()).contains("0.6.12-rc.1"));
        // Text that states no version is an unusable program.
        bench.put("version", "hello\n");
        let err = cadmpeg::load_open_within(&bench.input(), 1_000_000, &bench.settings(), None).expect_err("no version");
        assert!(matches!(err, CommandError::Invalid(_)), "{err:?}");
    }

    #[test]
    fn a_failed_check_is_refused_and_nothing_is_converted() {
        let bench = Bench::new("check");
        bench.put("check.exit", "1");
        bench.put("check.err", "error: 2 errors found\n");
        let text = refusal(&bench, &bench.settings());
        assert!(text.contains("cadmpeg check") && text.contains("my part.sldprt") && text.contains("2 errors found"), "{text}");
        assert!(!bench.calls().contains("convert"), "a failed check converts nothing");
    }

    #[test]
    fn a_blocking_finding_is_refused_even_with_exit_status_zero() {
        let bench = Bench::new("finding");
        bench.put("check.json", r#"{"check_report":{"findings":[{"severity":"warning"},{"severity":"blocking","message":"open shell"}]}}"#);
        let text = refusal(&bench, &bench.settings());
        assert!(text.contains("1 blocking finding"), "{text}");
        assert!(!bench.calls().contains("convert"), "a blocking finding converts nothing");
        // A report that is not JSON cannot clear the part.
        bench.put("check.json", "all good");
        let text = refusal(&bench, &bench.settings());
        assert!(text.contains("report"), "{text}");
        assert!(!bench.calls().contains("convert"));
    }

    #[test]
    fn a_hung_program_is_stopped_at_the_time_limit_and_its_folder_removed() {
        let bench = Bench::new("hang");
        bench.put("version.delay", "");
        bench.put("check.hang", "");
        let mut settings = bench.settings();
        settings.timeout = Duration::from_secs(3);
        let started = Instant::now();
        let text = refusal(&bench, &settings);
        assert!(started.elapsed() < Duration::from_secs(15), "stopped at the limit, not after the sleep");
        assert!(text.contains("did not finish") && text.contains("check"), "{text}");
        // The child itself is gone, not only abandoned.
        let pid = std::fs::read_to_string(bench.root.join("bin/check.pid")).expect("the stand-in wrote its pid");
        let pid: u32 = pid.trim().parse().expect("a numeric process id");
        assert!(pid > 0, "a positive process id");
        let status = std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .expect("check whether the child still runs")
            .status;
        assert!(!status.success(), "process {pid} still runs");
    }

    #[test]
    fn a_failed_or_empty_conversion_is_refused() {
        let bench = Bench::new("convert");
        bench.put("convert.exit", "1");
        let text = refusal(&bench, &bench.settings());
        assert!(text.contains("cadmpeg convert") && text.contains("exit status 1"), "{text}");
        // Exit status 0 with no file, then with an empty file.
        bench.put("convert.exit", "0");
        std::fs::remove_file(bench.root.join("bin/convert.step")).expect("remove");
        assert!(refusal(&bench, &bench.settings()).contains("wrote no STEP"));
        bench.put("convert.step", "");
        assert!(refusal(&bench, &bench.settings()).contains("wrote no STEP"));
    }

    #[test]
    fn an_oversized_step_is_refused() {
        let bench = Bench::new("big");
        let mut settings = bench.settings();
        settings.max_step_bytes = 1000; // box.step is about 7 kB
        let text = refusal(&bench, &settings);
        assert!(text.contains("larger than") && text.contains("1000"), "{text}");
    }

    #[test]
    fn output_past_the_capture_limit_stops_the_program() {
        let bench = Bench::new("flood");
        bench.put("convert.flood", "");
        let mut settings = bench.settings();
        settings.timeout = Duration::from_secs(5);
        let started = Instant::now();
        let text = refusal(&bench, &settings);
        assert!(started.elapsed() < Duration::from_secs(15), "stopped at the capture limit");
        assert!(text.contains("printed more than"), "{text}");
    }

    #[test]
    fn garbage_output_is_refused_by_the_step_importer() {
        let bench = Bench::new("garbage");
        bench.put("convert.step", "this is not STEP\n");
        let err = cadmpeg::load_open_within(&bench.input(), 1_000_000, &bench.settings(), None).expect_err("garbage");
        assert!(matches!(err, CommandError::Import(ImportError::Step(_))), "{err:?}");
        assert_eq!(bench.left_behind(), 0);
    }
}
