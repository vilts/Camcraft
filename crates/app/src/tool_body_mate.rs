//! Coordinate-system mate resolution (#361 / #1040): resolve explicit
//! or unambiguous ISO 13399 `MCS` <-> `CSW` mating pairs across existing
//! assembly pieces and an incoming vendor model, refusing absent or
//! ambiguous frames by name.
//! Provenance: ISO/TS 13399 role pairing (`MCS` mounting vs `CSW*`
//! workpiece side); our own implementation, no vendor file consulted.

use std::ops::Range;

use crate::geometry::import::step_csys::CsysRole;

use super::{BodyCsys, BodyPiece, LoadedPiece};

pub(super) fn resolve<'a>(
    tool_name: &str,
    existing: &'a [BodyPiece],
    incoming: &'a LoadedPiece,
    target_piece: Option<usize>,
    target_csys: Option<&str>,
    piece_csys: Option<&str>,
) -> Result<(&'a BodyCsys, &'a BodyCsys), String> {
    let range = match target_piece {
        Some(idx) if idx >= existing.len() => {
            return Err(format!(
                "target piece index {idx} is out of range (tool '{tool_name}' has {} piece(s))",
                existing.len()
            ));
        }
        Some(idx) => idx..idx + 1,
        None => 0..existing.len(),
    };
    if target_csys.is_none() && piece_csys.is_none() {
        return resolve_auto(tool_name, existing, range, incoming);
    }
    let tc = match target_csys {
        Some(name) => find_target_by_name(tool_name, existing, range.clone(), name)?,
        None => {
            let pc_name = piece_csys.unwrap_or_default();
            let pc = find_piece_by_name(&incoming.source, &incoming.csys, pc_name)?;
            find_complementary_target(tool_name, existing, range, pc)?
        }
    };
    let pc = match piece_csys {
        Some(name) => find_piece_by_name(&incoming.source, &incoming.csys, name)?,
        None => find_complementary_piece(&incoming.source, &incoming.csys, tc)?,
    };
    Ok((tc, pc))
}

fn is_complementary(a: CsysRole, b: CsysRole) -> bool {
    matches!(
        (a, b),
        (CsysRole::Mounting, CsysRole::WorkpieceSide { .. })
            | (CsysRole::WorkpieceSide { .. }, CsysRole::Mounting)
    )
}

fn resolve_auto<'a>(
    tool_name: &str,
    existing: &'a [BodyPiece],
    range: Range<usize>,
    incoming: &'a LoadedPiece,
) -> Result<(&'a BodyCsys, &'a BodyCsys), String> {
    if incoming.csys.is_empty() {
        return Err(format!("piece '{}' has no named coordinate systems to mate by", incoming.source));
    }
    if !range.clone().any(|i| !existing[i].csys.is_empty()) {
        return Err(format!(
            "tool '{tool_name}'s existing body has no named coordinate systems to mate onto (give an explicit placement)"
        ));
    }
    let mut pairs: Vec<(usize, &'a BodyCsys, &'a BodyCsys)> = Vec::new();
    for i in range.clone() {
        for tc in &existing[i].csys {
            for pc in &incoming.csys {
                if is_complementary(tc.role, pc.role) {
                    pairs.push((i, tc, pc));
                }
            }
        }
    }
    match pairs.as_slice() {
        [(_, tc, pc)] => Ok((*tc, *pc)),
        [] => Err(format!(
            "no complementary ISO 13399 MCS/CSW mating pair between tool '{tool_name}' ([{}]) and '{}' ([{}]); pass explicit target_csys and piece_csys",
            describe_existing(existing, range),
            incoming.source,
            describe_list(&incoming.csys)
        )),
        many => {
            let opts: Vec<String> = many
                .iter()
                .map(|(i, tc, pc)| format!("piece[{i}].{} <-> {}", tc.name, pc.name))
                .collect();
            Err(format!(
                "ambiguous ISO 13399 coordinate system mate ({} candidates: {}); specify target_piece, target_csys and piece_csys",
                many.len(),
                opts.join(", ")
            ))
        }
    }
}

