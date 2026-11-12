## #996 - the command-line post reads a vendor .mch the way the app does

**What was decided.** `post --machine <file>` (`crates/post/src/bin/post.rs`)
now posts with `post::machine::mirror(parse_mch(file))` when the file is a
vendor kinematics tree (it has `kinematics.default.parts` - every shipped
machine), the same value the app's `posting_mounts::machine_value_for`
hands the engine. A file already written in the static grammar (no
`kinematics.default.parts`) still posts as written. The inverted-axis
refusal (`inverted_post_refusal`) and the parse refusals run first, on the
raw file, unchanged; the mirror carries the file's inverted axes as
`invertedAxes`, so `invert::apply` mirrors or refuses exactly as it did
through the raw door (the three `axis_direction_cli` tests hold).

**Why.** `script_props` reads only the static grammar, so the raw vendor
file gave the script no rotaries, spindle max, coolant codes or rates: the
CLI refused tilted streams the app posts ("machine without a tilting
rotary") and dropped the spindle cap and coolant words. The mirror is the
app's one seam for this and already lives in camcraft-post, so no new crate
edge (camcraft-post does not depend on camcraft-app).

**Considered and dropped.**
- Always mirroring the parsed machine: a static-grammar file parses to an
  empty `Machine` and would lose every figure it states - a regression for
  hand-written test machines.
- Passing the raw file to `invert::apply` and the mirror to `script_props`
  inside `engine::run` (a two-value engine API): the mirror's
  `invertedAxes` is computed by the same `Machine::inverted_axes` the raw
  door uses, so the split adds an API change for no different answer.
- Judging the default holder against the holder library, as the app's
  `machine::parse_mch` does: that needs camcraft-tools in the post binary
  (the crate edge PR #880 refused); the holder judgment only refuses, it
  changes no posted word, so the CLI keeps the word-only reader.

**Cost to change later.** One helper (`script_machine`) in the binary; a
follow-up lets the #926 recorded runner (`crates/post/tests/recorded/`,
PR #1002) pass the file straight through instead of mirroring itself.
