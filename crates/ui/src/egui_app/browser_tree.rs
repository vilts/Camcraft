//! The browser tree's rows (#343, split from `panes`): each setup is a
//! node with a radio (the active setup) and its own children, in order -
//! WCS, Stock, Machine, Post, Fixtures (each fixture a child), Tool list,
//! Operations (its 1+n operations, in machining order). A document without
//! setups shows the same children straight under the Setup section, for
//! its implicit setup. Models and the document's tool library follow.
//! Rows are pure data (`painted::Row` plus a parallel `Leaf`), so the
//! tree's shape is testable without a harness; the click law is
//! `browser_pane`.
//! Provenance: our own tree layout (issue #343, the maintainer's node
//! list); no HSMWorks material consulted.

use camcraft_app::model;
use camcraft_app::operation;
use camcraft_app::schema::WCS_SLOTS;
use camcraft_app::tools::ToolKind;
use camcraft_app::{setup, setup_mount};

use crate::state::{SetupPage, State, UiTarget};

use super::icons::Icon;
use super::painted;
use super::op_icons::op_icon;
use super::tool_icons::tool_icon;
use super::tree_badges::status_badge;

/// The tree's collapsible top-level sections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowserSection {
    Setup,
    Models,
    Tools,
}

impl BrowserSection {
    pub(super) const fn index(self) -> usize {
        self as usize
    }
}

/// What a browser row's click means. Parallel to the painted Row list.
/// Setup-scoped rows carry their setup's name (`""` - the implicit setup
/// of a document without setups).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Leaf {
    Section,
    Parent(BrowserSection),
    /// The Setups manager row (M3.2 A, #322): target-only, never arms Del.
    Setups,
    /// A setup node: the radio activates it, the row opens its settings.
    Setup(String),
    Wcs(String),
    Stock(String),
    /// The Machine / Post / Fixtures / Tool list nodes.
    Page(String, SetupPage),
    /// A setup's Operations node: the menu row for Paste.
    SetupOps(String),
    Model(String),
    Tool(String),
    Op(String),
    /// A Pause / Home entry of the operation list (#155).
    Entry(String),
}

/// Append one tree row and its parallel leaf. `icon` is the bundled glyph
/// for the row's subject, `None` for headers (and the setup radio).
#[allow(clippy::too_many_arguments)]
pub(super) fn push_row(
    rows: &mut Vec<painted::Row>,
    leaves: &mut Vec<Leaf>,
    label: String,
    suffix: String,
    kind: painted::Kind,
    icon: Option<Icon>,
    selected: bool,
    hidden: bool,
    depth: usize,
    leaf: Leaf,
) {
    rows.push(painted::Row { label, suffix, kind, icon, badge: None, selected, hidden, depth });
    leaves.push(leaf);
}

/// What a Models row says about its model (M3.2 B): the fixture marker
/// rides the suffix column ahead of the hidden marker, and the kind/icon
/// pair picks the vise glyph for steel the program must dodge.
pub(crate) fn model_row_marks(is_fixture: bool, hidden: bool) -> (String, painted::Kind, Icon) {
    let mut suffix = String::new();
    if is_fixture {
        suffix.push_str(" (fixture)");
    }
    if hidden {
        suffix.push_str(" (hidden)");
    }
    (
        suffix,
        if is_fixture { painted::Kind::Fixture } else { painted::Kind::Model },
        if is_fixture { Icon::Fixture } else { Icon::Model },
    )
}

/// A Fixtures child row's suffix (#339 slice 4): how the gate judges
/// the steel - `mesh`, or `box only: <reason>` - then the hidden marker.
pub(crate) fn fixture_row_suffix(mesh: Option<camcraft_app::fixture_view::MeshState>, hidden: bool) -> String {
    let mut suffix = mesh.map(|m| m.label()).unwrap_or_default();
    if hidden {
        suffix.push_str(" (hidden)");
    }
    suffix
}

