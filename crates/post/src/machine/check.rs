//! Pure machine limit judgment over caller-provided program values (#2585).
//! Provenance: general machine-tool practice; extracted from our app machine checks.

use super::judge::{c_letter_problem, move_extremes, pose_problems, say, slowest_axis};
use super::{Kind, Machine, kind};
use crate::cl::Toolpath;

/// The caller's source for a finding. NC callers can judge each line's moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceLocation<'a> {
    Operation(&'a str),
    NcLine(usize),
}

fn source_name(source: Option<SourceLocation<'_>>) -> String {
    match source {
        Some(SourceLocation::Operation(name)) => name.to_owned(),
        Some(SourceLocation::NcLine(line)) => format!("line {line}"),
        None => "program".into(),
    }
}

/// What a program asks that the machine cannot give. Findings name the
/// caller-provided source, so a limit violation identifies where it occurs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MachineCheck {
    pub problems: Vec<String>,
    /// How many more the reporter withheld (the walk caps its list;
    /// the count never lies).
    pub more: usize,
    /// What the machine's kind puts outside this check (#669 - a plasma
    /// table's Z depth and rpm), named so a skip is never silent. Notes
    /// never block: they say what was NOT judged, not what failed.
    pub notes: Vec<String>,
}

impl MachineCheck {
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.problems.is_empty()
    }
}

pub const REPORT_CAP: usize = 40;

impl MachineCheck {
    /// Count distinct tool identities across the whole program.
    pub fn check_tool_count(&mut self, machine: &Machine, tools: &[&str]) {
        if let Some(slots) = machine.tool_count {
            let mut names = Vec::new();
            for name in tools {
                if !names.contains(name) {
                    names.push(*name);
                }
            }
            if names.len() > slots {
                say(
                    self,
                    format!(
                        "program calls {} tools, the changer holds {slots}",
                        names.len()
                    ),
                );
            }
        }
    }

    /// Check posted RPM and whole protruding tool dimensions, in millimeters.
    pub fn check_tool(
        &mut self,
        machine: &Machine,
        source: Option<SourceLocation<'_>>,
        tool_name: &str,
        len: f64,
        diameter: f64,
        rpm: f64,
    ) {
        self.check_spindle(machine, source, tool_name, rpm);
        self.check_tool_size(machine, source, tool_name, len, diameter);
    }

    /// A zero RPM represents an unspun tool, such as a probe.
    pub fn check_spindle(
        &mut self,
        machine: &Machine,
        source: Option<SourceLocation<'_>>,
        tool_name: &str,
        rpm: f64,
    ) {
        let name = source_name(source);
        if rpm > 0.0 {
            if let Some(hi) = machine.spindle_max
                && rpm > hi
            {
                say(
                    self,
                    format!(
                        "{name}: tool {} runs {rpm:.0} rpm, above the machine's {hi:.0}",
                        tool_name
                    ),
                );
            }
            if let Some(lo) = machine.spindle_min
                && rpm < lo
            {
                say(
                    self,
                    format!(
                        "{name}: tool {} runs {rpm:.0} rpm, below the machine's {lo:.0}",
                        tool_name
                    ),
                );
            }
        }
    }

    /// Check the full protrusion and cutter diameter against magazine limits.
    pub fn check_tool_size(
        &mut self,
        machine: &Machine,
        source: Option<SourceLocation<'_>>,
        tool_name: &str,
        len: f64,
        diameter: f64,
    ) {
        let name = source_name(source);
        if let Some(l) = machine.max_tool_length
            && len > l
        {
            say(
                self,
                format!(
                    "{name}: tool {} is {len:.1} mm long, the machine takes {l:.1}",
                    tool_name
                ),
            );
        }
        if let Some(diam) = machine.max_tool_diameter
            && diameter > diam
        {
            say(
                self,
                format!(
                    "{name}: tool {} is {:.1} mm across, the machine takes {diam:.1}",
                    tool_name, diameter
                ),
            );
        }
    }

