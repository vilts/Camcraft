# Post an Agie Classic Cut 2S job

CamCraft writes the nominal contour, resource manifest and piece script directly. The job opens with zero erosion steps. It requests cold die steel (material 0001), Berco Cut 0.25 mm wire and a target finish of Ra 0.30 µm. Check the effective work technology on the installed control before assigning any cutting steps. The control calculates the offsets and trims. The CamCraft wire-center preview uses a 0.005 mm representative finishing gap.

## Post from the Wire EDM workspace

1. Mount **AGIECUT Classic 2S (template)** as the setup machine. In the machine editor, open **EDM** and add a condition with ID **AGIE_QN2**, role **rough**, wire diameter **0.25 mm** and preview gap **0.005 mm**. Apply this change to the mounted machine. Choose **Native wire EDM job files** in the post picker.
2. Define axis-aligned rectangular stock at most 350 × 250 mm in plan. Choose a stock thickness of 15, 20, 40 or 80 mm. The stock and work origin together must fit that span. Verify the installed work datum and the available axis travel on Agie Control. CamCraft refuses profiles with a stated X or Y travel limit because its preview cannot check the full resident path against that limit. Put the first, unrotated work origin on the stock top. Its XY position determines the job coordinates.
3. Use one modeled workpiece and create one **Wire Contour**. For two to nine punches on the same stock, follow **Group captured punches** below. For a die opening, follow **Select a die opening** below. For two openings in one plate, follow **Group two die openings in one plate** below. For a punch, select the complete punch model. Use a punch with straight sides, vertical walls and flat ends at the stock planes. Angled faces are supported. Leave at least 0.4 mm of stock around the nominal polygon. Acute corners can need more stock: the post also checks the intersection of the two offset face lines and the swept wire at that corner. CamCraft checks each corner with the measured rough offset for the selected stock thickness. The stock margin includes the 0.125 mm wire radius. Some reentrant contours are supported. A tight inside corner fails if the rough offset for the selected stock thickness leaves more than 0.002 mm at the nominal vertex. Short segments and closely spaced faces fail if adjacent rough paths could meet. The punch must have a straight outer face that allows the lead to stay entirely on the waste side. Additional bodies, selected subprofiles, holes and steps are refused. Select a 0.25 mm wire and condition **AGIE_QN2**. Set the work planes to the stock top and bottom, and specify how the punch is captured. The capture setting confirms support for the punch when it is released. Keep the wire vertical. Taper, four-axis, burnout, explicit skims, tabs and fixture models are unsupported.
4. Place the centre of a predrilled 2 mm threading hole in the waste. The entire hole must be inside the stock. Allow a perpendicular approach to the interior of a straight face with at least 3 mm of lead and at least 3 mm of straight face on each side of the entry. Generate the operation.
5. Open **Post**, choose **One program** and keep the general post settings at their defaults. Under **Wire EDM technology**, set **Separation cut length (mm)** to **1.0**. For a closed contour, zero leaves the separation unspecified and blocks the post. For an open cutoff, see **Post a straight open cutoff**. This length applies to the separation segment that Agie Control generates. Check its position, trims and release sequence in the control’s job preview. Enter the parent folder and a fresh **Job folder name** such as `PUNCH40`. Use up to eight uppercase letters or digits, beginning with a letter. The post publishes the entire directory after validation. To retain an earlier job, give the new one a different name.

### Select a die opening

Use a 20 mm thick retained plate with one through-opening, planar caps and vertical walls. Other plate thicknesses are refused: the control checked the die cycle at 20 mm only. In the Wire Contour **Geometry** tab, choose **Edges** and Shift-click the connected edges of the upper rim of the opening. Set the wire side to **Die** and set **Freed piece captured** when the setup holds the released slug. CamCraft compares the selected rim with the full-height opening of the plate. A blind pocket, an outer edge or an incomplete rim is refused.