fn name_matches(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn collect_target_matches(
    existing: &[BodyPiece],
    range: Range<usize>,
    pred: impl Fn(&BodyCsys) -> bool,
) -> Vec<(usize, &BodyCsys)> {
    let mut hits = Vec::new();
    for i in range {
        for c in &existing[i].csys {
            if pred(c) {
                hits.push((i, c));
            }
        }
    }
    hits
}

fn find_target_by_name<'a>(
    tool_name: &str,
    existing: &'a [BodyPiece],
    range: Range<usize>,
    wanted: &str,
) -> Result<&'a BodyCsys, String> {
    match collect_target_matches(existing, range.clone(), |c| name_matches(&c.name, wanted)).as_slice() {
        [(_, c)] => Ok(*c),
        [] => Err(format!(
            "target coordinate system '{wanted}' not found on tool '{tool_name}' (available: [{}])",
            describe_existing(existing, range)
        )),
        many => {
            let locs: Vec<String> = many.iter().map(|(i, c)| format!("piece[{i}].{}", c.name)).collect();
            Err(format!(
                "ambiguous target coordinate system '{wanted}' on tool '{tool_name}' ({}); specify target_piece",
                locs.join(", ")
            ))
        }
    }
}

fn find_piece_by_name<'a>(source: &str, csys: &'a [BodyCsys], wanted: &str) -> Result<&'a BodyCsys, String> {
    let hits: Vec<&BodyCsys> = csys.iter().filter(|c| name_matches(&c.name, wanted)).collect();
    match hits.as_slice() {
        [c] => Ok(*c),
        [] => Err(format!(
            "piece coordinate system '{wanted}' not found on '{source}' (available: [{}])",
            describe_list(csys)
        )),
        many => Err(format!(
            "ambiguous piece coordinate system '{wanted}' on '{source}' ({} matches)",
            many.len()
        )),
    }
}

fn find_complementary_target<'a>(
    tool_name: &str,
    existing: &'a [BodyPiece],
    range: Range<usize>,
    pc: &BodyCsys,
) -> Result<&'a BodyCsys, String> {
    match collect_target_matches(existing, range.clone(), |c| is_complementary(c.role, pc.role)).as_slice() {
        [(_, c)] => Ok(*c),
        [] => Err(format!(
            "no complementary coordinate system on tool '{tool_name}' for '{}' ({}) (available: [{}])",
            pc.name,
            incoming_role_hint(pc.role),
            describe_existing(existing, range)
        )),
        many => {
            let locs: Vec<String> = many.iter().map(|(i, c)| format!("piece[{i}].{}", c.name)).collect();
            Err(format!(
                "ambiguous target coordinate system for '{}' ({}); specify target_piece and target_csys",
                pc.name,
                locs.join(", ")
            ))
        }
    }
}

fn find_complementary_piece<'a>(source: &str, csys: &'a [BodyCsys], tc: &BodyCsys) -> Result<&'a BodyCsys, String> {
    let hits: Vec<&BodyCsys> = csys.iter().filter(|c| is_complementary(tc.role, c.role)).collect();
    match hits.as_slice() {
        [c] => Ok(*c),
        [] => Err(format!(
            "no complementary coordinate system on '{source}' for '{}' (available: [{}])",
            tc.name,
            describe_list(csys)
        )),
        many => {
            let names: Vec<&str> = many.iter().map(|c| c.name.as_str()).collect();
            Err(format!(
                "ambiguous piece coordinate system on '{source}' for '{}' ({}); specify piece_csys",
                tc.name,
                names.join(", ")
            ))
        }
    }
}

fn incoming_role_hint(role: CsysRole) -> &'static str {
    match role {
        CsysRole::Mounting => "MCS expects CSW*",
        CsysRole::WorkpieceSide { .. } => "CSW* expects MCS",
        _ => "specify target_csys",
    }
}

fn describe_list(csys: &[BodyCsys]) -> String {
    if csys.is_empty() {
        return "none".into();
    }
    csys.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
}

fn describe_existing(existing: &[BodyPiece], range: Range<usize>) -> String {
    range
        .map(|i| format!("piece[{i}] ({}): {}", existing[i].source, describe_list(&existing[i].csys)))
        .collect::<Vec<_>>()
        .join("; ")
}
