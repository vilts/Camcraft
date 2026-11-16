+++
title = "Reading a refusal"
description = "Why CamCraft refused, and how to fix it."
weight = 3
aliases = ["guide/refusals/"]
+++

CamCraft refuses rather than guess. A refusal is not a crash: the document
is unchanged and nothing unsafe was written. Every refusal names what is
wrong, and usually what to do about it.

## Where refusals appear

- **A refused post** opens the **Post refused** window with the reason. It
  says that nothing was written to the file, and that a program already
  there is unchanged. The window stays until you close it, post
  successfully, or open another document. If you edit the document while
  it is open, it tells you the document has changed since, so post again
  to check it.
- **A refused change** (an edit, a new setup, a stock change) shows its
  reason on the page you edited or on the status line, and the change is
  not made.
- **A refused generation** shows its reason on the operation's page. When
  the reason names a place on the part, it is underlined: click it to
  frame that spot in the viewport.
- **A greyed-out button** says in its tooltip why it is unavailable.

## Common post refusals

| The message says | What it means | What to do |
|---|---|---|
| `<operation>: no toolpath yet - generate it first` | The operation was never generated. | Select it and click **Generate**. |
| `<operation>: inputs moved since generation - regenerate before posting` | The model, stock, tool, work offset or the operation's settings changed after it was generated. | Generate it again. |
| `post blocked - <operation>: <n> warning(s): ...` | The operation's own checks found something unsafe. | Read the warnings. The flagged moves are marked in the viewport; fix the setting or geometry they name. |
| `gouge: program cuts below the model ...` | Simulating the whole program found material removed from the part. | Use **Simulate** and **Deviate** to find where, then fix the operation that cuts there. |
| `... - fix the tool before posting` | An operation's tool is missing or cannot cut that kind of operation. | Pick a suitable tool on the operation's Geometry tab. |

A machine-limit refusal names the axis or setting and the limit the
mounted machine states. Either the program really is outside the machine's
range (move the work offset or the part, or choose another machine), or
the machine file's number is wrong (check it against the machine's
manual).

For a message that is not in this table, look it up by what you see on the
[Troubleshooting](@/guide/reference/troubleshooting.md) page.

## Geometry and generation

The table quotes the identifying part of each message. The full message
also gives the operation, dimensions or picked geometry.

| The message says | What it means | What to do |
|---|---|---|
| `the open area is too narrow for the ... tool and ... stock to leave at every level` | 3D Pocket has no cutter-centre space after tool clearance and allowance. | Use a smaller tool, review the allowance, or pick a larger machinable area. |
| A stock band is narrower than the tool | A stock-source Pocket, Adaptive or Offset cannot enter that strip between the part and stock edge. | Use a smaller tool or change the stock or operation source. Inspect each named band. |
| `stock-source chamfer breaks into a part; chamfer with the part as source` | The chamfer cut on stock crosses the kept part. | Set the part as the chamfer source and generate again. |
| `the entry ramp angle must be 0 (one chord) or between 0 and 90 degrees` | The entry angle is outside its allowed range. | Use a finite entry angle below 90 degrees. Zero requests one chord. |
| `the ramp angle must be between 0 and 90 degrees` | A link ramp needs a positive angle strictly below 90 degrees. | Correct the link angle. The entry and link settings are separate. |
| Boundary chains touch or cross | The selected edges do not define an unambiguous area between them. | Select two separate boundaries that do not touch or cross. |
| An open Contour has no clear air side | The part does not establish which side of the open run is outside material. | Correct the picked edge or source. Do not assume that reversing the run makes it safe. |
| `mesh may have nested or overlapping shells` | An inward mesh shell can describe a cavity, so automatic orientation is ambiguous. | Re-export separate non-overlapping solids. Isolated closed inward STL and OBJ solids are corrected at import, with a note. |

For a reduced-fidelity model, Cross Drill checks include the model's
excess in entry, reach, holder and part clearances. A refusal means those
checks cannot establish clearance with that uncertainty. Use a more
accurate model or change the setup; do not reduce the safety clearance to
hide the finding.

## EDM and controller data

| The message says | What it means | What to do |
|---|---|---|
| `a profile stitched from ... open runs ... is not supported` | A 4-axis wire profile has unsupported stitched geometry. | Pick one face or closed edge chain on each plane. Use explicit sync pairs for supported fan spans. |
| A 4-axis wire cut asks for tabs or no-core burnout | Those features are supported on one-profile cuts only. | Remove them or use a one-profile contour. |
| A taper machine does not state its angle or corner rule | The control's lean cannot be determined safely. | Complete the machine's `edm.taper_angle` data from its control documentation. |
| Sodick refuses taper or 4-axis work | The shipped Sodick post supports 2-axis contours only. | Use a supported vertical contour or a post verified for your control and cut type. |
| Sinker overburn or an incomplete orbit vector | Gap and orbit radius cannot produce the requested size, or the machine lacks the needed orbit data. | Review the electrode size, condition, orbit and machine data. See [Wire and sinker EDM](@/guide/operations/operations-edm.md). |
| `first X/Y traverse with no clearance height stated` | grblHAL, Carvera or TinyG cannot position safely without a clearance or retract plane. | Set the operation heights and generate again. For a standalone CL stream, supply the plane. |
| Tormach lathe has no stock or clearance plane | The post cannot establish the safe one-axis approach to or departure from G30. | Supply the stock and required heights. Check the stored G30 position at the control. |

Tool import, numbering and body placement refusals are explained in
[Tools and tool libraries](@/guide/set-up/tools.md).

## When you think a refusal is wrong

Report it on the
[issue tracker](https://github.com/NeoTech/Camcraft/issues) with the
message and, if you can, the project file. A refusal that blocks a safe
program is a bug. A program that would gouge or collide and was **not**
refused is a worse one: please report those first.
