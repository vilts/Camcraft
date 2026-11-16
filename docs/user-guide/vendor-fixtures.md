# Using a vendor fixture or vise STEP

Vendors of fixture plates, subplates and modular vises publish STEP models of
their hardware. This page shows how to bring one into CamCraft as a fixture,
turn it into the right orientation, place it on the machine, and save it to
the fixture library. Labels below are the ones the app shows.

## 1. Open the model: units and scale

1. Choose **File > Model** (Ctrl+O) and pick the `.step` (or `.stp`) file.
2. A STEP file that declares inches is read in inches and shown in
   millimetres. Check one known dimension against the vendor's drawing (for
   example, the overall length of a plate). If it is off by 25.4 times, the
   file's unit is not what the drawing says; ask the vendor for a metric or
   corrected model before going on.
3. **Setup > Stock** has a **Units** choice, **Millimeters** or **Inches**.
   It only changes how numbers are shown and typed. The model itself is
   always stored in millimetres.

## 2. Orient it to sit on the table

The model must sit with its mounting face down, on the machine table.

1. Select the imported model in the browser tree to open its **Model**
   page. The **Turn X+ / X- / Y+ / Y- / Z+ / Z-** buttons rotate the model
   a quarter turn about the world axes and apply at once.
2. For any other angle, type **Tilt about X (roll)**, **Tilt about Y
   (pitch)** and **Turn about Z (yaw)** in degrees (and **X**, **Y**, **Z**
   to move it), then press **Apply**. Every change is one undo step.
   (The turn and **Align WCS (3-2-1)** controls on **Setup > Origin** move
   the setup's work coordinate system, not the model. Don't use them to
   stand a fixture up.)
3. Look at the model from the top before going on: the part should stand
   the way it will sit on the machine.

## 3. Mark it as a fixture

1. In the browser tree, open the model's menu and choose **Use as fixture**
   (or **Stop using as fixture** to undo it).
2. A fixture is checked for collisions with the toolpaths. It is not
   machined: CamCraft refuses an operation whose source is a fixture.
3. To add a fixture from a file directly, open the setup's **Fixtures**
   node and choose **Import fixture…**.

## 4. Place it on the machine

1. Open **Setup > Setups**, find the fixture in the **Fixtures** section and
   choose **Locate part…** on its row.
2. Pick one to three mates: a part face or vertex, then the fixture face or
   vertex it sits on. Typical mates are a floor, a fixed jaw and a stop
   (three points, 3-2-1).
3. Set an offset in millimetres or a turn in degrees if needed, then choose
   **Apply locate**. CamCraft refuses a placement that puts the part inside
   the fixture and says why.
4. The same placement can be started from the part's **Locate in fixture**
   row on the model page.

## 5. Save it to the library and reuse it

1. On the setup's **Fixtures** node, choose **Save fixture to library…**.
   CamCraft asks before it replaces an entry with the same name.
2. In another setup or document, open the **Fixtures** node and choose
   **Insert from library…**, then pick the entry.

## 6. Limits to know

- **Fixed jaw opening.** A vise model from a vendor has one jaw opening.
  Model it at the opening you will use, or use the built-in parametric vises
  (shipped in PR #802, #788).
- **Fixture plate hole grid.** Built-in fixture plates place fixtures on a
  grid of holes in one step (shipped in PR #823, #819). Use **Locate part…**
  to place a fixture that is not on a plate grid.
- **Check clearances.** The model is only as accurate as the vendor's file.
  Check the clearance against the real hardware before a first cut.
