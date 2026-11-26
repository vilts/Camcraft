# Fanuc Manual Guide i blank kit: `mgi_blank_box.nc`, `mgi_blank_bar.nc`

**Machine:** a Fanuc control with Manual Guide i (for example a 31i-A5 Robodrill), in Manual Guide i graphic simulation only.
**Tests:** issue #2769. The Fanuc post option "Manual Guide i blank (G1902 / G1900)" (`manualGuideBlank`, off by default) writes the setup's stock as one blank block after `G21` and before the first `G90 G54` and tool change.
**Built by CamCraft:** each file is the Fanuc post's output for the stream that `crates/post/tests/fanuc_mgi_blank.rs` pins (`a_box_posts_g1902_before_the_first_g90_and_tool_change`, `a_z_bar_on_the_origin_posts_g1900`).

## Do not cut

Run these files in the graphic simulation only. Do not run them in AUTO / MEM. The single plunge goes 1 mm below Z0 at X0 Y0, with T03 and no coolant. On a control without Manual Guide i, `G1902` and `G1900` can cause an alarm.

## The two blanks

| File | Blank block | The stock | Work origin G54 |
|---|---|---|---|
| `mgi_blank_box.nc` | `G1902 B100. D60. H30. I50. J30. K30.` | a block 100 mm (X) x 60 mm (Y) x 30 mm (Z) | the centre of the top face |
| `mgi_blank_bar.nc` | `G1900 D60. L30. K0.` | a round bar along Z, diameter 60 mm, length 30 mm | the centre of the top face |

## Checklist

1. Each file loads with no alarm in Manual Guide i.
2. `mgi_blank_box.nc`: the simulation draws a block 100 x 60 x 30 mm. The G54 origin is at the centre of its top face.
3. `mgi_blank_bar.nc`: the simulation draws a round bar, diameter 60 mm and 30 mm long. The G54 origin is at the centre of its top face.
4. Item 3 checks the `K` word of `G1900`. The post writes `K` as the positive-Z end face minus the work origin (here 0). The bar should extend from Z-30 to Z0. An origin below the top face gives positive K; an origin above it cannot be represented and produces a refusal comment.
5. Tell us the input unit of the control (mm or inch). The manual says that Manual Guide i reads the blank in that unit, not in the unit of `G20` / `G21`.

Send a photo or the numbers that the simulation shows, on issue #2769.