pub(crate) fn kind_name(k: ToolKind) -> &'static str {
    match k {
        ToolKind::EndMill => "end mill",
        ToolKind::BallNose => "ball nose",
        ToolKind::BullNose => "bull nose",
        ToolKind::Drill => "drill",
        ToolKind::Tap => "tap",
        ToolKind::Bore => "boring bar",
        ToolKind::Turning => "turning insert",
        ToolKind::Groove => "grooving blade",
        ToolKind::Thread => "threading insert",
        ToolKind::Form => "form gauge",
        ToolKind::Taper => "chamfer mill",
        ToolKind::Probe => "touch probe",
        ToolKind::Torch => "plasma torch",
    }
}

/// The whole tree: the Setup section (the manager row, then one node per
/// setup - or the implicit setup's children), Models, and the document's
/// tool library. Empty lists show a faint placeholder row.
pub(crate) fn browser_rows(state: &State) -> (Vec<painted::Row>, Vec<Leaf>) {
    let mut rows = Vec::new();
    let mut leaves = Vec::new();
    let t = state.pane_target();

    let setup_open = state.browser_expanded[BrowserSection::Setup.index()];
    push_row(&mut rows, &mut leaves, "Setup".into(), String::new(), painted::Kind::Parent(setup_open), None, false, false, 0, Leaf::Parent(BrowserSection::Setup));
    if setup_open {
        // The manager row stands whenever the section is open (#322): its
        // page is where the FIRST setup is created.
        push_row(&mut rows, &mut leaves, "Setups".to_owned(), String::new(), painted::Kind::Section, Some(Icon::Setup), matches!(t, UiTarget::Setups), false, 1, Leaf::Setups);
        let names = setup::names(&state.world);
        if names.is_empty() {
            setup_children(state, &t, "", 1, &mut rows, &mut leaves);
        }
        let active = setup::active(&state.world);
        for name in names {
            let is_active = active.as_deref() == Some(name.as_str());
            let selected = matches!(&t, UiTarget::SetupNode(n, SetupPage::Settings) if *n == name);
            let suffix = if is_active { "active".to_owned() } else { String::new() };
            push_row(&mut rows, &mut leaves, name.clone(), suffix, painted::Kind::Setup(is_active), None, selected, false, 1, Leaf::Setup(name.clone()));
            setup_children(state, &t, &name, 2, &mut rows, &mut leaves);
        }
    }

    let models_open = state.browser_expanded[BrowserSection::Models.index()];
    push_row(&mut rows, &mut leaves, "Models".into(), String::new(), painted::Kind::Parent(models_open), None, false, false, 0, Leaf::Parent(BrowserSection::Models));
    if models_open {
        let entities = model::model_entities(&state.world);
        if entities.is_empty() {
            push_row(&mut rows, &mut leaves, "(no models yet)".into(), String::new(), painted::Kind::Section, None, false, true, 1, Leaf::Section);
        }
        for (name, entity) in entities {
            let selected = model::is_selected(&state.world, &entity);
            let hidden = state.hidden_models.contains(&name);
            let (suffix, kind, icon) = model_row_marks(model::is_fixture(&state.world, &entity), hidden);
            let target = matches!(&t, UiTarget::Model(n) if *n == name) || selected;
            push_row(&mut rows, &mut leaves, name.clone(), suffix, kind, Some(icon), target, hidden, 1, Leaf::Model(name));
        }
    }

    // The document's tool LIBRARY (decision #343 (b)): every setup's Tool
    // list is a view over these entries.
    let tools_open = state.browser_expanded[BrowserSection::Tools.index()];
    push_row(&mut rows, &mut leaves, "Tools".into(), String::new(), painted::Kind::Parent(tools_open), None, false, false, 0, Leaf::Parent(BrowserSection::Tools));
    if tools_open {
        let tools: Vec<(String, camcraft_app::tools::Tool)> = model::named_entities(&state.world)
            .into_iter()
            .filter(|(_, e)| model::is_tool(&state.world, e))
            .filter_map(|(name, e)| model::tool_of(&state.world, &e).map(|t| (name, t)))
            .collect();
        if tools.is_empty() {
            push_row(&mut rows, &mut leaves, "(no tools yet)".into(), String::new(), painted::Kind::Section, None, false, true, 1, Leaf::Section);
        }
        for (name, tool) in tools {
            let suffix = format!("Ø{} {}", tool.diameter, kind_name(tool.kind));
            let selected = matches!(&t, UiTarget::Tool(n) if *n == name);
            push_row(&mut rows, &mut leaves, name.clone(), suffix, painted::Kind::Tool, Some(tool_icon(tool.kind)), selected, false, 1, Leaf::Tool(name));
        }
    }
    (rows, leaves)
}