The opening must be convex. The control offsets a die toward the inside of the opening, so each corner of the opening is an inside corner of the plate. Each corner fails if the rough offset of the 20 mm row (0.245 mm) leaves more than 0.002 mm at the nominal vertex. A square corner fails. Fine facets, as on the faceted ends of the example, pass. Leave at least 0.4 mm of plate and stock between each wall and the stock edge.

Place the complete 2 mm threading hole inside the opening. CamCraft enters the nearest straight wall that has at least 3 mm of lead from the hole and at least 3 mm of straight wall on each side of the perpendicular entry. The entry point stays at least 0.4 mm from each other wall. The ISO object lists the nominal opening counter-clockwise from the entry, and the setup script states the die side (`c_die`).

### Group two die openings in one plate

Use one retained plate with exactly two through-openings, one setup, 20 mm stock and the same **AGIE_QN2** wire condition for both operations. The plate must be the only model, with no placed drawing. Create one **Wire Contour** for each opening. Select each opening's upper rim, enter its own threading hole inside that opening and confirm capture for its slug. Each opening must meet the rules of **Select a die opening** above. Leave a clear band of plate along X or Y between the two cutting areas. The cutting area of each work includes the opening, its threading hole, the 3 mm lead, the furthest rough corner and the 0.125 mm wire radius. Generate both operations and post to a new job folder with a name of up to seven characters. CamCraft refuses a punch and a die in one group, the same opening selected twice, a plate with another number of openings and AGIE_BERCO_RA035.

Run the original two-opening example from the project root.

```sh
cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/die-pair.camscript
```

The `DUODI20` folder contains two ISO files, one SBR and one SBL. The left work `CUT1` threads at X−35.5 Y0. The right work `CUT2` threads at X24.5 Y0. Both works state the die side (`c_die`) and refer to one CCB030 quality. Assign the TECCUT row to each work.

In the offline control check, each work showed **Matrijs** and `CCB030[4]` after TECCUT, with offsets of 0.245, 0.155, 0.140 and 0.134 mm inside its opening. The formal check reported no errors. The plot cut the main pass of both openings first, then the two 1 mm separations, then the three trims of `CUT1` and the three trims of `CUT2`. No cutting move crossed the plate between the openings. Support both slugs before the control releases each separation section. Check each inward offset and the release of both slugs in the control preview before running the job.

### QN2 offsets by stock thickness

Agie Control calculated these wire-centre offsets on the straight outside faces of a synthetic rectangular punch. All dimensions are in millimetres. The native job sends nominal geometry. The resident technology applies each offset. The post review names the offsets of the selected thickness. Check them in the control before GRAFICHECK and FORMALCHECK.

| Stock thickness | Rough | Trim 1 | Trim 2 | Trim 3 |
| ---: | ---: | ---: | ---: | ---: |
| 15 | 0.245 | 0.155 | 0.140 | 0.134 |
| 20 | 0.245 | 0.155 | 0.140 | 0.134 |
| 40 | 0.260 | 0.170 | 0.140 | 0.130 |
| 80 | 0.266 | 0.176 | 0.143 | 0.133 |

The clearance check rounds the rough offset plus the wire radius up to the next tenth of a millimetre. For these four thicknesses this gives 0.4 mm on a straight face. At a pointed corner the post checks the miter for that thickness separately. Other thicknesses are refused. They need their own control measurements.

### Request Ra 0.35 µm for one 20 mm punch

For one closed polygon punch in 20 mm cold die steel, you can request quality **CCB035** instead of CCB030. Add a mounted machine condition with ID **AGIE_BERCO_RA035**, role **rough**, wire diameter **0.25 mm** and preview gap **0.005 mm**. Select **AGIE_BERCO_RA035** in the Wire Contour condition field. The project file keeps the selection. The job requests **Ra 0.35**, **Te 1.50** and **Tkm 1.00**, with the speed option (**Snelheid** on the control) set and the other two options disabled. The job arms no technology: after import the work shows **CCB035[0]**. All other files of the job are the same as for CCB030.

