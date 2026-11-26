+++
title = "Posting"
description = "Writing the NC program, and what is checked first."
weight = 2
aliases = ["guide/posting/"]
+++

Posting turns the document's operations into an NC program for one
machine controller. Click {{ <icon name="act-post" /> }} **Post** (Ctrl+P, Cmd+P on macOS), choose where to save the
program, and CamCraft checks the program, writes it, and opens it in the
G-code viewer.

## Post processors

The post is chosen on the **Post** page: a setup can pick its own, and a
setup that picks none uses the document's. Select a post for your machine
kind and control. The 0.26.0 release includes these scripts in
`assets/posts/`; machine filenames below are in `assets/machines/`.
A machine template is a starting point, not proof of the installed control
or machine configuration. Check its travel, tools and clearance before use.

| Post choice | Script | Control | Supplied machine files |
|---|---|---|---|
| Fanuc | `fanuc.cps` | Fanuc milling | No control-specific file; generic `vmc.mch` and `3axis-vmc.mch` templates |
| Haas | `haas.cps` | Haas milling | `haas-vf2.mch`, `haas-vf2ss.mch`, `haas-mini-mill.mch`, `haas-dt1.mch`, `haas-dm1.mch` |
| Siemens 840D | `sinumerik.cps` | Siemens Sinumerik 840D | None dedicated |
| Siemens 810M | `sinumerik-810m.cps` | Siemens Sinumerik 810M / WinNC 810M | `emco-vmc-200.mch` (EMCO VMC-200) |
| Heidenhain | `heidenhain.cps` | Heidenhain TNC DIN/ISO | None dedicated |
| Mazak | `mazak.cps` | Mazak ISO milling | None dedicated |
| Tormach PathPilot | `tormach.cps` | PathPilot milling | `tormach-770mx.mch`, `tormach-770mx-ml.mch`, `tormach-770mx-microarc4.mch`, `tormach-770m.mch`, `tormach-770m-plus.mch`, `tormach-1100mx.mch`, `tormach-1100m.mch`, `tormach-1100m-plus.mch`, `tormach-1500mx.mch`, `tormach-pcnc440.mch`, `tormach-24r.mch`, `tormach-xstech.mch`, `tormach-xstech-pro.mch` |
| grblHAL | `grblhal.cps` | grblHAL | `grblhal-3axis.mch`, `sienci-altmill-2x4.mch`, `sienci-altmill-4x4.mch`, `sienci-altmill-4x8.mch` |
| TinyG | `tinyg.cps` | Synthetos TinyG | `tinyg-3axis.mch` |
| LinuxCNC | `linuxcnc.cps` | LinuxCNC RS274NGC | `linuxcnc-3axis.mch` |
| Grbl 1.1 | `grbl.cps` | Grbl 1.1 | `grbl-3axis.mch` |
| Doosan DNM | `doosan-dnm.cps` | Fanuc 0i/31i-M on Doosan DNM | `doosan-dnm-3axis.mch`, `doosan-dnm.mch` |
| Makera Carvera | `carvera.cps` | Makera Carvera | `makera-carvera.mch`, `makera-carvera-air.mch`, `makera-z1.mch` |
| Mach4 | `mach4.cps` | Mach4 Mill | None dedicated |
| RichAuto DSP A11 | `richauto.cps` | RichAuto DSP A11 router | None dedicated |
| Mach3 | `mach3.cps` | Mach3 Mill | Mach3-era templates: `tormach-pcnc770.mch`, `tormach-pcnc770-s3.mch`, `tormach-pcnc1100-s1.mch`, `tormach-pcnc1100-s2.mch`, `tormach-pcnc1100-s3.mch`; select the post manually and check the installed control |
| Heidenhain Klartext | `heidenhain-klartext.cps` | iTNC 530, NC software 340490-03 | `fehlmann-picomax55.mch` |
| Fanuc Lathe | `fanuc-lathe.cps` | Fanuc turning | No control-specific file; generic `lathe.mch` template |
| Tormach PathPilot Lathe | `tormach-lathe.cps` | PathPilot turning | `tormach-8l.mch`, `tormach-15l-slant-pro.mch`, `tormach-15lx-slant-pro.mch` |
| Tormach PathPilot Plasma | `tormach-plasma.cps` | PathPilot plasma | `tormach-1300pl.mch` |
| Generic ISO Wire EDM | `wire-iso.cps` | Generic RS-274 motion; EDM words from machine codes | No control-specific pairing; fill the wire machine's EDM code templates |
| Sodick Wire EDM (Mark 21) | `sodick-wire.cps` | Sodick Mark 21 / EX21, vertical 2-axis work | `sodick-alc400g.mch`; select the post manually and replace example technology values |
| Native wire EDM job files | `agie-native.cps` | Agie Classic Cut 2S native job | `agiecut-classic-2s.mch` template; select the post manually |
| G-EDM Wire EDM | `g-edm.cps` | G-EDM G-RBLv3, 2-axis wire | None dedicated |
| Sinker EDM (ISO) | `sinker-iso.cps` | Generic ISO-style sinker; EDM words from machine codes | `sinker-edm.mch` template; fill its EDM code templates |
| Load by file | `waterjet-iso.cps` | Generic ISO abrasive waterjet | None dedicated; this supplied script is not a built-in picker entry in 0.26.0 |

