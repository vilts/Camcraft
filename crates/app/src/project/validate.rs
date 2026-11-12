use super::*;

/// Structural checks that JSON typing cannot express. All run before this
/// entity reaches the world; a corrupt field never half-loads (Prime
/// Directive: validate early, fail cleanly).
pub(super) fn validate_kind(dto: &EntityDto, kind: EntityKind) -> Result<(), ProjectError> {
    // v22, once for every kind: the fixture marker rides a MODEL row
    // and nothing else - stock, offsets, tools, operations and setups
    // wearing it would be a file describing a CAM concept that has no
    // world representation.
    if dto.fixture.is_some() && !matches!(kind, EntityKind::Model) {
        return Err(corrupt(format!(
            "'{}' wears a fixture tag but is not a model",
            dto.name
        )));
    }
    super::program_entry_dto::validate(dto, kind)?; // #155
    // #1054: a recorded STEP facet budget rides a model row only, inside
    // the bounds an import would accept.
    if let Some(b) = dto.facet_budget {
        if !matches!(kind, EntityKind::Model) {
            return Err(corrupt(format!("'{}' records a STEP facet budget but is not a model", dto.name)));
        }
        crate::geometry::import::step_budget::check(b).map_err(|e| corrupt(format!("'{}': {e}", dto.name)))?;
    }
    match kind {
        EntityKind::Entry => {}
        EntityKind::Model => {
            if dto.stock.is_some()
                || dto.tool.is_some()
                || dto.operation.is_some()
                || dto.setup.is_some()
            {
                return Err(corrupt(format!(
                    "'{}' is a model carrying setup or CAM parameters",
                    dto.name
                )));
            }
            if dto
                .fixture
                .as_ref()
                .is_some_and(|f| f.setup.as_deref().is_some_and(str::is_empty))
            {
                return Err(corrupt(format!(
                    "'{}' is tagged a fixture of an empty setup name",
                    dto.name
                )));
            }
        }
        EntityKind::Stock => {
            if dto.tool.is_some() || dto.operation.is_some() || dto.setup.is_some() {
                return Err(corrupt(format!(
                    "'{}' is stock carrying tool, operation or setup parameters",
                    dto.name
                )));
            }
            // v21: setup furniture may live under a private name - the
            // two shapes are exactly what `setup::stock_entity_name`
            // resolves (the implicit-setup law's legacy name, or the
            // "{SetupN}/Stock" a declared setup points at by default).
            if dto.name != STOCK_NAME && !dto.name.ends_with("/Stock") {
                return Err(corrupt(format!(
                    "a stock entity must be named '{STOCK_NAME}' or a setup's \"<setup>/Stock\", found '{}'",
                    dto.name
                )));
            }
            let Some(stock) = &dto.stock else {
                return Err(corrupt("stock entity without parameters"));
            };
            let shape = stock.to_shape();
            if !shape.is_sane() || !shape.is_bounded() {
                return Err(corrupt(format!(
                    "stock parameters are not usable: {stock:?}"
                )));
            }
            match (&shape, &dto.facets) {
                // Parametric stock is regenerated, never stored as floats;
                // FromModel stock has nothing to regenerate from.
                (StockShape::Box { .. } | StockShape::Cylinder { .. }, Some(_)) => {
                    return Err(corrupt("parametric stock must not store facets"));
                }
                (StockShape::FromModel { .. }, None) => {
                    return Err(corrupt("from-model stock is missing its facets"));
                }
                (StockShape::FromModel { .. }, Some(tris)) if tris.is_empty() => {
                    return Err(corrupt("from-model stock has no geometry"));
                }
                _ => {}
            }
        }
        EntityKind::Wcs => {
            // v21: a setup's WCS lives under a private name; v54 (#342)
            // names it `<setup>/WCS`, older setups kept their slot's
            // name (`G54`, `<setup>/G55`). The word it posts is the
            // setup's label, never this name.
            let name_ok = WCS_SLOTS.contains(&dto.name.as_str())
                || dto.name.rsplit_once('/').is_some_and(|(_, last)| {
                    WCS_SLOTS.contains(&last) || last == crate::setup::WCS_SUFFIX
                });
            if !name_ok {
                return Err(corrupt(format!(
                    "'{}' is not a WCS name (expected G54, a setup's \"<setup>/WCS\", or an older \"<setup>/<slot>\")",
                    dto.name
                )));
            }
            if dto.facets.is_some()
                || dto.stock.is_some()
                || dto.tool.is_some()
                || dto.operation.is_some()
                || dto.setup.is_some()
            {
                return Err(corrupt(format!("'{}' is an offset, not a solid", dto.name)));
            }
            // #327: the loader's transform write turns a zero-norm
            // rotation into the identity, which for an offset would post
            // a rotated frame as unrotated - refuse it by name instead.
            if !dto.translation.iter().all(|v| v.is_finite())
                || !dto.rotation.iter().all(|v| v.is_finite())
                || crate::geometry::normalize_quat(dto.rotation).is_none()
            {
                return Err(corrupt(format!(
                    "WCS '{}' has a non-finite or degenerate placement (origin or rotation)",
                    dto.name
                )));
            }
        }
        EntityKind::Tool => {
            if is_furniture_name(&dto.name) {
                return Err(corrupt(format!(
                    "'{}' is reserved for the setup's stock or work offsets, not a tool",
                    dto.name
                )));
            }
            if dto.name.is_empty() {
                return Err(corrupt("a tool entity must be named"));
            }
            if dto.facets.is_some()
                || dto.stock.is_some()
                || dto.operation.is_some()
                || dto.setup.is_some()
            {
                return Err(corrupt(format!("'{}' is a tool, not a solid", dto.name)));
            }
            let Some(tool) = &dto.tool else {
                return Err(corrupt("tool entity without a definition"));
            };
            if !tool.to_tool().is_sane() {
                return Err(corrupt(format!("tool parameters are not usable: {tool:?}")));
            }
            // #221: a stated number is a real tool-table number.
            if let Some(n) = tool.number
                && !crate::tool_numbers::valid(n)
            {
                return Err(corrupt(format!(
                    "tool '{}' has number {n}; tool numbers run {}",
                    dto.name,
                    crate::tool_numbers::RANGE_TEXT
                )));
            }
            // #368: a body is a body this build's envelope law accepts,
            // on a tool it fits - or the file refuses by name.
            if let Some(b) = &tool.body {
                let body = b.to_body(&dto.name).map_err(corrupt)?;
                crate::tool_body::fits(&tool.to_tool(), &body)
                    .map_err(|e| corrupt(format!("tool '{}': {e}", dto.name)))?;
            }
            // #93: a drawn cutter agrees with the tool's numbers, or
            // the file refuses by name.
            if let Some(c) = &tool.cutter {
                super::tool_body_wire::cutter_from_dto(&tool.to_tool(), c, &dto.name).map_err(corrupt)?;
            }
        }
        EntityKind::Operation => {
            if dto.name.is_empty() {
                return Err(corrupt("an operation entity must be named"));
            }
            if dto.facets.is_some()
                || dto.stock.is_some()
                || dto.tool.is_some()
                || dto.setup.is_some()
            {
                return Err(corrupt(format!(
                    "'{}' is an operation, not a solid, tool or setup",
                    dto.name
                )));
            }
            let Some(op) = &dto.operation else {
                return Err(corrupt("operation entity without a definition"));
            };
            if op.tool.is_empty() {
                return Err(corrupt(format!("'{}' names no tool", dto.name)));
            }
            if op.label.as_deref().is_some_and(|s| s.trim().is_empty() || s.len() > 120 || s.chars().any(char::is_control)) {
                return Err(corrupt(format!("'{}' has an invalid operation label", dto.name)));
            }
            if op.setup.as_deref().is_some_and(str::is_empty) {
                return Err(corrupt(format!("'{}' names an empty setup", dto.name)));
            }
            if let SrcDto::Entity { name } = &op.src
                && name.is_empty()
            {
                return Err(corrupt(format!(
                    "'{}' has an empty geometry source",
                    dto.name
                )));
            }
            if let SrcDto::Selection { items } = &op.src
                && (items.is_empty() || items.iter().any(|g| g.entity.is_empty()))
            {
                return Err(corrupt(format!(
                    "'{}' has an empty or half-named geometry selection",
                    dto.name
                )));
            }
            let params = op.params.to_params();
            if !params.is_sane() {
                return Err(corrupt(format!(
                    "operation parameters are not usable for a {} operation '{}'",
                    params.kind(),
                    dto.name
                )));
            }
            if !op.area.to_controls().is_sane_for(&params) {
                return Err(corrupt(format!(
                    "the Boundary settings of '{}' are not usable for a {} operation",
                    dto.name,
                    params.kind()
                )));
            }
            if let Some(cache) = &op.cache {
                let path = cl_path(&cache.moves, &cache.cycles);
                if path.is_empty() {
                    return Err(corrupt(format!(
                        "'{}' carries a cached path with no moves",
                        dto.name
                    )));
                }
                if !path.all_finite() {
                    return Err(corrupt(format!(
                        "'{}' cache has non-finite moves",
                        dto.name
                    )));
                }
                if !path.is_chained() {
                    return Err(corrupt(format!(
                        "'{}' cache is not a linked path",
                        dto.name
                    )));
                }
                if path.cycles.iter().any(|c| c.first_move >= path.moves.len()) {
                    return Err(corrupt(format!(
                        "'{}' cache has a cycle pointing past the end of its chain",
                        dto.name
                    )));
                }
            }
        }
        EntityKind::Setup => {
            if dto.name.is_empty() {
                return Err(corrupt("a setup entity must be named"));
            }
            if dto.facets.is_some()
                || dto.stock.is_some()
                || dto.tool.is_some()
                || dto.operation.is_some()
            {
                return Err(corrupt(format!(
                    "'{}' is a setup, not a solid, tool or operation",
                    dto.name
                )));
            }
            let Some(s) = &dto.setup else {
                return Err(corrupt("setup entity without a definition"));
            };
            // A setup always owns a stock and a WCS name; the WCS may
            // name an entity the file does not hold - it is simply not
            // placed yet (the schema's own law) - but an empty string
            // names nothing at all. The rungs left one name (#342).
            let wcs_empty = match &s.wcs {
                WcsDto::One(w) => w.is_empty(),
                WcsDto::Slots(l) => l.iter().any(String::is_empty),
            };
            if s.stock.is_empty() || wcs_empty {
                return Err(corrupt(format!(
                    "'{}' has an empty stock or work-offset name",
                    dto.name
                )));
            }
        }
    }
    Ok(())
}

/// Does this name follow a setup-furniture shape - the legacy global
/// names, or a private "<setup>/Stock" / "<setup>/<slot>" (M3.2 A)? The
/// Tool arm's reserved-name test: a tool wearing a furniture name would
/// poison the by-name lookups in `setup::stock_entity` and friends.
pub(super) fn is_furniture_name(name: &str) -> bool {
    name == STOCK_NAME || WCS_SLOTS.contains(&name) || crate::setup::is_furniture_shape(name)
}

/// Moves and cycles, for the cache's structural checks (the warnings ride
/// along untouched; `Warning`'s serde shape is its own contract). The
/// cycles join the checks on purpose: `all_finite` reads them, and the
/// `first_move` gate below makes sure a warning pointed at a cycle has
/// something in the chain to point at.
pub(super) fn cl_path(moves: &[MoveDto], cycles: &[CycleDto]) -> Toolpath {
    let mut path = Toolpath::new(moves.iter().map(|m| m.to_move()).collect());
    path.cycles = cycles.iter().copied().map(CycleDto::to_cycle).collect();
    path
}