/// One setup's children at `depth` (`name` `""` - the implicit setup).
fn setup_children(
    state: &State,
    t: &UiTarget,
    name: &str,
    depth: usize,
    rows: &mut Vec<painted::Row>,
    leaves: &mut Vec<Leaf>,
) {
    let w = &state.world;
    let ctx = (!name.is_empty()).then_some(name);
    let is_active = ctx.is_none() || setup::active(w).as_deref() == ctx;
    let page_open = |p: SetupPage| matches!(t, UiTarget::SetupNode(n, q) if n == name && *q == p);
    // #342: one WCS per setup - plain "WCS", the word its post writes beside it.
    let word = WCS_SLOTS.get(setup::wcs_slot(w, ctx)).copied().unwrap_or("G54");
    let wcs_hidden = state.hidden_offsets.contains(&setup::wcs_entity_name(w, ctx));
    push_row(rows, leaves, "WCS".to_owned(), word.to_owned(), painted::Kind::Wcs, Some(Icon::WorkOffset), is_active && matches!(t, UiTarget::Wcs), wcs_hidden, depth, Leaf::Wcs(name.to_owned()));
    let stock_hidden = state.hidden_stocks.contains(&setup::stock_entity_name(w, ctx));
    let stock_text = if model::stock_world_aabb_for(w, ctx).is_some() { String::new() } else { "not set".to_owned() };
    push_row(rows, leaves, "Stock".to_owned(), stock_text, painted::Kind::Stock, Some(Icon::Stock), is_active && matches!(t, UiTarget::Stock), stock_hidden, depth, Leaf::Stock(name.to_owned()));
    let own = |owns: bool| if owns { "  (own)" } else { "" };
    let machine = format!(
        "{}{}",
        setup_mount::machine_label(&setup_mount::machine_of(w, ctx)),
        own(setup_mount::own_machine(w, ctx).is_some()),
    );
    push_row(rows, leaves, "Machine".to_owned(), machine, painted::Kind::Section, Some(Icon::Machine), page_open(SetupPage::Machine), false, depth, Leaf::Page(name.to_owned(), SetupPage::Machine));
    let post = format!(
        "{}{}",
        camcraft_app::posting::post_ref_label(&setup_mount::post_of(w, ctx)),
        own(setup_mount::own_post(w, ctx).is_some()),
    );
    push_row(rows, leaves, "Post".to_owned(), post, painted::Kind::Section, Some(Icon::Post), page_open(SetupPage::Post), false, depth, Leaf::Page(name.to_owned(), SetupPage::Post));
    let fixtures = camcraft_app::fixture::tagged(w, ctx);
    push_row(rows, leaves, "Fixtures".to_owned(), count(fixtures.len(), "fixture"), painted::Kind::Section, Some(Icon::Fixture), page_open(SetupPage::Fixtures), false, depth, Leaf::Page(name.to_owned(), SetupPage::Fixtures));
    for (fixture, entity) in fixtures {
        let hidden = state.hidden_models.contains(&fixture);
        let selected = matches!(t, UiTarget::Model(n) if *n == fixture) || model::is_selected(w, &entity);
        let suffix = fixture_row_suffix(state.fixture_mesh_state(&fixture), hidden);
        push_row(rows, leaves, fixture.clone(), suffix, painted::Kind::Fixture, Some(Icon::Fixture), selected, hidden, depth + 1, Leaf::Model(fixture));
    }
    let ops = setup_ops(state, ctx);
    let tools = count(distinct_tools(state, &ops), "tool");
    push_row(rows, leaves, "Tool list".to_owned(), tools, painted::Kind::Section, Some(Icon::Tools), page_open(SetupPage::Tools), false, depth, Leaf::Page(name.to_owned(), SetupPage::Tools));
    push_row(rows, leaves, "Operations".to_owned(), String::new(), painted::Kind::Section, Some(Icon::OpGroup), false, false, depth, Leaf::SetupOps(name.to_owned()));
    if ops.is_empty() {
        push_row(rows, leaves, "(no operations yet)".into(), String::new(), painted::Kind::Section, None, false, true, depth + 1, Leaf::Section);
    }
    for (key, entity) in ops {
        if camcraft_app::program_entry::is_entry(w, &entity) {
            super::tree_entry::entry_row(state, &key, &entity, depth + 1, rows, leaves);
        } else {
            op_row(state, t, &key, &entity, depth + 1, rows, leaves);
        }
    }
}

