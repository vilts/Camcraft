## #922 - Gate maps name the crate a module lives in

**Decided.** In every app suite's Gate map, a module M4.1 moved out of `app` is named by its owning crate
(`app::post` -> `post::post`, `app::verify` -> `sim::verify`, `app::machining2d` -> `motion::machining2d`,
likewise drilling, finishing3d, multiaxis, turning, links, load, plasma, probing, sim, solid_rest, tools,
orientation, spun, cutter_profile, holder_library, cl, geo2d, stock). Modules that are an app wrapper over a
crate module (`machine`, `stock_sim`, `fixture`) keep `app::X` and gain the crate name beside it, so a change
on either side selects the suite. `heidenhain_cycles` gets a map. An xtask test reads the real headers: it
fails on any `app::<moved>` token or unmapped app suite, and pins that a change under `crates/post/src/post/`,
`crates/sim/src/verify/` and `crates/motion/src/machining2d/` selects the dialect, verify and machining2d suites.

**Why.** quick matches another crate's module as `<crate>::<module>`; the stale `app::<module>` names never
matched, so a post change ran 3 of 28 suites. Fixing the headers keeps the quick.rs matcher unchanged.

**Dropped.** Moving the 36 misplaced tests into their crates (the #919 follow-up; bigger, needs a golden home);
teaching quick.rs that `app::post` aliases `post::post` (hides the stale name instead of fixing it);
`app::geometry` left alone (a real app module, and `geometry` is a shared crate so quick runs batch anyway).

**Cost to change.** Low: header comments only. The pin test lists the moved modules; a future split adds a name.
