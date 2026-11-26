//! Inspection work setup from our own implementation's closed contour attributes and zero-step import checks.

use super::{Grid, SCALE};
use crate::agie_setup::{attribute_call, checked};

fn coord(v: i64) -> String {
    format!("{:.5}", v as f64 / SCALE)
}

fn attr(lines: &mut Vec<String>, kind: &str, scope: &str, key: &str, value: &str) {
    checked(lines, attribute_call(kind, scope, key, value));
}

pub(super) fn encode(stem: &str, hole: Grid) -> String {
    let group = format!("\"{stem}\"");
    let work = format!("{group},{group}");
    let context = format!("{work},NULL");
    let point = format!("{work},\"stp1\"");
    let mut lines = vec!["'$Include \"jescript.sbh\"".to_owned(), "sub main".into()];
    checked(&mut lines, "  ok=JE_Initiate".into());

    for (field, value) in [
        ("ID_HEIGHT", "10.000"),
        ("ID_NOMEOBJ", stem),
        ("ID_MATERIAL", "0001"),
        ("ID_THREAD", "Berco Cut 0.25"),
        ("ID_POSX", "0"),
        ("ID_POSY", "0"),
        ("ID_POSZ", "0"),
        ("ID_ROTATION", "0"),
        ("ID_DIML", "0"),
        ("ID_DIMB", "0"),
        ("ID_POSPOSX", "0"),
        ("ID_POSPOSY", "0"),
        ("ID_POSPOSZ", "0"),
        ("ID_VALSECP", "3.0"),
        ("ID_VALRETP", "30.0"),
    ] {
        attr(
            &mut lines,
            "PIECE",
            "NULL,NULL,NULL",
            field,
            &format!("\"{value}\""),
        );
    }
    checked(
        &mut lines,
        format!("  ok=JE_Create(GROUP,je_piece,{group},NULL)"),
    );
    checked(&mut lines, format!("  ok=JE_Create(WORK,je_piece,{work})"));
    attr(
        &mut lines,
        "WORK",
        &context,
        "ID_GEOMETRY",
        &format!("\"AGIE.USING_{stem}.ISO\""),
    );
    attr(&mut lines, "WORK", &context, "ID_PUNCH", "c_left");
    attr(&mut lines, "WORK", &context, "ID_WORKCOND", "\"2\"");
    checked(
        &mut lines,
        "  ok=JE_CreateQuality(je_piece,\"QREF10\",\"0.04,1.50,1.00,F,F,F\",je_q1)".into(),
    );
    attr(&mut lines, "WORK", &context, "ID_QUALITY", "je_q1");
    for (field, value) in [
        ("ID_THREAD", "\"Berco Cut 0.25\""),
        ("ID_ENTRYMODE", "c_entper"),
        ("ID_HEIGHT", "\"10.000\""),
        ("ID_POSX", "\"0\""),
        ("ID_POSY", "\"0\""),
        ("ID_SEPCUT", "\"0\""),
        ("ID_REVCUT", "\"T\""),
        ("ID_COMMPOINTENTRY", "\"0\""),
        ("ID_COMMPOINTEXIT", "\"0\""),
        ("ID_PRIORITY", "\"10\""),
        ("ID_RADIUSMINLA", "\"T\""),
        ("ID_I_TYPE", "c_radius_rm"),
        ("ID_E_TYPE", "c_radius_rm"),
        ("ID_ENTRY", "\"2\""),
    ] {
        attr(&mut lines, "WORK", &context, field, value);
    }
    checked(
        &mut lines,
        format!("  ok=JE_CreateStartPoint(WORK,\"stp1\",je_piece,{work})"),
    );
    for (field, v) in ["ID_POSX", "ID_POSY"].into_iter().zip(hole) {
        attr(
            &mut lines,
            "STARTPOINT",
            &point,
            field,
            &format!("\"{}\"", coord(v)),
        );
    }
    attr(&mut lines, "WORK", &context, "ID_STP", "\"stp1\"");
    attr(&mut lines, "WORK", &context, "ID_STPMODE", "c_work");
    attr(&mut lines, "STARTPOINT", &point, "ID_STP_PL", "\"4\"");
    attr(&mut lines, "STARTPOINT", &point, "ID_DIAMETER", "\"3.0\"");
    checked(
        &mut lines,
        format!("  ok=JE_GenerateCuts(WORK,je_piece,{work},ASDELETE,NULL)"),
    );
    attr(
        &mut lines,
        "PIECE",
        "je_grp,NULL,NULL",
        "ID_STATEGY",
        "\"EW\"",
    );
    checked(&mut lines, "  ok=JE_ClosePiece(je_piece)".into());
    lines.push("end sub".into());
    lines.push(String::new());
    lines.join("\r\n")
}