The generic `tilthead.mch` and `millturn.mch` templates do not name a
control-specific post. The `ooznest-workbee-1000x1500.mch` file describes
an Estlcam machine, but 0.26.0 supplies no Estlcam post. A machine file
alone does not establish that a compatible post exists.

A post is a `.cps` script, so you can load your own with
**Load post (.cps)**.

**Native wire EDM job:** choose **One program** and write to a new job
directory. The job includes nominal ISO geometry, a resource manifest and
a setup script with zero erosion steps. See
[Native wire EDM job files](@/guide/operations/operations-edm.md#native-wire-edm-job-files)
for supported cuts, separation settings and the checks to make on the
control.

To write or debug a script, see the
[Post-processor API](@/guide/reference/post-processors.md).

The Post dialog opens where this document last posted, or beside the saved
project with the project's name and the `.nc` extension.

The scope label at the top right of the Post dialog names the selected
post and, when applicable, its setup. **Hover** over that label to see
the full built-in post script path. A custom post already shows its file
path in the label. Check this path when two scripts have similar names.

### Controller details

- **Tormach milling:** consecutive sections that use the same loaded tool
  skip the repeated G30/M6. Tool compensation and safe clearance still
  apply. A separate program starts without a known loaded tool.
- **Tormach lathe:** after G30, the first point is approached one axis at
  a time. Before G30, the tool leaves the stock in X, then Z. Missing
  stock or clearance information is refused. Set and check the control's
  stored G30 position before running the program.
- **grblHAL, Carvera and TinyG:** the first XY traverse needs a stated
  clearance or retract plane. A missing plane is refused; the refused
  move is not written, including through the standalone post tool.
- **Wire EDM:** a user post must declare `wireEdm` to serve wire work.
  The generic post uses the machine's EDM code templates. The Sodick
  post supports 2-axis work only and refuses taper, 4-axis work and
  control-side G41/G42 H offsets. Set tank and flushing functions at the
  control; this post does not program them.
- **Fanuc-family numbers:** Fanuc, Haas, Mazak, Doosan DNM and Fanuc
  Lathe write a decimal point on whole-number lengths, angles and
  feed-per-revolution words, for example `Z14.`. This prevents a control
  with DPI=0 from reading that number as least increments.
- **Indexed work offsets:** grblHAL and TinyG accept G54 through G59
  for indexed work. An offset beyond that range is refused.

Post options can belong to the document or to one setup. A setup can
follow the document or keep its own options. Agents can inspect and edit
these choices with `get_post_options` and `set_post_options`; see
[AI agents (MCP)](@/guide/automate/mcp.md).

### Fanuc AI contour control

With **Fanuc** selected, open the Post options' **Smoothing** group.
**AI contour control (G05.1)** is off by default. Enable it only if the
control has that option. **AI contour control level** supplies the R
value, from 1 to 10 (default 5); it is the controller's machining
condition level, not a tolerance in mm.

For example, enabling it at level 5 writes `G05.1 Q1 R5` after the
operation preamble. `G05.1 Q0` cancels it at the operation's end and
before a canned cycle. Probe sections do not enable it. A level outside
1–10 is refused by name; the standalone post also reports "G05.1 Q1
takes R1 to R10; not written" and emits no enable word.

This option owns the AI contour words when enabled. It does not also
write the operation's tolerance-based Smoothing request. With the option
off, that request alone gives "SMOOTHING NOT WRITTEN - G5.1 STATES NO
TOLERANCE". Check the control's level settings and prove out the program.

### Fanuc Manual Guide i blank

In the Fanuc Post options' **Header & comments** group, **Manual Guide i
blank (G1902 / G1900)** is off by default. Enable it only on a control
with Manual Guide i. It writes the setup's supported stock blank once,
after the unit selection and before the first operation.

A machine-aligned box uses `G1902`: B, D and H are its X, Y and Z sizes;
I, J and K locate the work origin from its minimum corner. For example,
a 100 × 60 × 20 mm box with the work origin at the centre of its top
writes `G1902 B100. D60. H20. I50. J30. K20.`. A Z-axis cylinder whose
axis passes through the work origin uses `G1900`: D is diameter, L is
length, and K is the positive-Z end face's distance from the work origin.

Unsupported stock produces a `NO BLANK BLOCK` comment with its reason,
and posting continues without a blank block. Examples include stock from
a model or previous setup, rotated stock or WCS, X/Y-axis cylinders and
a Z cylinder whose axis is off the work origin. Correct the stock or
define the blank at the control; CamCraft does not substitute a box.

### Explain G-code

Tick **Explain G-code (add a comment to each block)** on the document or
setup post page to append plain-English
comments to supported blocks. Tormach PathPilot milling, Fanuc, Haas,
Mazak, Doosan DNM and Fanuc Lathe support this option. The comments do
not change executable words or motion. Fanuc-family posts omit an
explanation when it would exceed the block length budget. Explain is off
by default; a post without explanation support refuses the option. The
program gets longer. The change is one undo step.

### Stops between setups

For a setup-change stop, Haas, Mazak, Doosan DNM, Sinumerik, Heidenhain,
grblHAL and TinyG lift Z and name the next setup before the M0 or M1.
Ordinary operation stops keep their existing behavior. Heidenhain uses
the section's work clearance and refuses missing or unsafe clearance;
it preserves a tip already above that height. Sinumerik refuses a missing
machine retract. This does not establish a safe home position for every
machine: inspect the retract and the setup before running it.

### Heidenhain Klartext on the Fehlmann Picomax 55

Select **Fehlmann Picomax 55 (iTNC 530)** as the setup's machine and
**Heidenhain Klartext** as its post. The post writes a conversational
`.H` program for NC software `340490-03`. The **Heidenhain** entry
writes DIN/ISO `.I` programs.

Use one metric setup, a defined stock blank, and one preset row (1–6).
Place the clearance plane above the stock. Each operation must start and
finish at or above that plane. The post supports three-axis straight moves, plunges, and constant-height
XY arcs. It applies the work origin to the blank, points, and arc centers.
It refuses drilling and tapping cycles, probing, tilted tool axes, helices,
and control-side cutter compensation. Select Off or the machine's M8
flood circuit for coolant.

Use a flat, ball-nose, bull-nose, or chamfer mill. Each tool number
must identify the same cutter and geometry throughout the program.

The machine definition states a spindle range of 50–9000 rpm. The post
checks the requested speed against the stated range. The bundled machine
definition gives rapid rates only. The post checks the numeric range of
each feed word. Verify axis-specific cutting feeds and machine-coordinate
travel for the physical setup.

Start with the spindle stopped and the quill at its upper tool-change
position. At the initial stop, verify the quill is coupled for three-axis
operation, W is fixed at the position used to set the preset, and the
active tool axis is +Z. Check that the preset row has X, Y, and Z values,
its basic rotation is zero, and no coordinate transformations are active.
Check the tool table and confirm clear space for the full retract and transfer. The program
uses `M140 MB MAX`, which moves along the active tool axis toward the
active travel limit. At each installation stop, confirm the quill is at
the physical tool-change position and the spindle drive is off before
handling the tool. After the retract with the called tool, confirm its
actual height clears the entire XY transfer before resuming. HEIDENHAIN documents that
preset cycle 247 has no effect in Test Run. Check the selected physical
preset independently. This profile has software tests. Acceptance on the
target control and machine is pending. [iTNC 530 manual, §§7.4 and 8.9](https://content.heidenhain.de/doku/tnc_guide/pdf_files/iTNC530/34049x-neu/en/bhb.pdf)

## What is checked before anything is written

The post is a gate. It writes the program only when all of these hold:

- every operation has a tool that exists and can cut that kind of
  operation;
- every operation has been generated, and nothing it depends on has
  changed since (no **stale** operations);
- no operation carries a warning that blocks posting;
- the program as a whole verifies: the simulated stock shows no gouge into
  the part, no move strikes a fixture, and no move exceeds a limit the
  mounted machine states, such as axis travel or spindle speed.

An Adaptive fallback to **Offset rings** raises a visible advisory
warning with the number of affected areas. For example, one area the
planner could not cut adaptively appears as "Adaptive fell back to
Offset rings on 1 area". Read the toolpath notes for each area and its
reason. This warning does not hold the post because the rings are
verified; collision and gouge warnings still block it.

For part-source Engrave and Trace, simulation accepts the intended mark
only on the selected part or parts. Neighbouring parts remain subject to
the gouge check. Selecting one part does not permit a mark on another.

If any check fails, **nothing is written**; a program already at that path
is left unchanged. See [Reading a refusal](@/guide/reference/refusals.md).

## More than one machine

When setups use different machines or posts, Post writes one NC file per
machine and post, and checks each one against its own machine.

## Before you run it

Passing the gate means CamCraft found nothing wrong with the program
against the model, stock, fixtures and machine facts it knows. It does not
know everything about your machine and setup. Read the program, run it in
your control's own simulation if it has one, and dry-run it with the
spindle off and Z raised before cutting material.

## Headless posting

The command-line tool `camcraft-cli` can drive CamCraft without the window,
including as an MCP server for AI agents (see [AI agents (MCP)](@/guide/automate/mcp.md)). The standalone `post` tool runs a
`.cps` script over a cutter-location file; it does not run the document
checks above, so its output is not verified.

Which unit the program is written in, and where its coordinates are
measured from, is on [Units and coordinates](@/guide/reference/units-and-coordinates.md).
