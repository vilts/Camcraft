use super::*;

pub(super) fn check_and_apply(world: &World, mut file: ProjectFile) -> Result<Vec<String>, ProjectError> {
    // Migration hook: step UP one version at a time, each migration exact,
    // before any entity touches the world. Anything newer than this build
    // remains refused. The chain must run its steps sequentially - a v1
    // file passes through v2's effects before v3's are applied.
    let from = file.version;
    if file.version == 1 {
        // v1 -> v2: entities gained `rotation`; v1 had translation-only
        // placements, so identity is the exact migration. Kind and stock
        // fields default (absent = model), so nothing else changes.
        for dto in &mut file.entities {
            dto.rotation = IDENTITY_Q;
        }
        file.version = 2;
    }
    if file.version == 2 {
        // v2 -> v3: `kind` gained tool/operation and their payload fields
        // appeared; every existing entity keeps its fields and defaults
        // the new ones (absent), which is exactly what a v2 file meant.
        file.version = 3;
    }
    if file.version == 3 {
        // v3 -> v4: feature picks graduate from "the src enum can carry
        // them" to "they persist". The shape is already right - a v3 file
        // with a selection reads verbatim, one without gains nothing -
        // so the step-up is the stamp itself.
        file.version = 4;
    }
    if file.version == 4 {
        // v4 -> v5: the M1.5.2 landing shape (patch picks, arc moves,
        // face stepdowns, pocket retract) is declared in the DTOs with
        // serde defaults that mean exactly the v4 behavior, so a v4
        // file needs no content change to live in v5 - the step-up is
        // the stamp itself, like v3 -> v4.
        file.version = 5;
    }
    if file.version == 5 {
        // v5 -> v6: heights became From+Offset expressions, and the
        // wire (`HeightWire`) already read a bare number as the exact
        // `Fixed` expression - every v5 height keeps its z to the bit.
        // Like v4 -> v5, the step-up is the stamp itself.
        file.version = 6;
    }
    if file.version == 6 {
        // v6 -> v7: contour gained the side choice, and the wire already
        // reads an absent `side` as `auto` - which is exactly what a v6
        // contour meant, since Auto resolves at generate time and the
        // stored path is unchanged. Step-up is the stamp itself.
        file.version = 7;
    }
    if file.version == 7 {
        // v7 -> v8: document units arrive as a top-level `units` field.
        // Every v7 file displayed millimeters (its numbers always were
        // millimeters), and the field's default - absent means mm - is
        // exactly that. Step-up is the stamp itself, like v6 -> v7.
        file.version = 8;
    }
    if file.version == 8 {
        // v8 -> v9: the Lead vocabulary grows a tangential member, and
        // `lead` has been a required key since v1 - no v8 file can
        // contain the new tag, and no v8 file needs a content change to
        // live in v9. Step-up is the stamp itself, like v7 -> v8.
        file.version = 9;
    }
    if file.version == 9 {
        // v9 -> v10: drilling arrives - the `drill` params variant, the
        // `drill`/`tap`/`bore` tool-kind tags, `tip_angle`/`pitch`, and
        // cached `cycles`. A v9 file can contain none of them (the tags
        // and keys did not exist), and the serde defaults for the two
        // optional-shaped additions (`cycles` -> empty, tool floats ->
        // the inert 0.0) are exactly v9's behavior. No content change;
        // step-up is the stamp itself, like every bump since v6.
        file.version = 10;
    }
    if file.version == 10 {
        // v10 -> v11: the 3-D pocket arrives as one new params variant
        // (`pocket3d`), which no v10 file can contain - the tag did not
        // exist. Nothing existing changed shape, so no content change;
        // step-up is the stamp itself, like every bump since v6.
        file.version = 11;
    }
    if file.version == 11 {
        // v11 -> v12: the 3-D pocket's rest pass arrives as one new
        // params key (`rest_rough`), absent from every v11 file - the
        // serde default is false, which is exactly what v11 generated.
        // Nothing existing changed shape; step-up is the stamp itself,
        // like every bump since v6.
        file.version = 12;
    }
    if file.version == 12 {
        // v12 -> v13: the constant-engagement strategy arrives as one
        // new params key (`max_engagement`) and one new `strategy` tag
        // (`adaptive`), neither of which any v12 file can contain - the
        // serde default of 45° is exactly what a v12 file meant, since
        // its rings were stepover-spaced and the angle reads only under
        // Adaptive. No content change; step-up is the stamp itself, like
        // every bump since v6.
        file.version = 13;
    }
    if file.version == 13 {
        // v13 -> v14: the feed modulation arrives as one new params key
        // (`feed_opt`), absent from every v13 file - the serde default
        // is off, which is exactly what v13 generated (every F word at
        // the programmed feed). No content change; step-up is the stamp
        // itself, like every bump since v6.
        file.version = 14;
    }
    if file.version == 14 {
        // v14 -> v15: the 3-D finish arrives as one new params variant
        // (`finish3d`), which no v14 file can contain - the tag did not
        // exist. Nothing existing changed shape; step-up is the stamp
        // itself, like every bump since v6.
        file.version = 15;
    }
    if file.version == 15 {
        // v15 -> v16: the parallel finish arrives as two new keys
        // inside `finish3d` (`pattern`, `stepover`), absent from every
        // v15 file - the serde defaults (`waterline`, derived
        // stepover) are exactly what v15 ran. No content change;
        // step-up is the stamp itself, like every bump since v6.
        file.version = 16;
    }
    if file.version == 16 {
        // v16 -> v17: one more new key inside `finish3d` - `ramp`,
        // absent from every v16 file, whose serde default (false: the
        // hop) is exactly what v16 ran. Same pure step-up, same law.
        file.version = 17;
    }
    if file.version == 17 {
        // v17 -> v18: the surface-finish pair inside `finish3d` -
        // `min_step` (absent: None, the march's own floor) and
        // `ramp_angle` (absent: 45°, the slope limit the raster's
        // jurisdiction already granted). Both defaults are exactly
        // what v17 ran; no content change. Same pure step-up, same
        // law.
        file.version = 18;
    }
    if file.version == 18 {
        // v18 -> v19: the machine arrives as a top-level `machine` key.
        // No v18 file can contain it - the checker did not exist - and
        // the field's default, absent means no machine, is exactly what
        // every v18 document was: a program judged against nothing,
        // which is what the unstated-limit law already does with an
        // empty machine. Step-up is the stamp itself, like v7 -> v8.
        file.version = 19;
    }
    if file.version == 19 {
        // v19 -> v20: the model `rev` key (M3.1's revision counter). No
        // v19 file can contain it - the component did not exist - and the
        // field's default, absent means 0, is exactly what every v19
        // document was: geometry with no remembered landings. The
        // staleness machinery needs nothing migrated - it recomputes the
        // signature from the persisted facets. Step-up is the stamp
        // itself, like v7 -> v8 and v18 -> v19.
        file.version = 20;
    }
    if file.version == 20 {
        // v20 -> v21: setups arrive (M3.2 stage A) as a `setup` kind, a
        // `setup` name inside each operation, and a top-level
        // `active_setup` key. No v20 file can contain any of them - the
        // kind tag did not exist - and the defaults read as exactly the
        // v20 world: no setup rows and no setup names is the implicit-
        // setup law's legacy mode verbatim, and an absent active choice
        // resolves to the first setup by name (nothing). No content
        // change; step-up is the stamp itself, like every bump since v6.
        file.version = 21;
    }
    if file.version == 21 {
        // v21 -> v22: fixtures arrive (M3.2 stage B) as an optional
        // `fixture` marker on model entities - a tag, not a new kind,
        // because a fixture IS the model the user loaded. No v21 file
        // can contain it: the component did not exist, and its default
        // (no marker) is exactly the v21 collision world. Step-up is
        // the stamp itself, like every bump since v6.
        file.version = 22;
    }
    if file.version == 22 {
        // v22 -> v23: the post choice arrives as one optional top-level
        // `post` key (M3.3 stage A). No v22 file can contain it - the
        // component did not exist, and its default (absent, the
        // built-in) is exactly the v22 posting world: one embedded
        // script, no choices. Step-up is the stamp itself, like every
        // bump since v6.
        file.version = 23;
    }
    if file.version == 23 {
        // v23 -> v24: the 5-axis twins arrive as two new params
        // variants (`contour5d`, `parallel5d`, M5.1 stage C), which no
        // v23 file can contain - the kind tags did not exist, and
        // `validate_kind` already refuses an unknown params tag before
        // anything is written. No existing shape changed; step-up is
        // the stamp itself, like every bump since v6.
        file.version = 24;
    }
    if file.version == 24 {
        // v24 -> v25: the swarf family arrives as two new params
        // variants (`swarf`, `advanced_swarf`, M5.1 stage D), under
        // the same law as the twins above: no v24 file can contain
        // them, and `validate_kind` already refuses the tags before
        // anything is written. No existing shape changed.
        file.version = 25;
    }
    if file.version == 25 {
        // v25 -> v26: the rotary twins arrive as two new params
        // variants (`rotary_contour`, `rotary_parallel`, M5.1 stage
        // E), under the same law: no v25 file can contain them, and
        // `validate_kind` already refuses the tags before anything is
        // written. No existing shape changed - the fitted axis is
        // model data, so the file gains no key for it.
        file.version = 26;
    }
    if file.version == 26 {
        // v26 -> v27: the rotary rough arrives as one new params
        // variant (`rotary_offset`, M5.1 stage E2), under the same
        // law: no v26 file can contain it, and `validate_kind`
        // already refuses the tag before anything is written. No
        // existing shape changed.
        file.version = 27;
    }
    if file.version == 27 {
        // v27 -> v28: the stamped ladder arrives as one new params
        // variant (`multi_axis_rough`, M5.1 stage E3), under the same
        // law: no v27 file can contain it, and `validate_kind`
        // already refuses the tag before anything is written. No
        // existing shape changed - the posed axis lives in the
        // toolpath, not the file.
        file.version = 28;
    }
    if file.version == 28 {
        // v28 -> v29: shank avoidance arrives as one new params key
        // (`shank_avoid`, M5.1 stage F) inside the three axis-law
        // kinds, whose serde default (false) is exactly what v28 ran.
        // Pure step-up, like every bump since v6: no v28 file means
        // anything else by omitting it.
        file.version = 29;
    }
    if file.version == 29 {
        // v29 -> v30: the cache's warning alphabet gains one variant
        // (`sweep_hit`, M5.1 stage G's law (c3)). Nothing to rewrite:
        // no v29 file can contain a tag its generator never knew, and
        // a cached verdict from the old laws re-derives stale the
        // moment its op regenerates. The bump is the reader's
        // protection: a v30 file's `sweep_hit` cache is a word a v29
        // build cannot spell, and serde would fail closed anyway.
        file.version = 30;
    }
    if file.version == 30 {
        // v30 -> v31: flowline finishing arrives as one new params
        // variant (`flow5d`, M5.1 stage I), under the twins' law: no
        // v30 file can contain it - the kind tag did not exist - and
        // `validate_kind` already refuses an unknown params tag
        // before anything is written. No existing shape changed -
        // the direction field is a reading of the model, so the file
        // gains no key for it, exactly as the ruling and the rotary
        // axis before theirs.
        file.version = 31;
    }
    if file.version == 31 {
        // v31 -> v32: the drive-curve contour arrives as one new
        // params variant (`multi_axis_contour`, M5.1 stage J), under
        // the twins' law: no v31 file can contain it - the kind tag
        // did not exist - and `validate_kind` already refuses an
        // unknown params tag before anything is written. No existing
        // shape changed - the chains it rides live in the geometry
        // source every kind already files, so the format gains no key.
        file.version = 32;
    }
    if file.version == 32 {
        // v32 -> v33: the turning substrate arrives (M5.2 stage A) -
        // one new params variant (`facing`), one new tool-kind word
        // (`turning`) and the inert-defaulted `nose_radius` key, under
        // the twins' law: no v32 file can contain them - the tags did
        // not exist - and `validate_kind` already refuses an unknown
        // params tag before anything is written. The new key
        // serde-defaults to the 0.0 every mill tool already meant,
        // exactly as `tip_angle` did at v9; the turned profile the
        // ladder reads is the mesh every kind already files.
        file.version = 33;
    }
    if file.version == 33 {
        // v33 -> v34: the profile kinds arrive (M5.2 stage B) as two
        // new params variants (`profile_rough`, `profile_finish`),
        // under the same law: no v33 file can contain them - the kind
        // tags did not exist - and `validate_kind` already refuses an
        // unknown params tag before anything is written. No existing
        // shape changed - the silhouette the rungs eat is the mesh
        // every kind already files.
        file.version = 34;
    }
    if file.version == 34 {
        // v34 -> v35: the blade kinds arrive (M5.2 stage C) as three
        // new params variants (`groove_rough`, `groove_finish`,
        // `part`), one new tool-kind word (`groove`) and the
        // inert-defaulted `width` key, under the twins' law: no v34
        // file can contain them - the tags did not exist - and
        // `validate_kind` already refuses an unknown params tag
        // before anything is written. The new key serde-defaults to
        // the 0.0 every non-blade tool already meant, exactly as
        // `nose_radius` did at v33; the groove the blade rides is the
        // mesh every kind already files.
        file.version = 35;
    }
    if file.version == 35 {
        // v35 -> v36: the threading kind arrives (M5.2 stage D) as one
        // new params variant (`threading`) and one new tool-kind word
        // (`thread`), under the same arrival law: no v35 file can
        // contain them - the tags did not exist - and `validate_kind`
        // already refuses an unknown params tag before anything is
        // written. The thread's V rides the tool's `width` +
        // `tip_angle` keys, both already inert-defaulted; the modeled
        // thread is the mesh every kind already files.
        file.version = 36;
    }
    if file.version == 36 {
        // v36 -> v37: the contour kinds arrive (M5.2 stage E) as
        // three new params variants (`chamfering`, `taper_turning`,
        // `multi_pass_turning`) and no new tool-kind word - the
        // rides stand on the turning insert's nose, which v36
        // files already carry. Same arrival law: a v36 file cannot
        // contain the new tags, and `validate_kind` refuses an
        // unknown params tag before anything is written.
        file.version = 37;
    }
    if file.version == 37 {
        // v37 -> v38: the form kind arrives (M5.2 stage E2) as one
        // new params variant (`form_turning`) and one new tool-kind
        // word (`form`), under the same arrival law: no v37 file can
        // contain them - the tags did not exist - and
        // `validate_kind` already refuses an unknown params tag
        // before anything is written. The crown and the band ride
        // the tool's `nose_radius` + `width` keys, both already
        // filed; the modeled formed surface is the mesh every kind
        // already files.
        file.version = 38;
    }
    if file.version == 38 {
        // v38 -> v39: the boring kind arrives (M5.2 stage F) as one
        // new params variant (`boring_turning`) and no new tool-kind
        // word - the `bore` kind has filed since the drilling days,
        // and its ground nose rides the `nose_radius` key v33 files
        // already carry. Same arrival law: a v38 file cannot contain
        // the new tag, and `validate_kind` refuses an unknown params
        // tag before anything is written. The hole the bar rides is
        // the mesh every kind already files.
        file.version = 39;
    }
    if file.version == 39 {
        // v39 -> v40: the drill op grows its radial list (M5.2 stage
        // F2) as one optional `cross` key on the existing `drill`
        // variant - absent decodes (via the serde default) to the
        // empty list every v39 document means, and a pure-Z file
        // re-saves without the key, byte for byte. No new tag, no
        // new kind: a v39 file cannot contain `cross` because the
        // key did not exist.
        file.version = 40;
    }
    if file.version == 40 {
        // v40 -> v41: the theme key arrives (M6.1 C) as one optional
        // top-level field - absent decodes (via the serde default) to
        // the built-in `dark` every v40 document wears, and a no-theme
        // file re-saves without the key, byte for byte. No new tag, no
        // new kind: a v40 file cannot contain it because the key did
        // not exist.
        file.version = 41;
    }
    if file.version == 41 {
        // v41 -> v42: the ribbon pin key arrives (M6.2) as one
        // optional top-level field - absent decodes (via the serde
        // default) to an unpinned ribbon, which is what every v41
        // document has, and a pinned-none file re-saves without the
        // key, byte for byte. No new tag, no new kind: a v41 file
        // cannot contain it because the key did not exist.
        file.version = 42;
    }
    if file.version == 42 {
        // v42 -> v43: the two 2D roughing kinds arrive (M2.4 A) IN
        // THE MIDDLE of the 2D mill's run - the ribbon's contiguity
        // law says a section's rows are a run, so the new tags are
        // 5 and 6 and everything from the old Drill row up shifts by
        // two. A v42 file cannot contain the new variants (the
        // arrival law), but it CAN carry pins by index, and the
        // table those indices named has moved: from 5 up, +2. Every
        // other number in the file is a name or a tag-by-value, and
        // both survived the insert untouched.
        for idx in &mut file.pinned {
            if *idx >= 5 {
                *idx += 2;
            }
        }
        file.version = 43;
    }
    if file.version == 43 {
        // v43 -> v44: the engrave twin, the chamfer ride and the
        // circular ladder arrive (M2.4 B) after Trace in the 2D mill's
        // run - the ribbon's contiguity law again, so the new tags are
        // 7, 8 and 9 and everything from the old Drill row up shifts
        // by three. A v43 file cannot contain the new variants (the
        // arrival law), but it CAN carry pins by index: from 7 up, +3.
        for idx in &mut file.pinned {
            if *idx >= 7 {
                *idx += 3;
            }
        }
        file.version = 44;
    }
    if file.version == 44 {
        // v44 -> v45: the boring ride and the thread mill arrive
        // (M2.4 C) IN the runs they belong to - the partition law
        // (ribbon.rs) says a section's rows are a contiguous run,
        // and the family law says the turning rows stay the table's
        // tail, so a tail-append is not a legal seat: Thread closes
        // the Mill2D run at 10, Bore joins Drilling behind the
        // drill at 12. The two insertions cut at two boundaries, so
        // the rung is the M2.4 A/B choreography with two steps:
        // the old Drill row (10) takes the Thread's +1, and from
        // the old Pocket3D row (11) up both new rows count, +2. A
        // v44 file cannot contain the new variants (the arrival
        // law); its pins are indices, and the table they named
        // moved under them.
        for idx in &mut file.pinned {
            if *idx >= 11 {
                *idx += 2;
            } else if *idx == 10 {
                *idx += 1;
            }
        }
        file.version = 45;
    }
    if file.version == 45 {
        // v45 -> v46: the ring ladder's own spacing number arrives
        // (M2.5 A) inside the Finish3D arm - one optional key, whose
        // absence decodes to the scallop law `2sqrt(r*tol)`, exactly
        // what every v45 file ran - and the pattern grammar grows
        // two words a v45 file cannot contain (the arrival law). No
        // row seats or moves, so the pins stand: pure step-up, the
        // v17 -> v18 law. The stamp of every finish op refreshes
        // once, as it did then - one-time staleness is the price of
        // the grammar, not a wrong answer.
        file.version = 46;
    }
    if file.version == 46 {
        // v46 -> v47: the crease pair arrives (M2.5 B) inside the
        // Finish3D arm - one optional key (`pencil_reference`), whose
        // absence decodes to "no reference ball named", exactly what
        // every v46 file ran - and the pattern grammar grows two
        // words a v46 file cannot contain (the arrival law). No row
        // seats or moves, so the pins stand: pure step-up again.
        file.version = 47;
    }
    if file.version == 47 {
        // v47 -> v48: the deburr ride arrives (M2.5 E B) as one new
        // variant a v47 file cannot contain (the arrival law), seated
        // in the middle of the Mill3D run - the partition law says a
        // section's rows are contiguous, so 15 is its seat and every
        // old row from the 5-Axis Contour (15) up slides one. The
        // pins are indices, and the table they named moved: one cut,
        // one +1. The stamp digest writes the kind's tag, so every
        // op behind the seat regenerates exactly once - one-time
        // staleness is the price of the seat, not a wrong answer.
        for idx in &mut file.pinned {
            if *idx >= 15 {
                *idx += 1;
            }
        }
        file.version = 48;
    }
    if file.version == 48 {
        // v48 -> v49: the strategy grammar grows one word (M11.4 C,
        // `offset_rings` - the pre-C Adaptive ring tiling kept as a
        // choice) that a v48 file cannot contain (the arrival law).
        // No row seats or moves, so the pins stand: pure step-up.
        // A v48 `adaptive` reads as the HSM plan it now names; its
        // stamp digest changed, so it regenerates once, saying so.
        file.version = 49;
    }
    if file.version == 49 {
        // v49 has no explicit order or display labels; the optional keys
        // default to name order and the stable entity name respectively.
        file.version = 50;
    }
    if file.version == 50 {
        // v50 setups pick no program offset; the absent key reads as
        // the inferred per-operation law they posted with (#196).
        file.version = 51;
    }
    if (51..=PROJECT_VERSION).contains(&file.version) {
        // v51 -> v52 (#329): a work-offset height stops naming its own
        // slot - the setup's is program zero. Agreeing setups take the
        // slot, disagreeing ones refuse by name; a v52+ file is checked
        // slot-free (`wcs_migrate`).
        super::wcs_migrate::settle(&mut file, from)?;
        file.version = file.version.max(52);
    }
    if file.version == 52 {
        // v52 -> v53 (#326): a setup may hold the part turned. Absent
        // is "as modelled" - what every v52 file machined.
        file.version = 53;
    }
    let mut notes = Vec::new();
    if (53..=PROJECT_VERSION).contains(&file.version) {
        // v53 -> v54 (#342): one WCS per setup - each setup keeps the
        // origin of the slot it posts, slots no setup posts are dropped
        // with a note; a v54+ file is checked one-WCS (`wcs_one`).
        // v54 -> v55 (#343): a setup may own its machine and post;
        // absent follows the document's. v55 -> v56 (#316): a setup may
        // be placed relative to another; absent links nothing. v56 -> v57
        // (#320): a setup may pick its stock source; absent is #317's
        // implicit rule. v57 -> v58 (#368): a tool may carry a modelled
        // body; absent is the holder cylinder every older tool judged.
        // None of these rungs needs a rewrite.
        notes = super::wcs_one::settle(&mut file, from)?;
        file.version = PROJECT_VERSION;
    }
    if file.version != PROJECT_VERSION {
        return Err(ProjectError::UnsupportedVersion(file.version));
    }
    // The document's unit first (it belongs to the whole file, not to an
    // entity): an unspellable value is a corrupt file, refused before any
    // world write. Opening always lands the file's value - including mm,
    // so a stale singleton from the previous document can never leak in.
    // The post options (issue #34): an unspellable word is a corrupt
    // file, refused before any world write; absent is the default.
    let post_options = crate::schema::PostOptions {
        threading: match file.post_options.as_ref().and_then(|o| o.threading.as_deref()) {
            None | Some("g32") => crate::schema::ThreadingOutput::G32Passes,
            Some("g76") => crate::schema::ThreadingOutput::G76Cycle,
            Some(other) => return Err(corrupt(format!("unknown threading output {other:?}"))),
        },
        // The top-level switches (#110, #111): an absent key is off.
        skip_stops: file.skip_stops,
        per_op: file.per_op_files,
        smoothing: file.smoothing,
        // #596: an absent key leaves the program's unit modal.
        end_units: match file.post_options.as_ref().and_then(|o| o.end_units.as_deref()) {
            None => crate::schema::EndUnits::Leave,
            Some("inch") => crate::schema::EndUnits::Inch,
            Some("mm") => crate::schema::EndUnits::Millimeters,
            Some(other) => return Err(corrupt(format!("unknown end units {other:?}"))),
        },
    };
    let doc_units = match file.units.as_deref() {
        None | Some("mm") => Unit::Millimeters,
        Some("in") => Unit::Inches,
        Some(other) => return Err(corrupt(format!("unknown units {other:?}"))),
    };
    // Whole-file validation BEFORE any world write: a corrupt entity deeper
    // in the list must not leave a half-loaded document behind.
    for dto in &file.entities {
        validate_kind(dto, dto.kind.unwrap_or(EntityKind::Model))?;
    }
    super::setup_links::check(&file)?;
    let keys: std::collections::HashSet<&str> = file.entities.iter()
        .filter(|e| e.kind == Some(EntityKind::Operation))
        .map(|e| e.name.as_str()).collect();
    let mut labels = std::collections::HashSet::new();
    for dto in &file.entities {
        if let Some(op) = &dto.operation {
            let label = op.label.as_deref().unwrap_or(&dto.name);
            if !labels.insert(label) || (label != dto.name && keys.contains(label)) {
                return Err(corrupt(format!("operation label '{label}' conflicts with another operation")));
            }
        }
    }
    // #813: a machine embedded before #804 may hold a table A the
    // index-swing clause cannot judge; say so by name.
    notes.extend(super::swing_note::notes(&file));
    if let Some(r) = file.machine.as_ref().and_then(|m| crate::machine::tooling_refusal(&m.machine)) {
        return Err(corrupt(format!("machine definition {r}")));
    }
    if let Some(m) = &file.machine
        && !m.machine.is_sane()
    {
        return Err(corrupt(
            "machine definition states a limit that cannot be meant",
        ));
    }
    // The workpiece material (M13.2): a word of the built-in table, or
    // a named refusal - never a silent "no material".
    let material = match file.material.as_deref() {
        None => None,
        Some(w) => Some(crate::tools::WorkMaterial::from_word(w).ok_or_else(|| {
            corrupt(format!("unknown workpiece material {w:?}"))
        })?),
    };
    if file.active_setup.as_deref().is_some_and(str::is_empty) {
        return Err(corrupt(
            "the active setup names nothing - an empty setup name",
        ));
    }
    units::set(world, doc_units);
    // Opening lands the machine too - including the mute state, so a
    // stale singleton from the previous document can never leak in
    // (the units law, restated for the checker's input).
    match file.machine.as_ref() {
        Some(m) => crate::machine::set(world, &m.path, Some(&m.machine)),
        None => crate::machine::set(world, "", None),
    }
    // Opening lands the active-setup choice too - including None, so a
    // stale singleton from the previous document can never leak in (the
    // units law, restated for setups). A name matching no setup row is
    // legal: `setup::active`'s existence fallback is the same law the
    // live world uses mid-history, so the file reads as it was saved.
    crate::setup::set_active(world, file.active_setup);
    // Opening lands the post choice too - including the default, so a
    // stale singleton from the previous document can never leak in (the
    // units law again). Paths and registry names are landed as written,
    // unvalidated: each names a script, not a limit, and the post-time
    // refusal is the net (`script_source` reads both, and both refuse
    // by name when gone).
    crate::posting::post_options_set(world, post_options);
    crate::posting::post_set(
        world,
        &match file.post {
            Some(PostDto::Named(name)) => crate::schema::PostRef::Named(name),
            Some(PostDto::File(path)) => crate::schema::PostRef::File(path),
            None => crate::schema::PostRef::Builtin,
        },
    );
    // The theme is not the document's (#877): opening leaves the user's
    // preference as it is, whatever an older file's `theme` key says.
    // Opening lands the pins too - including the empty default, so a
    // stale singleton from the previous document can never leak in
    // (the theme's law, worn in the pin's clothes).
    crate::ribbon::pinned_set(world, &file.pinned);
    // Opening lands the material too - including None (the units law).
    crate::cutting::material_set(world, material);
    // #1094: a raw-stock-only mark lands only when its source model is in
    // the file - one naming a missing model is read as unmarked (judged),
    // so a model imported later under that name never inherits it.
    let model_names: std::collections::HashSet<String> = file.entities.iter()
        .filter(|e| e.kind.unwrap_or(EntityKind::Model) == EntityKind::Model)
        .map(|e| e.name.clone()).collect();
    for dto in file.entities {
        let kind = dto.kind.unwrap_or(EntityKind::Model);
        let entity = model::spawn(world, &dto.name);
        model::set_transform(
            world,
            &entity,
            Location {
                pos: Point3::new(dto.translation[0], dto.translation[1], dto.translation[2]),
                rot: dto.rotation,
            },
        );
        match kind {
            EntityKind::Model => {
                if let Some(tris) = dto.facets {
                    model::set_facets(world, &entity, dto_to_facets(tris).into());
                }
                // The loader's pin is UNCONDITIONAL (M5.3 A): the counter
                // must end the load exactly where the file remembered it,
                // and set_facets above moved it as every landing does.
                // 0 means the key was absent, i.e. a pre-v20 document:
                // the pin puts back the 0 a v19 loader would have left.
                // A model first updated after such a load lands rev 1 -
                // the counter counts landings this document remembers,
                // not the file's whole history. Staleness never reads
                // it; the stamp's signature does the work.
                model::set_rev(world, &entity, dto.rev);
                // #864: the flag lands with the facets it describes; an
                // absent key, or a value that is no coarse chord, is none.
                crate::model_fidelity::set(world, &entity, dto.coarse_mm);
                // #1054: validate_kind refused a budget out of bounds.
                crate::model_fidelity::set_budget(world, &entity, dto.facet_budget);
            }
            EntityKind::Stock => {
                // validate_kind already refused these; Result, never unwrap.
                let shape = dto
                    .stock
                    .ok_or_else(|| corrupt("stock entity without parameters"))?
                    .to_shape();
                let facets = match &shape {
                    StockShape::FromModel { .. } => dto_to_facets(
                        dto.facets
                            .ok_or_else(|| corrupt("from-model stock is missing its facets"))?,
                    ),
                    _ => stock::stock_mesh(&shape)
                        .filter(|f| !f.is_empty())
                        .ok_or_else(|| corrupt("stock mesh regeneration failed"))?,
                };
                model::set_facets(world, &entity, facets.into());
                // The same UNCONDITIONAL pin as the Model arm (#340): every
                // SetStock lands facets and moves the counter, the save
                // writes it, so the load must end where the file left it.
                // An absent key (0) puts back what that file remembered.
                model::set_rev(world, &entity, dto.rev);
                entity.entity_view(world).set(StockDef {
                    shape: shape.clone(),
                });
                // #1094: the mark lands only on a from-model stock, naming
                // its source; anywhere else it means nothing and is dropped.
                // The print is the mark's check (third review): no print,
                // no mark - read as unmarked with a note, never trusted.
                if let (true, StockShape::FromModel { source }) = (dto.raw_stock_only, &shape) {
                    let print = dto.raw_stock_print.as_deref().and_then(crate::stock_print::from_text);
                    match print {
                        Some(print) if model_names.contains(source) => {
                            entity.entity_view(world).set(crate::schema::StockSourceOnly { source: source.clone(), print });
                        }
                        Some(_) => notes.push(format!("'{}' was marked raw stock only, but its source model '{source}' is not in the file - read as unmarked", dto.name)),
                        None => notes.push(format!("'{}' was marked raw stock only without the fingerprint of '{source}' it was set on - read as unmarked; tick \"{}\" again if it is raw stock", dto.name, crate::stock_source::RAW_STOCK_ONLY)),
                    }
                }
            }
            EntityKind::Wcs => {
                entity.entity_view(world).add(Wcs);
            }
            EntityKind::Entry => super::program_entry_dto::apply(world, &entity, dto.entry)?,
            EntityKind::Tool => {
                // validate_kind already refused a missing or insane one.
                let tdto = dto
                    .tool
                    .ok_or_else(|| corrupt("tool entity without a definition"))?;
                entity.entity_view(world).set(ToolDef {
                    tool: tdto.to_tool(),
                    number: tdto.number,
                });
                // #368: validate_kind already refused a body that is no body.
                if let Some(b) = &tdto.body {
                    let body = b.to_body(&dto.name).map_err(corrupt)?;
                    crate::tool_body::set_body(world, &entity, Some(&body));
                }
                // #93: validate_kind already refused a cutter that disagrees.
                if let Some(c) = &tdto.cutter {
                    let spun = super::tool_body_wire::cutter_from_dto(&tdto.to_tool(), c, &dto.name).map_err(corrupt)?;
                    crate::tool_cutter::set_cutter(world, &entity, Some(&spun));
                }
            }
            EntityKind::Operation => {
                let odto = dto
                    .operation
                    .ok_or_else(|| corrupt("operation entity without a definition"))?;
                let src = match &odto.src {
                    SrcDto::Stock => SrcRef::Stock,
                    SrcDto::Entity { name } => SrcRef::Entity(name.clone()),
                    SrcDto::Selection { items } => SrcRef::Selection(
                        items
                            .iter()
                            .map(|g| GeoSel {
                                entity: g.entity.clone(),
                                feature: match g.feature {
                                    FeatureSelDto::Face { id } => FeatureSel::Face(id),
                                    FeatureSelDto::Chain { id } => FeatureSel::Chain(id),
                                    FeatureSelDto::Patch { id } => FeatureSel::Patch(id),
                                    FeatureSelDto::Vertex { id } => FeatureSel::Vertex(id),
                                },
                            })
                            .collect(),
                    ),
                };
                entity.entity_view(world).set(Operation {
                    tool: odto.tool,
                    src,
                    params: odto.params.to_params(),
                    stamp: odto.stamp.map(GenStamp),
                    setup: odto.setup,
                    area: odto.area.to_controls(),
                    coolant: odto.coolant,
                    after: odto.after,
                    suppressed: odto.suppressed,
                });
                if odto.label.is_some() || odto.order.is_some() {
                    entity.entity_view(world).set(OperationMeta {
                        label: odto.label,
                        order: odto.order,
                    });
                }
                if let Some(cache) = &odto.cache {
                    entity.entity_view(world).set(cache.to_cache());
                }
            }
            EntityKind::Setup => {
                // validate_kind already refused a missing one.
                let sdto = dto
                    .setup
                    .ok_or_else(|| corrupt("setup entity without a definition"))?;
                // #196: the picked program offset is a G-word in the file;
                // an unspellable one is a corrupt file, never a guess.
                let work_offset = match sdto.work_offset.as_deref() {
                    None => None,
                    Some(w) => Some(crate::schema::WCS_SLOTS.iter().position(|s| *s == w).ok_or_else(|| {
                        corrupt(format!("setup '{}' picks unknown work offset {w:?}", dto.name))
                    })?),
                };
                // #326: a turned part must be a rigid transform; anything
                // else is a corrupt file, never a guess.
                let part_alignment = match &sdto.part_alignment {
                    None => None,
                    Some(a) => Some(a.to_location().ok_or_else(|| {
                        corrupt(format!("setup '{}' holds the part at a placement that is not a rigid transform", dto.name))
                    })?),
                };
                // #343: the setup's own machine, sanity-checked like the
                // document's - a corrupt file, never a guessed limit.
                let machine = match &sdto.machine {
                    None => None,
                    Some(m) => Some(m.to_def(&dto.name).map_err(corrupt)?),
                };
                // #320: an unknown stock-source word is a corrupt file.
                let stock_source = match sdto.stock_source.as_deref() {
                    None => None,
                    Some(w) => Some(crate::setup_stock::StockSource::from_word(w).ok_or_else(|| {
                        corrupt(format!("setup '{}' takes its stock from '{w}', which is no stock source", dto.name))
                    })?),
                };
                // #389: an unspellable own thread output is a corrupt file.
                let post_options = match &sdto.post_options {
                    None => None,
                    Some(o) => Some(o.to_options(&dto.name).map_err(corrupt)?),
                };
                // #1031: a limit no command would accept is a corrupt file,
                // never a guessed one.
                if let Some(why) = sdto.workholding_max_rpm.and_then(crate::setup_workholding::invalid) {
                    return Err(corrupt(format!("setup '{}': {why}", dto.name)));
                }
                entity.entity_view(world).set(SetupDef {
                    stock: sdto.stock,
                    // `wcs_one` left exactly one name on every setup.
                    wcs: match sdto.wcs {
                        WcsDto::One(w) => w,
                        WcsDto::Slots(_) => {
                            return Err(corrupt(format!("setup '{}' still lists work offset slots", dto.name)));
                        }
                    },
                    datum_from: sdto.datum_from,
                    work_offset,
                    part_alignment,
                    machine,
                    post: sdto.post.as_ref().map(SetupPostDto::to_ref),
                    relative_to: sdto.relative_to.clone(),
                    stock_source,
                    post_options,
                    workholding_max_rpm: sdto.workholding_max_rpm,
                });
            }
        }
        // v22: the fixture marker rides a finished model row (the kind
        // match above built the solid; validate_kind refused the tag on
        // anything else). Setting it after keeps the geometry-first
        // order of every other load.
        if let Some(f) = &dto.fixture {
            // #812: a parameter key this build can't read is dropped
            // with a note, never a failed load.
            entity.entity_view(world).set(super::fixture_keys::fixture_def(f, &dto.name, &mut notes));
        }
    }
    // #819: a place on a plate the file does not hold is dropped, never
    // left for the next plate of that name to pick up.
    for name in crate::plate::mount::drop_dangling(world) {
        notes.push(format!("fixture '{name}': plate location dropped on load: no such plate"));
    }
    Ok(notes)
}
