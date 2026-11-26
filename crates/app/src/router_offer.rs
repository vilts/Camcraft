//! What a router machine offers on the Milling tab (#2410): the one list
//! of router options that the UI pages and the MCP job view both read.
//! A router is a mill in every rule; it only offers more. A machine
//! that is not a router (a mill, a lathe, a declared table) offers none
//! of them, and a project with no machine offers all of them, because
//! nothing says that the job is not a router job.
//!
//! Provenance: the maintainer's router-kind decision on #2410 (no Router
//! tab; the router options live on the Milling tab); our own page law.

use flecs_ecs::core::WorldGet;
use flecs_ecs::prelude::World;

use crate::machine::{self, Machine};
use crate::schema::MachineDef;

/// One option that only a router offers. A router sub-issue adds its
/// row here when it ships (#2410 scope 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouterOption {
    /// The Setups page's flat-sheet "Sheet job" group (#3070).
    SheetJob,
    /// Dogbone and T-bone corner relief on the 2D clearing pages (#2400).
    CornerRelief,
}

impl RouterOption {
    /// Every router option, in page order.
    pub const ALL: [RouterOption; 2] = [RouterOption::SheetJob, RouterOption::CornerRelief];

    /// The word MCP reports for the option.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            RouterOption::SheetJob => "sheet_job",
            RouterOption::CornerRelief => "corner_relief",
        }
    }
}

/// The router options that `machine` offers: all of them for a router
/// or for no machine, none for any other machine.
#[must_use]
pub fn router_options_for(machine: Option<&Machine>) -> &'static [RouterOption] {
    match machine {
        None => &RouterOption::ALL,
        Some(m) if machine::is_router(Some(m)) => &RouterOption::ALL,
        Some(_) => &[],
    }
}

/// The router options that the setup's machine offers: its own machine,
/// else the document's (`None`: the first setup, or the document when it
/// has no setups). Reads the singleton without a clone, as the pages ask
/// each frame.
#[must_use]
pub fn router_options(world: &World, setup: Option<&str>) -> &'static [RouterOption] {
    if let Some(own) = crate::setup_mount::own_machine(world, setup) {
        return router_options_for(own.machine.as_ref());
    }
    world
        .try_get::<&MachineDef>(|d| router_options_for(d.machine.as_ref()))
        .unwrap_or(&RouterOption::ALL)
}

/// Whether the setup's machine offers `option`.
#[must_use]
pub fn offers(world: &World, setup: Option<&str>, option: RouterOption) -> bool {
    router_options(world, setup).contains(&option)
}
