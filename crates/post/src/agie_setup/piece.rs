//! The zero-step SBL piece script for one group of native works, from our own implementation of the
//! piece format and our offline control checks (#2984 closed cycles, #2962 open cuts, #2961 groups).

/// The piece: its group name and the stock that all of its works share.
pub(super) struct Piece<'a> {
    /// The group name and the stem of the SBL and SBR.
    pub stem: &'a str,
    pub stock_min_um: [i32; 2],
    pub stock_max_um: [i32; 2],
    pub height_um: i32,
    /// The one quality object that every work of the piece refers to.
    pub quality: super::Quality,
}

/// One native work. A closed cycle has a 2 mm start hole. An open cut has an
/// entry point and an exit point in air.
pub(super) struct Work<'a> {
    /// The stem of the ISO geometry of this work.
    pub geometry: &'a str,
    /// `c_punch`, `c_die`, `c_left` or `c_right`.
    pub side: &'static str,
    /// The start hole of a closed cycle, or the entry point of an open cut.
    pub start_um: [i32; 2],
    /// The exit point of an open cut. `None` for a closed cycle.
    pub exit_um: Option<[i32; 2]>,
}

/// Format native millimetres without rounding a floating point coordinate.
fn mm(value_um: i64) -> String {
    let sign = if value_um < 0 { "-" } else { "" };
    let value = value_um.unsigned_abs();
    format!("{sign}{}.{:03}", value / 1000, value % 1000)
}

fn attribute(lines: &mut Vec<String>, object: &str, context: &str, key: &str, value: &str) {
    lines.push(attribute_call(object, context, key, value));
}

/// One SBL attribute call. The inspection work shares it (#3219).
pub(super) fn attribute_call(object: &str, context: &str, key: &str, value: &str) -> String {
    format!("  ok=JE_AttribObject({object},SETATTR,je_piece,{context},{key},{value},je_dim)")
}

pub(super) fn checked(lines: &mut Vec<String>, command: String) {
    lines.push(command);
    lines.extend([
        "  if (ok<>0) then".into(),
        "    stop".into(),
        "  end if".into(),
    ]);
}

fn point(lines: &mut Vec<String>, context: &str, at: [i32; 2]) {
    for (field, coordinate) in ["ID_POSX", "ID_POSY"].into_iter().zip(at) {
        attribute(
            lines,
            "STARTPOINT",
            context,
            field,
            &format!("\"{}\"", mm(i64::from(coordinate))),
        );
    }
}

pub(super) fn piece(piece: &Piece<'_>, works: &[Work<'_>]) -> String {
    let [xmin, ymin] = piece.stock_min_um.map(i64::from);
    let [xmax, ymax] = piece.stock_max_um.map(i64::from);
    let stem = piece.stem;
    let height = mm(i64::from(piece.height_um));
    let group = format!("\"{stem}\"");
    let mut lines = vec!["'$Include \"jescript.sbh\"".into(), "sub SetOrigin".into()];
    for (field, value) in [
        ("ID_HEIGHT", height.clone()),
        ("ID_NOMEOBJ", stem.into()),
        ("ID_MATERIAL", "0001".into()),
        ("ID_THREAD", "Berco Cut 0.25".into()),
        ("ID_POSX", "0".into()),
        ("ID_POSY", "0".into()),
        ("ID_POSZ", "0".into()),
        ("ID_ROTATION", "0".into()),
        ("ID_DIML", mm(xmax - xmin)),
        ("ID_DIMB", mm(ymax - ymin)),
        ("ID_POSPOSX", mm(xmin)),
        ("ID_POSPOSY", mm(ymin)),
        ("ID_POSPOSZ", mm(-i64::from(piece.height_um))),
        ("ID_VALSECP", "3.0".into()),
        ("ID_VALRETP", "30.0".into()),
    ] {
        attribute(
            &mut lines,
            "PIECE",
            "NULL,NULL,NULL",
            field,
            &format!("\"{value}\""),
        );
    }
    lines.extend([
        "  ok=JE_GenerateAttrib(c_tecauto,c_tecauto_t)".into(),
        "  ok=JE_GenerateAttrib(c_tecassign,c_tecass_fi)".into(),
        "  ok=JE_GenerateAttrib(c_tecalert,c_tecalert_sea)".into(),
        "end sub".into(),
        "sub CutPunch".into(),
    ]);
    // One quality object for the whole piece. Each work refers to it, so no
    // second quality name can bind to other resident data (#2984).
    checked(
        &mut lines,
        format!(
            "  ok=JE_CreateQuality(je_piece,\"{}\",\"{}\",je_q1)",
            piece.quality.name(),
            piece.quality.request()
        ),
    );
    checked(
        &mut lines,
        format!("  ok=JE_Create(GROUP,je_piece,{group},NULL)"),
    );
    for (index, setup) in works.iter().enumerate() {
        work(&mut lines, &group, &height, index + 1, setup);
    }
    lines.extend([
        "end sub".into(),
        "sub main".into(),
        "  ok=JE_Initiate".into(),
        "  if (ok=0) then".into(),
        "    SetOrigin".into(),
        "    CutPunch".into(),
        "    ok=JE_AttribObject(PIECE,SETATTR,je_piece,je_grp,NULL,NULL,ID_STATEGY,\"EW\",je_dim)"
            .into(),
        "    ok=JE_ClosePiece(je_piece)".into(),
        "  else".into(),
        "    stop".into(),
        "  end if".into(),
        "end sub".into(),
        String::new(),
    ]);
    lines.join("\r\n")
}