On the offline control, the TECCUT header for this request shows **TYPE:VS**, and TECCUT preselects the Ra 0.35 technology **`estbca25nsn060b035020`**. Identify the technology by this name. The number in the first TECCUT column is a match distance that changes with the request; it is not a row number. With this technology assigned, the work has **5 passes** (the main cut and four trims) at these offsets: **0.234, 0.189, 0.154, 0.139 and 0.129 mm**. The last trim runs in the main-cut direction and ends on the contour face. If the header, the preselected technology, the number of passes or the offsets differ, stop and do not cut. Ra is the requested value; only a physical cut can measure the surface.

CamCraft refuses AGIE_BERCO_RA035 for other stock thicknesses, a die opening, an open cutoff, a punch group and a DXF circle or racetrack. No file is written. Use AGIE_QN2 for those jobs.

Run `cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/fine-punch.camscript` into a fresh `FINE20` directory.

## Try the synthetic punch

From the project root, run the supplied script with a local CamCraft desktop CLI build.

```sh
cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/punch.camscript
```

The script imports an original 40 × 20 × 20 mm rectangular punch, places it in 80 × 60 × 20 mm stock and sets the threading point at X−30 Y0. Its post call selects the native separation with `options: {separationLengthMm: 1.0}`. Stock top is Z0 in the job. The `PUNCH40` directory contains the nominal contour in `PUNCH40.ISO`, its import reference in `PUNCH40.SBR` and the piece, start point and requested quality in `PUNCH40.SBL`. Existing destinations are refused. Edit the script’s output name to make another job.

For an oblique entry, run `cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/oblique-punch.camscript` with a fresh `POLY20` destination. This pentagonal punch has a 3:4 sloping entry face. Its threading hole at X−20.5 Y6 approaches X−12.5 Y0 perpendicular to the middle of that face.

Run `cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/slope-punch.camscript` for a 7:10 entry face. The `SLOPE20` job keeps the drilled hole at X−20.5 Y6. Its perpendicular entry falls between five-decimal ISO coordinates. CamCraft can move the entry up to 0.00001 mm toward the waste while preserving the lead, separation room and rough-pass clearance. Both jobs open with zero erosion steps, as the rectangular job does.

Run `cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/notched-punch.camscript` for a 20 mm wide, 1 mm deep reentrant notch. The `NOTCH20` job enters at X−20 Y0 from the hole at X−30 Y0. The nominal notch tip is at X0 Y9. The job opens with zero erosion steps, as the other jobs do.

Run `cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/die-plate.camscript` for a die opening. The script selects the complete upper rim of a 60 × 60 × 20 mm plate in 70 × 70 × 20 mm stock. The 38-sided opening has straight walls at X±12.5 and faceted semicircular ends. The `DIE20` job enters the wall at X−12.5 Y0 from the hole at X−5.5 Y0. It opens with zero erosion steps. Its setup script is the punch setup with the die side.

## Group captured punches

Use one setup with 20 mm stock and two to nine complete modeled punches. Create a **Wire Contour** for each punch. Select the punch model, the wire side **Punch** and the same wire tool and condition **AGIE_QN2** for all of them. Give each operation its own threading hole, and confirm the capture separately for each punch. Each model must be the punch of one operation. Another model, a placed drawing, a die opening, an open cutoff or a tenth operation is refused.

Keep a clear band along X or Y between the complete cutting areas of every pair of punches, also of two punches that are not adjacent in the program. The area of each punch includes the solid, the threading hole, the 3 mm lead, the furthest rough corner of the 20 mm row and the 0.125 mm wire radius. CamCraft refuses the job when two areas are not separated, and the message names the two works. This check can refuse some safe nested or diagonal layouts.

Choose a new job name of up to seven letters and digits. The post adds one digit to the name for each ISO object, for example `PAIR201.ISO` and `PAIR202.ISO`. The one SBR lists all ISO objects, and the one SBL has one group with the works `CUT1`, `CUT2` and so on in program order. All works use the stock origin and the one requested quality. Each work opens with zero erosion steps. Assign TECCUT to each work. In the control preview, check the profile and separation sequence and the threading hole of each work. The control can complete the profile passes of all punches before it starts their separation cuts.

Run the original example from the repository root:

