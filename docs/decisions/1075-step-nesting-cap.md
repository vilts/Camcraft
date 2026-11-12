## #1075 - STEP records may nest at most 64 parentheses deep

**Decided:** before ruststep parses a STEP file, one linear byte scan
(`geometry::import::step_depth`) tracks parenthesis depth outside the
spans ruststep reads as text (strings, comments, `<...>` resources and
anchor names - table below) and refuses the file past
`MAX_NESTING = 64`, naming the cap, the record's `#id` and the line
(`\n`-counted, so CRLF and LF agree). Both functions that hand bytes to
ruststep call it first: `step::parse_with_chord` (every model import:
`ImportModel`, `ImportFixture` / fixture models, the tool-body mesh, the
MCP `import_model` and script import all reach it through
`import::load_model[_graded]`) and `step_csys::read_tool_csys` (the tool
body's coordinate systems).

**Why 64 - the evidence (2026-11-12, Windows 11, this machine):**

| ruststep (pinned rev 90fdc935), one record nested N deep | 1 MiB stack | 2 MiB stack | 8 MiB stack |
|---|---|---|---|
| debug build, last N that survives | 400 | 821 | 3341 |
| release build, last N that survives | 530 | 1086 | 4418 |

About 2.5 KB of stack per level in debug, 1.9 KB in release. The smallest
stack an import runs on by default is 1 MiB: the Windows main thread, where
the GUI's import falls back to running inline when its worker thread cannot
be spawned (the worker and test threads get Rust's 2 MiB). Every committed
STEP fixture (`testdata/models/*.step`, a real AP214 export included) nests
3 deep; real AP203/AP214 records rarely pass 4-5. 64 is about 20x the
deepest real record and about 6x below the debug crash on 1 MiB, leaving
room for whatever the caller already has on the stack. It also matches the
cap the solidcore STEP importer (#916, PR #941) already uses.

**Considered and dropped:**
- (b) parse on a thread with a large, bounded stack and treat its death as
  the refusal: a stack overflow aborts the whole process on every platform,
  it cannot be caught on a thread; a bigger stack only moves the depth.
- Hosting the scan in PR #1006's `step_diag.rs`: it is not merged; a new
  file keeps the two PRs from colliding (both touch `parse_with_chord`, in
  different hunks).
- A cap near the crash depth (e.g. 256): no real file needs it and it eats
  the headroom on a 1 MiB stack.

**Cost to change later:** one constant; the measured table above says how
far it can move. A ruststep that parses iteratively would make the scan
redundant but harmless.

**The scan mirrors ruststep's tokenizer (review of PR #1079).** The first
version knew strings and comments only; ruststep also reads `<` ... `>`
(REFERENCE resources, ANCHOR names) as text, so `<x/*y>` or `<x'y>` put
the scan into a comment or string state that ruststep never entered, and
a 5000-deep record after it passed the check and overflowed the stack.
The whole tokenizer at the pinned rev 90fdc93 (`parser/token.rs`,
`parser/combinator.rs`, `parser/exchange/*.rs`) was then read end to end.
ruststep recurses only on `(` (`list`, `typed_parameter`,
`subsuper_record`, `anchor_item_list`), parses front to back, and stops at
its first syntax error. Where its rule is unclear the scan COUNTS the
parenthesis: a wrong refusal of a weird file is acceptable, a missed deep
record is not.

| construct | ruststep | the scan |
|---|---|---|
| string `'...'` | `'`, then `''` or any char but `'`, then `'`; `(` `)` `/*` `<` `"` are text; no backslash escape (`\'` ends it) | `Str`: same rule, `''` taken greedily as ruststep does |
| comment `/* ... */` | only between tokens; no nesting; first `*/` closes; `/*/` does not | `Comment`: same; `/*` in code opens one |
| resource `<...>` (REFERENCE), anchor name `<...>` (ANCHOR) | `<`, any chars but `>`, `>`; `'` `/*` `(` `"` are text | `Uri`: `<` opens, first `>` closes |
| binary `"..."` | not implemented (`FIXME binary`): a parse error at `"` | `Binary`: `'` `/*` `<` are text, `(` still counted, `)` not subtracted |
| enumeration `.X.`, keywords, `!USER` keywords, `#n` / `@n` names, numbers, `$`, `*` | upper, digit, sign, `.` only - no state | no state; `#n` at depth 0 names the record |
| anchor tag `{name:item}` | braces are tokens; the item may be a `(` list | no state; `(` counted |
| HEADER records, `DATA(...)` parameters, ANCHOR item lists | the same `(` recursion as DATA records | counted like DATA records (reported without an entity id) |
| SIGNATURE (after `END-ISO-10303-21;`) | BASE64 `[0-9A-Za-z+/=]`, no `(`; parsed after every DATA section | scanned as code; whatever it opens only skips text ruststep never recurses into |
| unterminated string / comment / `<` / `"` | parse error at the opener | skipped to the end of the file (nothing after it is parsed) |
| unbalanced `)` | parse error | depth saturates at 0 |

`crates/app/tests/step_nesting.rs` pins every row: each construct, with a
stray `'`, `/*`, `<` or `"` inside, followed by a record one past the cap,
is refused by the scan; text parentheses in a string, a comment and a
resource do not count and the file still imports.

**Re-audit on every pin move.** The scan mirrors a third-party tokenizer,
so it must be re-audited whenever the ruststep pin in the root
`Cargo.toml` moves (the comment beside `[patch.crates-io]` says so). The
importer's move to solidcore (#917), which caps nesting itself, removes
this dependency and the scan with it.
