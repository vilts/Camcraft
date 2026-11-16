//! Provenance: bounded release-gate lanes requested in issue #1703; standard Rust scoped threads.

use crate::plan::Step;

pub const LIMIT: usize = 4;

/// Whole app/kernel suites include import watchdogs, and UI suites can open windows. Keep these,
/// builds and clippy alone. A selected release subset uses the same conservative rule.
pub fn alone(s: &Step) -> bool {
    s.in_ui
        || s.args.iter().any(|a| matches!(a.as_str(),
            "--no-run" | "clippy" | "camcraft-ui" | "camcraft-kernel" | "camcraft-app"
        ))
}

/// Group independent suites into waves of at most four. Mix profiles within a wave so heavy release
/// tests can run beside debug suites. Barrier steps never share a wave, including in selected runs.
pub fn waves(steps: &[Step]) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut debug = Vec::new();
    let mut release = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        if s.args.iter().any(|a| a == "--no-run") {
            out.push(vec![i]);
        } else if !alone(s) {
            if s.args.iter().any(|a| a == "--release") {
                release.push(i);
            } else {
                debug.push(i);
            }
        }
    }
    let (mut debug, mut release) = (debug.into_iter(), release.into_iter());
    loop {
        let mut wave = Vec::new();
        for _ in 0..LIMIT / 2 {
            if let Some(i) = debug.next() {
                wave.push(i);
            }
            if let Some(i) = release.next() {
                wave.push(i);
            }
        }
        while wave.len() < LIMIT {
            let Some(i) = debug.next().or_else(|| release.next()) else { break };
            wave.push(i);
        }
        if wave.is_empty() {
            break;
        }
        out.push(wave);
    }
    for (i, s) in steps.iter().enumerate() {
        if alone(s) && !s.args.iter().any(|a| a == "--no-run") {
            out.push(vec![i]);
        }
    }
    out
}
