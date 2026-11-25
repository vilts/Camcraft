// Provenance: issues #1832 and #3276 and the repository's release workflow contract.
use std::{fs, path::Path};

fn workflow() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows/release.yml");
    fs::read_to_string(path).expect("read the release workflow")
}

/// One job of release.yml: from its `  <name>:` line up to the next job (a
/// line indented two spaces) or the end of the file.
fn job(name: &str) -> String {
    let workflow = workflow();
    let body = workflow
        .split_once(&format!("\n  {name}:\n"))
        .unwrap_or_else(|| panic!("release.yml has a {name} job"))
        .1;
    body.lines()
        .take_while(|line| line.is_empty() || line.starts_with("   ") || line.starts_with("  #"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The job's own `if:` condition, trimmed (None when it has none).
fn condition(name: &str) -> Option<String> {
    job(name)
        .lines()
        .find_map(|l| l.strip_prefix("    if: ").map(|c| c.trim().to_string()))
}

fn site_job() -> String {
    job("site")
}

#[test]
fn the_site_job_runs_the_site_tests_with_its_pinned_zola() {
    let job = site_job();
    let install = job.find("- name: Install pinned Zola").expect("the site job installs Zola");
    let tests = job.find("- name: Site tests").expect("the site job runs the site tests (#1832)");
    assert!(install < tests, "the site tests run after Zola is installed");
    let step = &job[tests..];
    let step = step[1..].split_once("\n      - ").map_or(step, |(s, _)| s);
    assert!(
        step.lines().any(|l| l.trim() == "run: python3 -m unittest discover -s site/tests -v"),
        "the step runs every site test, the operation reference included:\n{step}"
    );
    assert!(step.lines().any(|l| l.trim() == "ZOLA: /tmp/zola"), "the tests build with the pinned Zola:\n{step}");
    assert!(!step.contains("continue-on-error"), "a red site test fails the site job:\n{step}");
}

#[test]
fn site_only_dispatch_runs_only_site_and_pages() {
    // #3276: the input lives on the existing workflow_dispatch; no trigger is added.
    let wf = workflow();
    let on = wf.split_once("\non:\n").expect("release.yml has an on: block").1;
    let on = on.split_once("\n\n").map_or(on, |(head, _)| head);
    let triggers: Vec<&str> =
        on.lines().filter(|l| l.starts_with("  ") && !l.starts_with("   ")).map(str::trim).collect();
    assert_eq!(triggers, ["push:", "workflow_dispatch:"], "release.yml triggers:\n{on}");
    assert!(on.contains("      site_only:\n"), "workflow_dispatch has the site_only input:\n{on}");
    assert!(on.contains("        type: boolean\n        default: false"), "site_only is a boolean, off by default:\n{on}");

    // Every build job is skipped on a site_only run; aur, snap and flatpak follow linux.
    for name in ["linux", "macos", "windows", "wasm"] {
        assert_eq!(condition(name).as_deref(), Some("${{ !inputs.site_only }}"), "{name} skips on site_only");
    }
    for name in ["aur", "snap", "flatpak"] {
        assert!(job(name).contains("    needs: linux"), "{name} follows linux");
        assert_eq!(condition(name), None, "{name} has no condition of its own");
    }

    // publish stays tag-only and never runs on site_only; the site still
    // builds without the wasm job.
    assert_eq!(
        condition("publish").as_deref(),
        Some("${{ !cancelled() && !inputs.site_only && startsWith(github.ref, 'refs/tags/') }}"),
        "publish is tag-only and skips on site_only"
    );
    assert_eq!(condition("site").as_deref(), Some("${{ !cancelled() }}"));

    // pages: the tag path as before, or a site_only dispatch from a site/* branch.
    let pages = condition("pages").expect("pages has a condition");
    assert!(pages.contains("needs.site.result == 'success'"), "{pages}");
    assert!(
        pages.contains("(startsWith(github.ref, 'refs/tags/') && needs.publish.result == 'success')"),
        "the tag path still waits for publish: {pages}"
    );
    assert!(
        pages.contains(
            "(github.event_name == 'workflow_dispatch' && inputs.site_only && startsWith(github.ref, 'refs/heads/site/'))"
        ),
        "a site_only dispatch deploys only from a site/* branch: {pages}"
    );
}

/// The steps of the site job, each from its `      - ` line to the next.
fn site_steps() -> Vec<String> {
    let job = site_job();
    let steps = job.split_once("\n    steps:\n").expect("the site job has steps").1;
    steps.split("\n      - ").map(|s| s.trim_start_matches("      - ").to_string()).collect()
}

#[test]
fn site_only_dispatch_can_take_the_app_of_an_earlier_run() {
    // #3276: an optional string input on the same workflow_dispatch, empty by default.
    let wf = workflow();
    let on = wf.split_once("\non:\n").expect("release.yml has an on: block").1;
    let on = on.split_once("\n\n").map_or(on, |(head, _)| head);
    let input = on.split_once("      app_run_id:\n").expect("workflow_dispatch has the app_run_id input").1;
    assert!(input.contains("        type: string\n        default: ''"), "app_run_id is a string, empty by default:\n{on}");

    // The site job may read the artifacts of another run: actions: read, and nothing more.
    let job = site_job();
    let perms = job.split_once("\n    permissions:\n").expect("the site job has permissions").1;
    let perms: Vec<&str> = perms.lines().take_while(|l| l.starts_with("      ")).map(str::trim).collect();
    assert_eq!(perms, ["contents: read", "actions: read"], "site job permissions");

    let steps = site_steps();
    let downloads: Vec<&String> =
        steps.iter().filter(|s| s.starts_with("uses: actions/download-artifact@v4")).collect();
    assert_eq!(downloads.len(), 2, "one download from this run, one from app_run_id");

    // The download from this run is unchanged: only after a successful wasm job.
    let own = downloads[0];
    assert!(own.contains("if: ${{ needs.wasm.result == 'success' }}"), "{own}");
    assert!(own.contains("continue-on-error: true"), "{own}");
    assert!(!own.contains("run-id:"), "{own}");

    // The earlier run's download: only on site_only with an id, with the job token, strict.
    let earlier = downloads[1];
    assert!(earlier.contains("if: ${{ inputs.site_only && inputs.app_run_id != '' }}"), "{earlier}");
    assert!(earlier.contains("name: wasm-app"), "{earlier}");
    assert!(earlier.contains("path: /tmp/wasm-app"), "{earlier}");
    assert!(earlier.contains("run-id: ${{ inputs.app_run_id }}"), "{earlier}");
    assert!(earlier.contains("github-token: ${{ github.token }}"), "{earlier}");
    assert!(
        !earlier.lines().any(|l| l.trim().starts_with("continue-on-error:")),
        "a failed download of the asked-for app fails the site:\n{earlier}"
    );

    // The id is checked before the download, through env:.
    let check = steps.iter().position(|s| s.starts_with("name: Earlier run of the browser build"));
    let check = check.expect("a step checks app_run_id");
    let download_at = steps.iter().position(|s| std::ptr::eq(s, earlier)).expect("the download is a step");
    assert!(check < download_at, "the id is checked before the download");

    // The downloaded app still passes web/check-app-page.sh, and a failure is an error.
    let is_app = steps
        .iter()
        .find(|s| s.starts_with("name: Is there a browser build to publish"))
        .expect("the site job decides if there is an app");
    assert!(is_app.contains("APP_RUN_ID: ${{ inputs.site_only && inputs.app_run_id || '' }}"), "{is_app}");
    assert!(is_app.contains("bash web/check-app-page.sh /tmp/wasm-app"), "{is_app}");
    assert!(is_app.contains("::error::the wasm-app artifact of run $APP_RUN_ID"), "{is_app}");

    // No input is interpolated into a run: script; inputs go through env:.
    for step in &steps {
        if let Some((_, script)) = step.split_once("run: |") {
            assert!(!script.contains("${{ inputs."), "an input is interpolated in a script:\n{step}");
        }
    }
}
