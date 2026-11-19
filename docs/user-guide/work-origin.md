# Where the work origin is, and how to change it

Every setup has one work origin (WCS). It is the point the program calls
X0 Y0 Z0, and the point you touch off on the machine. If the machine's
zero and CamCraft's origin are not the same point, the program cuts in the
wrong place, for example in the air above the part. Check it before the
first cut.

## Where CamCraft says the origin is

- **The WCS page** (click the setup's **WCS** row in the browser). The line
  under the heading says where the origin is in words, for example
  `G54 origin: top of stock, front-left corner (X0 Y0 Z0)`.
- **The setup sheet** has the same words on its `origin:` line, under the
  WCS numbers.
- **The viewport** draws the origin as the WCS axis triad.

The words read the origin against the stock box, or against the model's
bounds when there is no stock. Left and right run along X, front and back
along Y (you stand at the front, at -Y), and bottom and top along Z. An
origin that is not on a corner, an edge centre or the centre of the box is
given as distances from the box's left side, front and top instead.

A new setup's origin is at the world origin until you place it.

## How to change it

On the WCS page, either:

- click one of the nine **Anchor on** points (after choosing its Z level),
  which places the origin at once; or
- type the **X / Y / Z origin** and the angles, then click **Set origin**
  at the foot of the page, beside **Close**.

While typed values are not applied yet, the footer shows
**Origin not applied**. Closing the page does not apply them. The quarter
turns and **Level** apply at once.

The work-offset dropdown (G54-G59) under the page only chooses the word
the post writes. It does not move the origin.