```sh
cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/punch-pair.camscript
```

The left rectangular punch starts at X-30 Y0. The right five-sided punch starts at X1 Y0. Copy all four files of the `PAIR20` directory together.

The three-punch example writes a five-file `TRIO20` job. The grid example places nine rectangular punches on a 32 mm pitch on 120 × 100 × 20 mm stock and writes eleven files in the `NINE20` directory. Each punch starts 13 mm left of its centre.

```sh
cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/punch-trio.camscript
cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/punch-grid.camscript
```

## Post a straight open cutoff

Use 20 mm cold die steel, Berco Cut 0.25 mm and AGIE_QN2. Model the retained solid as one full-height prism. Select the straight X- or Y-parallel wall edge that reaches opposite faces of the rectangular stock. In **Geometry**, choose **Left** or **Right** so the kerf lies on the waste side as the selected chain is traversed. Clear **Start hole stated**. The native post extends each end of the nominal cut 4 mm into air. It uses a separate perpendicular approach at each end.

Clamp both resulting pieces independently. Keep the clamps clear of the entire wire path, the 4 mm tails and both 3 mm approaches. The rough pass severs the offcut before three further trims. A catcher alone allows the offcut to move. Center the stock on the midpoints of the installed X and Y strokes. In **Passes**, confirm **Both pieces clamped through all passes** and **Stock centered in installed XY travel**. The post checks the air tails, approach points, work origin and wire envelope against the centered 350 × 250 mm travel. It uses the rough offset of the 20 mm row (0.245 mm) plus the 0.125 mm wire radius. Use a run-out clearance of 3.6 mm or less.

Choose **Native wire EDM job files**, set **Separation cut length (mm)** to zero and choose a new output directory. The controller alternates travel across the part on successive passes. After TECCUT, the four passes of the 20 mm row are offset 0.245, 0.155, 0.140 and 0.134 mm toward the waste. Inspect the formal check and the plotted four cutting passes before machining.

Run either original example from the repository root:

```sh
cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/cutoff-lower.camscript
cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/cutoff-upper.camscript
```

The examples select opposite sides of synthetic 80 × 60 × 20 mm stock. Each writes one native job, named `LOWER20` or `UPPER20`. Each job opens with zero erosion steps and no separation cut. The left job has the side `c_left` and the right job has `c_right`. The entry point `stp1` and the exit point `stp2` are 4 mm beyond opposite stock faces and 3 mm to the waste side of the line. Assign the matching TECCUT row on the control before the formal check.

## Punch from a DXF circle or racetrack

Create box stock of 20 mm thickness, import one DXF profile and place it on the stock top. Create a captured vertical **Wire Contour** punch. In **Geometry**, select the closed chain of the punch outline. The chain must be one DXF `CIRCLE`, or a racetrack of two straight lines and two tangent semicircular `ARC` entities with the same radius. The straight sides must be at least 6 mm long. Other geometry of the drawing must stay clear of the cut.

The generator and the wire laws use a polygon that encloses each true arc within 0.000025 mm. The native ISO object has the nominal arcs: `G03` blocks with the exact arc centres as incremental `I` and `J` values. The control applies its own wire offset. The setup script is that of the modeled punch.

The native job has these limits:

- The racetrack has straight sides parallel to X or Y. Place the threading hole at least 3 mm outside a straight side, with at least 3 mm of that side on each side of the perpendicular entry.
- The circle has a radius from 6 to 15 mm. Place the threading hole on a horizontal or vertical radius, at least 3 mm beyond the circle. The ISO object starts at that point of the circle and has two semicircles. The control applies the 1 mm separation along the circle.
- The centres, the radius and the arc ends must be on the five-decimal ISO grid.
- The stock must have at least 0.4 mm beyond each arc extremum, and the complete hole must be inside the stock. The rest of the drawing must stay outside the same area.

Die openings with arcs, other stock heights and groups are refused. Run the original examples from the repository root, each with a fresh destination:

```sh
cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/racetrack.camscript
cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/circle-punch.camscript
cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/north-circle.camscript
```

