+++
title = "Checks on multi-axis work"
description = "Reach and travel, the tilt tolerance, tool and holder collision, index swing, and what is refused or only warned."
weight = 5
+++

Anything a check flags as unsafe blocks the post. The checks below run for
multi-axis work. Each says what it covers and what it does not.

## Machine check: reach and travel per axis

Before a program posts, the machine check judges the cached moves of every
operation against the mounted machine's file, one sentence per problem. For
tilted and rotary moves it says:

- the move tilts the tool but the machine states no tilting rotary;
- the move commands a rotary letter the machine states no axis for;
- the commanded angle is outside that rotary's travel (it prints the range);
- a lone rotary about X cannot reach the pose, judged through the tool axis;
- a spindle C turn on a machine with no C axis, or outside its travel.

A machine that states no travel for an axis is not checked for it ("the
unstated-limit law"). Check your machine file's numbers against the machine's
manual. If the problem count passes the report cap, the rest is counted.

## Tilt tolerance

The reach laws treat a tool as upright when its tilt off +Z is within 1e-9
radians (`UPRIGHT_TILT_RAD`), so a move tilted only by float noise (about
1e-10 degrees, as a rotated setup can produce) is judged as upright and does
not raise a false holder collision (issue #938). A real tilt larger than that
takes the tilted-tool path.

## Shank, holder and fixture collision

Along tilted moves the verifier reads the tool's shank and holder against the
uncut stock and the part, and the fixture bands against the tool, at the
pose of each move. A hit is reported as a shank hit (once per move). Shank
avoid on the 5-axis operations moves the pose off such a hit within the Max
tilt cone before it is reported.

## Index swing

Before a work-turning rotary (a table A or B) indexes, the post retracts the
tool to the machine's safe Z. The post gate then checks that the tool tip at
that height clears the radius the stock, parts and workholding that turn
with the rotary sweep about the rotary's line, by a margin of 5 mm. Too
little clearance refuses the post by name.

- It needs the rotary's `nose_to_axis` (the distance from the spindle
  nose to the centreline at safe Z) in the machine file. An unstated or
  invalid value refuses, because the gate cannot show the turn clears.
- It judges a level rotary line (A about X, B about Y). A vertical one
  (C) keeps every height. A leaning line refuses by name.
- It does not apply to a rotary that carries the head (a tilt-head).

**Known gap (#1107):** the check judges the tool against the swept work. It
does not judge the swept work against what stands still beneath it: the
machine table, a fixture plate or a rotary subplate. A long or wide
workpiece on a low rotary can swing into the table or plate and the program
still posts. Check that clearance yourself.

## Refused or only warned

| Condition | Result |
|---|---|
| Tilt on a machine with no tilting rotary | Refused (machine check; also each non-TCP post) |
| Rotary angle outside the machine's travel | Refused |
| Post with no RTCP output given a tilted move | Refused by the post, counted warning, blocks the post |
| Index swing clearance too small, or `nose_to_axis` unstated | Refused by name |
| Shank or holder in the part or stock | Warning that blocks the post |
| Stock simulation of a posed program | "Not judged", by name: the column engine cannot judge a tilted tool, so no pass is claimed |
| Spindle body against the work, swept work against the table | Not checked |