fn count(n: usize, word: &str) -> String {
    match n {
        0 => String::new(),
        1 => format!("1 {word}"),
        n => format!("{n} {word}s"),
    }
}

/// The setup's operations in program order (`None` - every operation),
/// its Pause / Home entries among them (#155).
fn setup_ops(state: &State, ctx: Option<&str>) -> Vec<(String, flecs_ecs::prelude::Entity)> {
    let w = &state.world;
    camcraft_app::program_entry::sequence(w)
        .into_iter()
        .filter(|(_, e)| {
            ctx.is_none_or(|s| {
                setup::belongs(w, camcraft_app::program_entry::declared_setup(w, e).as_deref(), s)
            })
        })
        .collect()
}

fn distinct_tools(state: &State, ops: &[(String, flecs_ecs::prelude::Entity)]) -> usize {
    let mut seen: Vec<String> = Vec::new();
    for (_, e) in ops {
        if let Some(op) = model::operation_of(&state.world, e)
            && !seen.contains(&op.tool)
        {
            seen.push(op.tool);
        }
    }
    seen.len()
}

/// One operation row: kind, generation tag, warnings, flow marks, and a
/// BLOCKED reason when the tool law refuses it.
fn op_row(
    state: &State,
    t: &UiTarget,
    name: &str,
    entity: &flecs_ecs::prelude::Entity,
    depth: usize,
    rows: &mut Vec<painted::Row>,
    leaves: &mut Vec<Leaf>,
) {
    let w = &state.world;
    let (kind, icon) = model::operation_of(w, entity)
        .map(|op| {
            let k = op.params.kind();
            (k.to_string(), op_icon(k))
        })
        .unwrap_or_else(|| (String::new(), Icon::Generate));
    let status = operation::status_of(w, entity);
    let tag = match status {
        operation::OpStatus::Never => "never",
        operation::OpStatus::Ok => "ok",
        operation::OpStatus::Stale => "STALE",
    };
    let warns = model::toolpath_of(w, entity).map_or(0, |tp| tp.warnings.len());
    let warn_text = match warns {
        0 => String::new(),
        1 => "  1 warning".to_owned(),
        n => format!("  {n} warnings"),
    };
    // #110: a suppressed op and a stop / home after it say so on the row.
    let flow = model::operation_of(w, entity).map_or_else(String::new, |op| {
        let mut s = String::new();
        if op.suppressed {
            s.push_str("  SUPPRESSED");
        }
        if !op.after.is_default() {
            s.push_str("  then ");
            s.push_str(op.after.label());
        }
        s
    });
    let problem = operation::tool_gate::tool_problem_of(w, entity);
    let badge = status_badge(status, problem.is_some(), warns);
    let suffix = match problem {
        Some(why) => format!("{kind}  BLOCKED{warn_text}{flow}  {why}"),
        None => format!("{kind}  {tag}{warn_text}{flow}"),
    };
    let hidden = state.hidden_paths.iter().any(|p| p == name);
    let selected = matches!(t, UiTarget::Operation(n) if n == name);
    let label = model::operation_label(w, name, entity);
    push_row(rows, leaves, label, suffix, painted::Kind::Op, Some(icon), selected, hidden, depth, Leaf::Op(name.to_owned()));
    if let Some(row) = rows.last_mut() {
        row.badge = Some(badge);
    }
}