The `RACE20` job has an R10 racetrack, moved and turned by 90 degrees, with its cap centres at X−6 Y1 and X10 Y1. The `CIRCLE20` job has an R10 circle with the hole at X−17 Y0. The `NORTH15` job has an R15 circle with the hole at X0 Y22.

## Punch from a convex DXF loop of tangent lines and arcs

A closed DXF chain of `LINE` and `ARC` entities that is not a circle or a racetrack can also be a punch. Select the chain as for a racetrack. These conditions apply:

- Each joint must be tangent, and the loop must be convex: each arc turns counter-clockwise around the punch, by at most 180 degrees. The arcs can have different radii from 0.5 to 20 mm.
- The loop has at least one straight side and at most 128 segments.
- At least one straight side is parallel to X or Y. Place the threading hole at least 3 mm outside that side, with at least 3 mm of that side on each side of the perpendicular entry. The other sides can be oblique.
- All joints and arc centres must be on the five-decimal ISO grid. The post checks the exact squared radius at both ends of each arc and the tangent direction at each joint in grid units.
- The stock must have at least 0.4 mm beyond each extremum of the loop, and the complete hole must be inside the stock. The rest of the drawing must stay outside the same area.

The generator and the wire laws use a polygon of up to 2,048 vertices that encloses each true arc within 0.000025 mm. The native ISO object keeps each nominal endpoint and arc centre. The control applies its own wire offset to each arc and each straight.

Run the original examples from the repository root, each with a fresh destination:

```sh
cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/rounded-punch.camscript
cargo run -p camcraft-app --bin camcraft-cli -- script examples/agie/three-corner-punch.camscript
```

The `ROUND20` job has four corner arcs with alternating radii of 6 and 4 mm, and the hole at X0 Y−21. The `THREEARC` job has three arcs of R5 that sweep about 143.13, 126.87 and 90 degrees, an oblique straight side, and the hole at X12 Y−12.

## Open the job in Agie Control

Keep all files of the job directory together. A punch group has one ISO object for each work: import each of them. In the control’s file manager, open **Imp/exp** and import the ISO object into **ISO Sys**. Then import the SBL into **SBL Sys**, keeping the matching SBR beside the SBL. In the workpiece list choose **New workpiece → Script Files**, select the imported SBL and confirm with **OK**.

Select the newly created piece. After a repeat import, an earlier piece can retain its machining steps. Match the new work’s ISO geometry and threading hole to the setup. Check that the new work shows **[0]**. At this point **FORMALCHECK** should report undefined technology.

1. Open **Work Details** on the new work. Verify steel **0001**, wire **Berco Cut 0.25**, the stock height you selected (**15**, **20**, **40** or **80 mm** for a punch, **20 mm** for a die) and the effective work quality. The requested **CCB030** values are **Ra 0.30**, **Te 1.50** and **Tkm 1.00**, with three options disabled. For **CCB035** the requested Ra is **0.35** and the speed option is set. Check the two options the control displays. A resident quality with the same name can hold different values.
2. If any visible value differs, use **Quality → Define** on this work to select the requested values. Confirm the effective settings before proceeding.
3. Open **Technology**, select **TECCUT** and choose **Assign** on the matching row. Inspect the generated steps. The rough and trim offsets must agree with the row of the selected stock thickness in [QN2 offsets by stock thickness](#qn2-offsets-by-stock-thickness), in that order. The post review names the same four offsets. For **CCB035**, check the technology name and the five offsets under **Request Ra 0.35 µm for one 20 mm punch** instead. A die plate and an open cutoff are always 20 mm thick, so they use the 20 mm row. For 20 mm stock the offsets are **0.245, 0.155, 0.140 and 0.134 mm**.
4. Run **FORMALCHECK**. Inspect errors and warnings. Save the piece and run **GRAFICHECK**. Check the installed XY datum and travel, the approach from the hole and the compensation: outside a punch, inside a die opening. Verify the 1 mm separation section, trimming sequence and support for release of the punch or the slug. CamCraft does not command a manual pause before the section cut.
