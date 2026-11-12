# 1197: workholding lives under Fixtures in every state

Issue #1197 (P3, effort:high). Taken unattended by the medium worker, escalated to high after its quick gate failed twice; continued on the high worker.

## Chosen

- **Setups page, no setups:** keeps "Create first setup", the one line
  "Its fixtures and tools are in the browser, under Setup.", and the export
  row. The workholding block (fixture rows, Import, vise, accessory, plate
  buttons) and the tool list are removed from this page.
- **Setups page, with setups:** unchanged (it never showed workholding).
- **Setup<n> Settings page:** an "Open <name> fixtures" button, placed on the
  Delete line at the foot of the page (no new row: the page is already at
  the structural layout limit, and a row above pushed "Pick +X edge" out of
  its pane). The Settings page still shows the workholding rpm limit row
  (#1031), which is a different control.
- **Implicit setup Settings page (no setups):** the pointer line only.
- **Browser:** unchanged. Each setup, the implicit one included, already
  has a Fixtures row (pinned by the existing fixture UX test).

## Why

The maintainer's words: "the same thing lives in two different places
depending on whether a setup exists". The issue asks for one home,
Fixtures, in all three states (no setup, one setup, several).

## Considered and dropped

- Keeping the block on the Setups page for the no-setup case (PR #1114's
  reason: "the implicit context has no Setup<n> node"). Dropped: the
  browser already has an implicit Fixtures node, so the block has a home.
- Moving the whole fixture block onto the Setup<n> Settings page. Dropped:
  #1104 asked for the Settings page to keep only setup settings; a pointer
  with one click meets "reachable in one obvious click".
- Moving the tool list to the implicit Tool list node only (no pointer
  text). Kept as a plain pointer for the same reason.

## Pinned by

`tests::setups_declutter::a_document_without_setups_keeps_workholding_under_fixtures_only`,
`tests::setups_declutter::one_setup_keeps_workholding_under_its_fixtures_page`
and `tests::setups_declutter::an_inactive_setups_fixtures_button_activates_it_and_opens_its_workholding`
(the button activates its setup first, as the browser's Fixtures row and the
Setups page's Open page button do - PR #1211 review).
The implicit Fixtures and Tool list rows under Setup with no setups are pinned by
`setup_tree::every_setup_owns_its_nodes_in_order`.
The existing `the_setups_page_lists_setups_only_and_each_setup_page_keeps_its_settings`
covers the several-setups state.

The two pointer lines share one wording (`setup_pages::WORKHOLDING_POINTER`).
