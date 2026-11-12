## #923 - a changed post script or machine file selects the suites that post through it

**Decided.** `cargo xtask gate quick` maps `assets/posts/<stem>.cps` and
`assets/machines/<stem>.mch` (new `xtask/src/assets.rs`) to every suite under
`crates/*/tests` whose source names `<stem>.cps`/`.mch`, quotes `"<stem>"`
(the conformance tables do), or walks the whole asset directory
(`assets/posts/{..}`, `read_dir` of `machines`). Those suites run whole. An
asset no suite matches is listed in the plan ("NO SUITE covers this asset"),
never silently dropped. A deleted asset selects nothing.

**Why.** Only a few suites carry a Gate map entry for assets, so a map-only
rule would select nothing for most posts; the source references are the
ground truth of "posts through it" and need no new annotation. Quick on
`fanuc.cps` now selects 38 app suites.

**Considered and dropped.** (a) Requiring a Gate map line per asset: dozens
of suite edits, drifts silently. (b) Failing the gate on an unmapped asset:
a new post legitimately exists before its suite; the plan report suffices.
(c) Running every post suite for any asset: wasteful for a quick tier.

**Cost to change.** When #926 lands per-post suites in `crates/post/tests`,
they name their `.cps` and are picked by the same rule; the app-side
over-selection can then be narrowed by removing the broad suites' mentions.
