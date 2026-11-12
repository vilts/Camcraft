## #934 - Sinumerik work offsets 5 and 6 post as G505 / G506

**Decided:** the Siemens 840D post (`assets/posts/sinumerik.cps`) writes
work offsets 5 and 6 as the settable frames `G505` / `G506`, not refused.
One script function, `frameWord`, maps the engine's generic word
(`WCS_SLOTS`, `G54`..`G59`) to the control's: `G54`..`G57` unchanged,
`G58` -> `G505`, `G59` -> `G506`. Both the section's opening block and a
probing result that sets an offset go through it, so a probe setting
offset 6 writes `$P_UIFR[6,..]` and re-selects `G506`. Any word that
names no settable frame (`G54`..`G57`, `G505`..`G599`) is refused by name
(a counted warning, which blocks the post).

**Why:** Siemens' public 840D sl programming documentation lists the
settable zero offsets as `G54`..`G57` then `G505`..`G599`; `G58` / `G59`
are axial programmable offsets and select no frame, so the old output
ran in the wrong frame or alarmed. The script already held this mapping
(`frameOf`: `G505`..`G599` -> frames 5..99), only the written word was
wrong. Writing the frame keeps all six setup offsets usable on this post,
which a refusal would have taken away for no safety gain: the frame
number is fixed by the control's own documentation, not a guess. The
setup stays the one source of truth (`setup::wcs_slot`); only the word
this post writes changes, and no other post is touched.

**Dropped:** refusing offsets 5 / 6 on this post (safe, but loses two
offsets a user can pick on the Setups page with no reason); mapping in
the engine (that is #931's design - the post supplying the word from an
index; this fix does not wait for it and moves cleanly into it).

**Cost to change:** one function in one post script plus its pinning
tests (`sinumerik_work_offsets.rs`, `probe_dialects.rs`).
