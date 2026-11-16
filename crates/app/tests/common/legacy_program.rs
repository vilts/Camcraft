//! Legacy `.cl` goldens dressed as runnable programs (#1357).
//!
//! The `.cl` goldens are bare generator paths: no setup, operation or
//! tool record, no spindle speed, and `; CYCLE` markers with no feed.
//! A post that refuses a canned cycle with no feed, or a cut with the
//! spindle stopped (the Prime Directive), refuses them as they stand. The
//! tests that post them as programs dress them here: each cycle marker
//! takes its own hole chain's cutting feed (what `post::dump` synthesizes
//! for a generated cycle), and a program gets the opening records a real
//! operation's stream carries - setup, operation and tool names, the tool
//! change and the spindle speed.
//!
//! Shared with the post crate's own golden tests (`fanuc_post.rs`,
//! `arc_planes.rs` include this file by path), so it speaks
//! `camcraft_post` only.
//!
//! Provenance: our own test fixtures over our own stream format
//! (docs/CNC-INTERMEDIATE.md); tool data from the generators' golden
//! tools in `golden_cl.rs`.

use camcraft_post::cl::MoveKind;
use camcraft_post::post::cnc::{self, Record};

/// Every `; CYCLE` marker that states no `F` takes the feed of the first
/// cutting move of its own hole chain (the first non-rapid move at or
/// after its `AT` anchor), written in the marker's own x1000 scale.
/// A marker whose chain has no cutting feed is left as it is - the post
/// then refuses it, which is what such a stream deserves.
pub fn with_cycle_feeds(stream: &str) -> String {
    let records = cnc::parse(stream).unwrap_or_else(|e| panic!("legacy stream parses: {e:?}"));
    let moves: Vec<&cnc::MoveRec> = records
        .iter()
        .filter_map(|r| match r {
            Record::Move(m) => Some(m),
            _ => None,
        })
        .collect();
    let feeds: Vec<Option<f64>> = records
        .iter()
        .filter_map(|r| match r {
            Record::Cycle(c) => Some(c),
            _ => None,
        })
        .map(|c| {
            moves
                .iter()
                .skip(c.first_move)
                .find(|m| m.kind != MoveKind::Rapid)
                .and_then(|m| m.feed)
        })
        .collect();
    let mut next = feeds.into_iter();
    let mut out = String::with_capacity(stream.len() + 64);
    for line in stream.lines() {
        let is_marker = line.trim_start().strip_prefix(';').is_some_and(|t| t.trim_start().starts_with("CYCLE"));
        let has_f = line.split_whitespace().any(|w| w.starts_with('F') && w.len() > 1);
        let feed = if is_marker { next.next().flatten() } else { None };
        match feed {
            Some(f) if !has_f => {
                let word = format!(" F{}", (f * 1000.0).round() as i64);
                match line.find(" AT") {
                    Some(at) => {
                        out.push_str(&line[..at]);
                        out.push_str(&word);
                        out.push_str(&line[at..]);
                    }
                    None => {
                        out.push_str(line);
                        out.push_str(&word);
                    }
                }
            }
            _ => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

/// The operation a legacy golden stands for: its tool and spindle.
pub struct Operation {
    pub name: &'static str,
    pub kind: &'static str,
    pub tool: u32,
    pub tool_name: &'static str,
    pub tool_kind: &'static str,
    pub diameter_mm: f64,
    pub rpm: f64,
}

/// The operation behind each legacy golden, by its name's shape: the
/// tool diameter is the one `golden_cl.rs` generated the path with; the
/// tap's speed is its stream feed over its 1.5 mm pitch (750 / 1.5).
pub fn operation_for(golden: &str) -> Operation {
    let mill = |name, kind, tool, tool_name, diameter_mm, rpm| Operation {
        name,
        kind,
        tool,
        tool_name,
        tool_kind: "Flat end mill",
        diameter_mm,
        rpm,
    };
    match golden {
        g if g.starts_with("face_") => mill("Face1", "Facing", 1, "10MM 3FL END MILL", 10.0, 4500.0),
        g if g.starts_with("pocket_") => mill("Pocket1", "Pocket", 3, "8MM 3FL END MILL", 8.0, 5000.0),
        "slot_fullwidth_centerline" => mill("Slot1", "Slot", 1, "10MM 3FL END MILL", 10.0, 4500.0),
        g if g.starts_with("slot_") => mill("Slot1", "Slot", 2, "6MM 3FL END MILL", 6.0, 6000.0),
        g if g.starts_with("trochoid_") => mill("Trochoid1", "Trochoidal slot", 2, "6MM 3FL END MILL", 6.0, 6000.0),
        g if g.starts_with("trace_") => mill("Trace1", "Trace", 2, "6MM 3FL END MILL", 6.0, 6000.0),
        "tap_plate" => Operation {
            name: "Tap1",
            kind: "Tap",
            tool: 5,
            tool_name: "M10X1.5 TAP",
            tool_kind: "Tap",
            diameter_mm: 10.0,
            rpm: 500.0,
        },
        g if g.starts_with("drill_") => Operation {
            name: "Drill1",
            kind: "Drill",
            tool: 4,
            tool_name: "6MM HSS DRILL",
            tool_kind: "Drill",
            diameter_mm: 6.0,
            rpm: 1500.0,
        },
        _ => mill("Contour1", "2D Contour", 2, "6MM 3FL END MILL", 6.0, 6000.0),
    }
}

/// A legacy golden as the program a real operation posts: the setup,
/// operation and tool comment parameters, the tool change and the
/// spindle speed ahead of the path, and each cycle's feed stated.
pub fn as_program(golden: &str, stream: &str) -> String {
    let op = operation_for(golden);
    let mut lines = stream.lines();
    let mut out = String::new();
    // The `; HSMW-CL` version comment stays the first line.
    if let Some(first) = lines.next() {
        out.push_str(first);
        out.push('\n');
    }
    out.push_str("PARAM SetupName Setup1\n");
    out.push_str(&format!("PARAM OperationName {}\n", op.name));
    out.push_str(&format!("PARAM OperationKind {}\n", op.kind));
    out.push_str(&format!("PARAM ToolKind {}\n", op.tool_kind));
    out.push_str(&format!("PARAM ToolDiameter {}\n", op.diameter_mm));
    out.push_str(&format!("TOOL {} {}\n", op.tool, op.tool_name));
    out.push_str(&format!("PARAM SpindleSpeed {}\n", op.rpm));
    let rest: Vec<&str> = lines.collect();
    // #1614: a real section states its clearance plane; the legacy
    // golden's START height is the generator's clearance.
    if let Some(z) = rest.iter().find_map(|l| l.strip_prefix("START ")).and_then(|w| w.split_whitespace().nth(2)).and_then(|z| z.parse::<f64>().ok()) {
        out.push_str(&format!("PARAM ClearancePlane {}\n", z / 1000.0));
    }
    out.push_str(&with_cycle_feeds(&rest.join("\n")));
    out
}
