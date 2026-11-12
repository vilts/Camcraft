## #531 - EDM condition table as user data

**Decided:** `edm.conditions[]` in the `.mch`: `{ id, name, gap, wire_diameter | undersize, role: rough|skim|finish, notes }`.
- `id` is the control's own selector, required and unique. A numeric id reads as its word.
- Lengths scale with the file's unit and are judged in mm after the scaling (finite, then bounded).
- **Review #560 (bounded geometry):** the wire kerf offset reads wire radius + gap, so the table must not move a cut silently. `gap` > 0 (a stated zero gap cannot be meant). On a wire, `gap` < wire radius when both are stated. `wire_diameter` and `undersize` lie in (0, 1] mm: wire runs about 0.02-0.5 mm in the public literature, and a sinker undersize per side is tenths of a mm. A condition's wire must be one of the machine's `edm.wire_diameters` (within 0.001 mm) when the machine states any; when it states none, any wire in range is allowed. A wire condition refuses `undersize` and a sinker condition refuses `wire_diameter` (the parse takes the declared kind). An unknown condition key refuses by name. Every rule lives in one function (`condition_refusal`), which the reader and `is_sane` both call. The table moved to `crates/post/src/machine/edm_conditions.rs` (with its tests beside it), so `edm.rs` stays under 400 lines; `Edm::condition`'s signature is unchanged.
- **Id rule:** a string id must not have surrounding spaces, and is refused rather than trimmed, because the id is the word posted to the control and must read as written. A numeric id must be a whole number ≥ 0 (`1.0` refuses), so one selector has one spelling. Ids must be unique ignoring case and leading zeros: `C1`/`c1` and `01`/`1` refuse as a pair. A control that tells such ids apart cannot be posted to unambiguously from a hand-written table, and refusing the pair is the safe side. The lookup still matches exactly as written. Since duplicates are refused, no near-miss id can answer for another row.
- A garbled entry refuses the file, naming its id, and `is_sane` restates the law for project files.
- `Edm::condition(machine, id, need)` returns the condition with every needed field stated, or the refusal naming the machine, the id and the missing field. It is the door the first EDM op (#532) calls at command creation, so a missing gap or wire diameter is refused before any generator runs.
- No conditions ship (#290 D4). Tests use an explicitly synthetic table.

**Why:** absent is not zero, and generator data is vendor material. CamCraft needs only the geometry (gap, wire diameter, undersize) to place a path; power and flushing stay the control's, selected by `id`.

**Considered and dropped:**
- A separate conditions file per machine: the `.mch` is already the machine's statement, and the project file carries it whole.
- Modelling power, on-time or flushing: that is spark-process physics, out of scope for #290.
- Trimming ids silently, or case-sensitive ids: a table where `C1` and `c1` give different gaps is exactly the two-rows-one-selector case the review warned about.
- A gap of 0 or an undersize of 0 as a stated value: under "absent is not zero", a stated zero is a number the offset would use, and neither physically means anything.

**Cost to change later:** low. The section is additive.
