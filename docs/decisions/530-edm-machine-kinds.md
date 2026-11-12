## #530 - EDM machine kinds and the `.mch` edm section

**Decided:**
- `Kind::WireEdm` / `Kind::SinkerEdm`, declared only (`wire_edm`, `sinker_edm` and a few synonyms). No axis inventory infers an EDM.
- The required axes are X+Y for wire and Z for sinker. A blank EDM machine gets no spindle and no coolant.
- `Machine.edm` is an optional section with the wire guides, max taper, wire diameters and AWT, and the sinker orbit list (CamCraft's geometry name plus the control's word), max orbit radius and pickup. Every key is optional; absent means not stated; a garbled value refuses the file. The same law is restated in `is_sane` for project files.
- No op family admits an EDM, so every current op is refused at the door on an EDM setup.
- **Post-time family gate:** `op_streams` now re-checks every op's family against its own setup's machine and refuses by name. Before this, an op created on a mill posted in a milling dialect after its setup was remounted onto a lathe or an EDM. The door law only guarded creation.
- The empty wire/sinker templates are test fixtures (`crates/app/tests/edm/`), not library machines yet. The library laws require every shipped `.mch` to be listed (`machine_library`) and every listed machine to have a post, and no EDM post exists until #533 / #542. Each template joins the library with its kind's first post.

- **Review #551 (strict section):** an unknown `edm` key, an `edm` section on a non-EDM kind, a wire key on a sinker (and the reverse), and an orbit entry's unknown key each refuse the file by name. An EDM head stating a spindle, live tool or coolant list refuses by name like plasma (one shared walk, `no_spindle.rs`, so plasma and EDM read the same keys). Lengths are checked finite after the unit scaling. `Machine::edm_sane` restates all of it for project files. A rotary is allowed on an EDM (indexers exist on both kinds).
- The parser's cases moved to `crates/post/src/machine/edm_tests.rs`; the app suite keeps the posting gate, the validator and the round trip. The post-time family gate moved to `crates/app/src/posting_family.rs` so `posting.rs` does not grow; `default_post_for` names the EDM kinds explicitly (no wildcard).

**Why:** the #290 design (D1, D4, D5). The post-time gate is the Prime Directive: a program must never post in a dialect its steel does not speak.

**Considered and dropped:**
- Inferring an EDM from U/V letters: a mill can name a linear axis U.
- Shipping the templates in the library now: no post serves them, which breaks the library's every-machine-has-a-post law and would offer users a machine that can do nothing.
- The condition table and codes in this issue: they are #531 and #533.
- Carrying an `edm` section inertly on a mill (the first version): a file that says one thing and is read as another is the misleading case.
- Refusing a rotary on an EDM, as plasma does: wire and sinker machines both take rotary indexers.

**Cost to change later:** low. Adding keys is additive, and the gate is one loop.