    /// Judge pose, feed and program-coordinate travel in millimeters.
    /// Set `check_travel` false only when the caller separately judges the
    /// posted lathe slide path. Pose and feed are still checked.
    pub fn check_path(
        &mut self,
        machine: &Machine,
        source: Option<SourceLocation<'_>>,
        path: &Toolpath,
        origin: [f64; 3],
        flip: [bool; 3],
        check_travel: bool,
    ) {
        let name = source_name(source);
        let lathe = kind(Some(machine)) == Kind::Lathe;
        for (i, mv) in path.moves.iter().enumerate() {
            // Pose (M5.1 stage A): a move that leans the tool asks
            // the machine to hold an angle, and the machine answers
            // with its rotaries - by name, never silently. Absence
            // is the ordinary case (prismatic), checked exact so no
            // float dust enters this branch from a normal path.
            // A C-axis live-tool move (issue #33) states the spindle's
            // angle; its pose is the fixed station's, so the letter it
            // asks for is C alone - read here, never re-derived from
            // the part-frame axis (whose A/B the machine does not have).
            if let Some(c) = mv.rotary {
                if let Some(msg) = c_letter_problem(machine, i, c) {
                    say(self, format!("{name}: {msg}"));
                }
            } else if let Some(a) = mv.axis
                && !crate::cl::is_vertical(&a)
            {
                // The letters (or, on a 4th-axis table, the angle about
                // X, #797) name what the rotaries must hold (`judge`).
                for msg in pose_problems(machine, i, a) {
                    say(self, format!("{name}: {msg}"));
                }
            }
            // Feed: only stated feeds check against stated ceilings.
            if let Some(feed) = mv.feed {
                let d = [
                    mv.to.x - mv.from.x,
                    mv.to.y - mv.from.y,
                    mv.to.z - mv.from.z,
                ];
                if let Some(ceil) = machine.feed_ceiling(d)
                    && feed > ceil
                {
                    say(
                        self,
                        format!(
                            "{name}: move {i} feeds {feed:.0} mm/min, {} tops out at {ceil:.0}",
                            slowest_axis(&machine.axes, ceil)
                        ),
                    );
                }
            }
            if !check_travel {
                continue; // its stroke was judged above, as posted (#647)
            }
            // Travel: endpoints and - for arcs - the quarter points
            // the sweep can bulge past. A straight move's extremes
            // ARE its endpoints. The stroke is the machine's own; an
            // inverted axis (#373) reads program `v` as `-v`, so the
            // band is judged in the machine's coordinates.
            // #631: on a lathe, an arc about the SPINDLE axis (a thread's
            // half-turn) is the spindle turning, not the slide: its far
            // side (negative X, and the quarter points) is the CL's
            // antipode, never a slide position. The slide reaches the
            // radius - judge |r| in X. Every other move keeps its signed X,
            // so a real move past centre is judged as before.
            let spindle_arc = lathe
                && mv.arc.is_some_and(|a| {
                    a.plane.is_xy()
                        && (a.center.x - origin[0]).abs() < 1e-6
                        && (a.center.y - origin[1]).abs() < 1e-6
                });
            for p in move_extremes(mv) {
                for (k, w) in [p.x, p.y, p.z].iter().enumerate() {
                    let Some(ax) = machine.axis(k) else { continue };
                    let Some((lo, hi)) = ax.travel else { continue };
                    // #1649: a flipped WCS axis posts the other sign too.
                    let sign = ax.direction.sign()
                        * if flip.get(k).copied().unwrap_or(false) {
                            -1.0
                        } else {
                            1.0
                        };
                    let v = if spindle_arc && k == 0 {
                        (p.x - origin[0]).hypot(p.y - origin[1]) * sign
                    } else {
                        (w - origin[k]) * sign
                    };
                    if v < lo || v > hi {
                        let inv = if ax.direction.is_standard() {
                            ""
                        } else {
                            " (inverted axis)"
                        };
                        say(
                            self,
                            format!(
                                "{name}: move {i} reaches {v:.3} in {}{inv}, the stroke is {lo:.3}..{hi:.3}",
                                ax.name
                            ),
                        );
                    }
                }
            }
        }
    }
}