/// The records of one work `CUT<number>`, with zero erosion steps on import.
fn work(lines: &mut Vec<String>, group: &str, height: &str, number: usize, setup: &Work<'_>) {
    let open = setup.exit_um.is_some();
    let work = format!("{group},\"CUT{number}\"");
    let work_context = format!("{work},NULL");
    let hole_context = format!("{work},\"stp1\"");
    let work_height = format!("\"{height}\"");
    checked(lines, format!("  ok=JE_Create(WORK,je_piece,{work})"));
    attribute(
        lines,
        "WORK",
        &work_context,
        "ID_GEOMETRY",
        &format!("\"AGIE.USING_{}.ISO\"", setup.geometry),
    );
    attribute(lines, "WORK", &work_context, "ID_PUNCH", setup.side);
    attribute(lines, "WORK", &work_context, "ID_WORKCOND", "\"2\"");
    checked(
        lines,
        format!(
            "  ok=JE_AttribObject(WORK,SETATTR,je_piece,{work_context},ID_QUALITY,je_q1,je_dim)"
        ),
    );
    // A closed cycle needs a 1 mm separation. An open cut has none (#2962).
    let separation = if open { "\"0.0\"" } else { "\"1.0\"" };
    for (field, value) in [
        ("ID_THREAD", "\"Berco Cut 0.25\""),
        ("ID_ENTRYMODE", "c_entper"),
        ("ID_HEIGHT", work_height.as_str()),
        ("ID_POSX", "\"0\""),
        ("ID_POSY", "\"0\""),
        ("ID_SEPCUT", separation),
        ("ID_REVCUT", "\"T\""),
        ("ID_COMMPOINTENTRY", "\"3.0\""),
        ("ID_COMMPOINTEXIT", "\"0\""),
        ("ID_PRIORITY", "\"10\""),
        ("ID_RADIUSMINLA", "\"T\""),
        ("ID_I_TYPE", "c_radius_rm"),
        ("ID_E_TYPE", "c_radius_rm"),
    ] {
        attribute(lines, "WORK", &work_context, field, value);
    }
    // An open cut leaves the stock perpendicular to its exit point. Only a
    // closed cycle has the entry type of its start hole.
    if open {
        attribute(lines, "WORK", &work_context, "ID_EXITMODE", "c_entper");
    } else {
        attribute(lines, "WORK", &work_context, "ID_ENTRY", "\"2\"");
    }
    lines.push(format!(
        "  ok=JE_CreateStartPoint(WORK,\"stp1\",je_piece,{work})"
    ));
    point(lines, &hole_context, setup.start_um);
    let (point_key, mode_key) = if open {
        ("ID_STPENTRY", "ID_STPOPEN")
    } else {
        ("ID_STP", "ID_STPMODE")
    };
    attribute(lines, "WORK", &work_context, point_key, "\"stp1\"");
    attribute(lines, "WORK", &work_context, mode_key, "c_work");
    attribute(lines, "STARTPOINT", &hole_context, "ID_STP_PL", "\"4\"");
    attribute(lines, "STARTPOINT", &hole_context, "ID_DIAMETER", "\"2.0\"");
    if let Some(exit) = setup.exit_um {
        lines.push(format!(
            "  ok=JE_CreateStartPoint(WORK,\"stp2\",je_piece,{work})"
        ));
        attribute(lines, "WORK", &work_context, "ID_STPEXIT", "\"stp2\"");
        attribute(lines, "WORK", &work_context, "ID_STPOPEX", "c_work");
        point(lines, &format!("{work},\"stp2\""), exit);
    }
    // An existing quality name can bind to stale data even after a successful assignment.
    checked(
        lines,
        format!("  ok=JE_GenerateCuts(WORK,je_piece,{work},ASDELETE,NULL)"),
    );
}
