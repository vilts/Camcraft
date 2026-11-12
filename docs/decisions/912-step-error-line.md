## #912 - STEP import names the entity that failed to parse

**Decided:** on a `ruststep` parse failure, `step_diag::locate` re-scans the
text once (linear: `;` framing outside `'..'` strings and `/* */` comments,
line counted, DATA..ENDSEC tracked) and parses each DATA record alone with
ruststep's `entity_instance`; the first record that fails becomes the error
("entity #9 at line 16 cannot be parsed: ..."). Unterminated strings and
comments name the line they opened on; a file without DATA says so. If every
record parses alone, the failure is elsewhere and ruststep's own message is
kept.

**Why:** ruststep fails the whole DATA section with a position that is not
the bad entity (#887). The user needs something findable in the file.

**Dropped:** patching/forking ruststep (a dependency, needs the maintainer);
re-parsing the file by bisection (quadratic on large files).

**Cost to change:** one module, `step_diag.rs`; message text is asserted in
`crates/app/tests/import.rs` (`step_parse_error_*`).
