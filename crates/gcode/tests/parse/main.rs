//! The one integration suite of `camcraft-gcode`: NC text in, `cl::Toolpath` sections or a line-numbered refusal out.
//! Provenance: RS-274/NGC (NISTIR 6556), ISO 6983, the LinuxCNC G-code reference and the Haas G04 reference (behaviour only).
//! Gate map: covers camcraft_gcode::{types, lexer, block, block_codes, arc, cycle, cycle_expand, dialect, domain, frame, motion, section, tooling, interp, error}.

mod codes_and_dwell;
mod cycle_bounds;
mod cycle_rules;
mod degenerate_input;
mod dialects_and_planes;
mod frames;
mod numeric_domain;
mod sections;
mod three_axis;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use camcraft_gcode::{parse_with, GcodeError, ParseOptions, ParsedProgram};

/// Parse `nc` with `options` on a worker thread. Fail on a panic, or when no
/// answer arrives in five seconds (a hang is a failure, not a slow pass).
pub fn parse_in_time(nc: &str, options: &ParseOptions) -> Result<ParsedProgram, GcodeError> {
    let (tx, rx) = mpsc::channel();
    let (nc_owned, opts) = (nc.to_string(), options.clone());
    let worker = thread::spawn(move || {
        let _ = tx.send(parse_with(&nc_owned, &opts));
    });
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(result) => {
            assert!(worker.join().is_ok(), "parse panicked for {nc:?}");
            result
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("parse panicked for {nc:?}"),
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("parse did not return in 5 s for {nc:?}"),
    }
}

/// Parse with default options in time, and return the refusal (fail if it parses).
pub fn refusal(nc: &str) -> GcodeError {
    match parse_in_time(nc, &ParseOptions::default()) {
        Ok(p) => panic!("expected a refusal for {nc:?}, got {} sections", p.sections.len()),
        Err(e) => e,
    }
}
