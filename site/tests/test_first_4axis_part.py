"""Pin the "Your first 4-axis part" guide page (#1904).

The page is held to three things it describes: the UI sources (each label
it prints in bold is a string literal there), the example project (each
number in its tables is the project's), and the posted program (each G-code
excerpt is a run of lines of `examples/first-4axis-part.nc`, which
`crates/app/tests/first_4axis_part.rs` holds to what the post emits).

Provenance: CamCraft's own guide, UI sources, example project and post.
"""

from pathlib import Path
import json
import re
import unittest


ROOT = Path(__file__).resolve().parents[2]
PAGE = ROOT / "site/content/guide/getting-started/first-4axis-part.md"
PROJECT = ROOT / "examples/first-4axis-part.camproj"
PROGRAM = ROOT / "examples/first-4axis-part.nc"
MODEL = ROOT / "examples/models/first-4axis-part.obj"
MACHINE = ROOT / "assets/machines/tormach-770mx-microarc4.mch"
POST = ROOT / "assets/posts/tormach.cps"
UI = "crates/ui/src/egui_app/"
IN = 25.4

# Each label the page prints in bold, and the source that draws it.
UI_STRINGS = {
    "Open": UI + "panes.rs",
    "Model": UI + "panes.rs",
    "Stock": UI + "panes.rs",
    "Origin": UI + "panes.rs",
    "Tools": UI + "panes.rs",
    "Post": UI + "panes.rs",
    "Paths": UI + "panes.rs",
    "Simulate": UI + "panes.rs",
    "Deviate": UI + "panes.rs",
    "Machine": UI + "browser_tree.rs",
    "Library": UI + "machine_page.rs",
    "Units": UI + "stock_page.rs",
    "Inches": "crates/geometry/src/geometry/units.rs",
    "Box": UI + "stock_page.rs",
    "Fit margin": UI + "stock_page.rs",
    "Fit to Models": UI + "stock_page.rs",
    "Length (X)": UI + "stock_page.rs",
    "Width (Y)": UI + "stock_page.rs",
    "Height (Z)": UI + "stock_page.rs",
    "X center": UI + "stock_page.rs",
    "Y center": UI + "stock_page.rs",
    "Z bottom": UI + "stock_page.rs",
    "X origin": UI + "wcs_page.rs",
    "Y origin": UI + "wcs_page.rs",
    "Z origin": UI + "wcs_page.rs",
    "New tool…": UI + "tool_library.rs",
    "Name": UI + "tool_settings_grid.rs",
    "Diameter": UI + "tool_settings_grid.rs",
    "Flute length": UI + "tool_settings_grid.rs",
    "Shank diameter": UI + "tool_settings_grid.rs",
    "Spindle rpm": UI + "tool_settings_grid.rs",
    "Add tool": UI + "tool_settings.rs",
    "Tool number": UI + "tool_settings_grid.rs",
    "Slot": "crates/app/src/operation/kinds.rs",
    "Milling 2D": "crates/app/src/operation/kinds.rs",
    "Tool": UI + "op_page/header.rs",
    "Source": UI + "op_page/header.rs",
    "Coolant": UI + "op_page/header.rs",
    "Rename": UI + "op_page.rs",
    "Apply name": UI + "op_page.rs",
    "Generate": UI + "op_page.rs",
    "Geometry": UI + "op_tabs.rs",
    "Heights": UI + "op_tabs.rs",
    "Feeds": UI + "op_tabs.rs",
    "Manual": UI + "mill2d_more_page.rs",
    "A X": UI + "mill2d_more_page.rs",
    "A Y": UI + "mill2d_more_page.rs",
    "B X": UI + "mill2d_more_page.rs",
    "B Y": UI + "mill2d_more_page.rs",
    "Slot width": UI + "mill2d_more_page.rs",
    "Clearance Z": UI + "mill2d_more_page.rs",
    "Top Z": UI + "mill2d_more_page.rs",
    "Bottom Z": UI + "mill2d_more_page.rs",
    "Stepdown": UI + "mill2d_more_page.rs",
    "Feed": UI + "mill2d_more_page.rs",
    "Plunge feed": UI + "mill2d_more_page.rs",
    "Tilted plane (3+2)": UI + "fields.rs",
    "Tool axis A": UI + "fields.rs",
    "Tool axis B": UI + "fields.rs",
    "Plane origin X": UI + "fields.rs",
    "Plane origin Y": UI + "fields.rs",
    "Plane origin Z": UI + "fields.rs",
    "From": UI + "heights_page.rs",
    "Top of stock": "crates/app/src/operation/types/height.rs",
    "Offset": "crates/ui/src/operations.rs",
    "Explain G-code (add a comment to each block)": UI + "post_options.rs",
}
# Bold text that is not a UI label: product names and lead-in phrases.
NOT_UI = {
    "Tormach 770MX", "microARC 4", "Mount the machine.", "Pick the post.",
    "Set the unit.", "Import the model.", "Fit the stock.",
    "Check the work offset.", "The heights are the same on each face.",
    "The slot ends are typed in the tilted plane.",
    "The face opposite the top is A 0, B 180.", "The machine check.",
    "The index swing.", "Shank and holder.", "The index.", "The two notes",
    "The cut.", "Check the A direction.",
    "Check the collet nut against the chuck.",
    "Check the swing at the top of Z.", "Know your G30 position.",
    "Air-cut first.", "Cut.",
}
# Text the page quotes from a refusal or a note, and the source that holds it.
QUOTED = {
    "simultaneous 4-axis motion": POST,
    "states no A feed rate": POST,
    "A counts on through 360": POST,
    "the A axis about X cannot reach it": POST,
    "is beyond the A axis travel": POST,
    "arc on an indexed A": POST,
    "the indexed frame posts lines only": POST,
    "stock sim did not judge this program": ROOT / "crates/app/src/posting_sim.rs",
    "posed moves are not modelled by the column engine (its atoms stand upright on +Z; a tilted tool cut upright is a wrong verdict)":
        ROOT / "crates/sim/src/stock_sim/cut.rs",
    "a tilted plane cannot yet index a picked selection - pick a whole model instead":
        ROOT / "crates/app/src/operation/job.rs",
}
OPERATIONS = ["Flat A-90", "Flat A0", "Flat A90", "Flat A180", "Cross slot A0"]


def page():
    return PAGE.read_text()


def flat(text):
    return " ".join(text.split())


def tables(source):
    """Each Markdown table as a list of rows of stripped cells."""
    out, rows = [], []
    for line in source.splitlines() + [""]:
        if line.startswith("|"):
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if not all(set(c) <= set("-: ") for c in cells):
                rows.append(cells)
        elif rows:
            out.append(rows)
            rows = []
    return out


def table_with(source, first_header, second_header):
    for rows in tables(source):
        if rows[0][0] == first_header and second_header in rows[0][1]:
            return rows
    raise AssertionError(f"no table headed {first_header} | {second_header}")


def project():
    return json.loads(PROJECT.read_text())


def entities(kind):
    return {e["name"]: e[kind] for e in project()["entities"] if kind in e}


class FirstFourAxisPart(unittest.TestCase):
    def test_each_bold_ui_label_is_a_string_in_its_source(self):
        bold = set(re.findall(r"\*\*([^*]+)\*\*", page()))
        unknown = bold - set(UI_STRINGS) - NOT_UI
        self.assertFalse(unknown, f"bold text with no source: {sorted(unknown)}")
        for label, source in UI_STRINGS.items():
            with self.subTest(label=label):
                self.assertIn(label, bold, "the table names a label the page does not print")
                self.assertIn(f'"{label}"', (ROOT / source).read_text(),
                              f"{source} does not draw this label")

    def test_machine_and_post_are_the_shipped_ones(self):
        text = page()
        machine = json.loads(MACHINE.read_text())
        name = machine["general"]["model"]
        self.assertIn(f"`{name}`", text)
        self.assertIn(f'("{name}", AssetFile("{MACHINE.name}"))',
                      (ROOT / "crates/app/src/posting_files.rs").read_text())
        self.assertIn('"Tormach PathPilot"', (ROOT / "crates/app/src/posting_files.rs").read_text())
        self.assertIn("`Tormach PathPilot`", text)
        self.assertEqual(project()["machine"]["machine"]["model"], name)
        self.assertEqual(project()["post"], {"named": "Tormach PathPilot"})
        self.assertEqual(project()["units"], "in")
        rotary = [p for p in machine["kinematics"]["default"]["parts"][0]["parts"]
                  if p.get("type") == "rotary"]
        self.assertEqual(len(rotary), 1)
        self.assertEqual((rotary[0]["name"], rotary[0]["axis"]), ("A", [1.0, 0.0, 0.0]))
        self.assertNotIn("travel", rotary[0])
        self.assertIn("with no travel limit", flat(text))
        self.assertIn(f"`nose_to_axis` {rotary[0]['nose_to_axis']}", flat(text))

    def test_stock_and_model_numbers_are_the_projects(self):
        text = flat(page())
        stock = [e for e in project()["entities"] if "stock" in e][0]
        box = stock["stock"]["box"]
        for label, value in (("Length (X)", box["dx"]), ("Width (Y)", box["dy"]),
                             ("Height (Z)", box["dz"]), ("X center", stock["translation"][0]),
                             ("Y center", stock["translation"][1]),
                             ("Z bottom", stock["translation"][2])):
            with self.subTest(label=label):
                self.assertIn(f"**{label}** {round(value / IN, 4):g}", text)
        xs, ys, zs = [], [], []
        for line in MODEL.read_text().splitlines():
            if line.startswith("v "):
                x, y, z = (float(n) for n in line.split()[1:4])
                xs.append(x), ys.append(y), zs.append(z)
        size = [round((max(a) - min(a)) / IN, 4) for a in (xs, ys, zs)]
        self.assertEqual(size, [1.5, 0.75, 0.75])
        self.assertIn("1.5 x 0.75 x 0.75 in", text)
        self.assertAlmostEqual(max(xs), 0.0)
        self.assertAlmostEqual(max(ys) + min(ys), 0.0)
        self.assertAlmostEqual(max(zs) + min(zs), 0.0)

    def test_tool_table_is_the_projects_tools(self):
        rows = {r[0].strip("*"): r[1:] for r in table_with(page(), "Field", "T1")[1:]}
        tools = entities("tool")
        names = rows["Name"]
        self.assertEqual(sorted(names), sorted(tools))
        fields = {"Diameter": ("diameter", IN), "Flute length": ("flute_len", IN),
                  "Shank diameter": ("shank_d", IN), "Spindle rpm": ("rpm", 1.0),
                  "Feed (in/min)": ("feed", IN)}
        for label, (key, scale) in fields.items():
            for name, cell in zip(names, rows[label]):
                with self.subTest(tool=name, field=label):
                    self.assertAlmostEqual(float(cell), tools[name][key] / scale, places=6)
                    self.assertEqual(tools[name]["kind"], "end_mill")

    def test_operation_tables_are_the_projects_operations(self):
        ops = entities("operation")
        self.assertEqual(sorted(ops, key=lambda n: ops[n]["order"]), OPERATIONS)
        index = {r[0]: r[1:] for r in table_with(page(), "Operation", "Tool")[1:]}
        cut = {r[0]: r[1:] for r in table_with(page(), "Operation", "A X")[1:]}
        self.assertEqual(list(index), OPERATIONS)
        self.assertEqual(list(cut), OPERATIONS)
        for name in OPERATIONS:
            slot = ops[name]["params"]["slot"]
            with self.subTest(operation=name):
                tool, tilted, a_deg, b_deg = index[name]
                self.assertEqual(tool, ops[name]["tool"])
                self.assertEqual(ops[name]["src"], "stock")
                if "tilt" in slot:
                    self.assertEqual(tilted, "on")
                    self.assertEqual([float(a_deg), float(b_deg)],
                                     [slot["tilt"]["a_deg"], slot["tilt"]["b_deg"]])
                    self.assertEqual(slot["tilt"]["origin"], [0.0, 0.0, 0.0])
                else:
                    self.assertEqual([tilted, a_deg, b_deg], ["off", "", ""])
                a, b, width, clearance, top, bottom, stepdown, feed, plunge = cut[name]
                want = [float(n) for n in (a + "," + b).split(",")]
                have = [v / IN for v in slot["a"] + slot["b"]]
                for w, h in zip(want, have):
                    self.assertAlmostEqual(w, h, places=6)
                for cell, key in ((clearance, "clearance_z"), (top, "top_z"), (bottom, "bottom_z")):
                    self.assertEqual(slot[key]["reference"], "top_of_stock")
                    self.assertAlmostEqual(float(cell), slot[key]["offset"] / IN, places=6)
                for cell, key in ((width, "width"), (stepdown, "stepdown"),
                                  (feed, "feed"), (plunge, "plunge_feed")):
                    self.assertAlmostEqual(float(cell), slot[key] / IN, places=6)

    def test_part_table_names_each_operation_with_its_index_and_tool(self):
        rows = table_with(page(), "Operation", "Index")[1:]
        self.assertEqual([r[0] for r in rows], OPERATIONS)
        program = PROGRAM.read_text().splitlines()
        indexes = [line for line in program if line.startswith("G0 A")]
        self.assertEqual([r[1].strip("`") for r in rows], indexes)
        tools = [line[1:3] for line in program if re.match(r"\(T\d ", line)]
        self.assertEqual([r[2][:2] if r[2].startswith("T") else None for r in rows][0], tools[0])
        self.assertEqual(rows[-1][2][:2], tools[-1])

    def test_gcode_excerpts_are_runs_of_the_posted_program(self):
        program = PROGRAM.read_text()
        blocks = re.findall(r"```gcode\n(.*?)```", page(), flags=re.S)
        self.assertGreaterEqual(len(blocks), 4)
        for number, block in enumerate(blocks):
            with self.subTest(excerpt=number):
                self.assertTrue(block.strip())
                self.assertIn("\n" + block, "\n" + program,
                              "the excerpt is not a run of whole lines of the program")
        self.assertTrue(program.startswith(blocks[0]), "the first excerpt is the program's start")
        self.assertTrue(program.rstrip("\n").endswith(blocks[-1].rstrip("\n")),
                        "the last excerpt is the program's end")

    def test_inline_gcode_words_are_in_the_posted_program(self):
        lines = PROGRAM.read_text().splitlines()
        for word in ("G20", "M8", "G53 G0 Z0", "G0 A-90", "G0 A0", "G0 A90", "G0 A180",
                     "G30", "M30"):
            with self.subTest(word=word):
                self.assertIn(f"`{word}`", page())
                self.assertTrue(any(word == l or l.startswith(word + " ") for l in lines))
        self.assertEqual(lines[-2:], ["G53 G0 Z0", "M30"])
        for i, line in enumerate(lines):
            if line.startswith("G0 A"):
                self.assertEqual(lines[i - 1], "G53 G0 Z0")
            if line.startswith("G1 "):
                self.assertNotRegex(line, r" [ABC]-?\d")

    def test_quoted_refusals_and_notes_are_in_their_sources(self):
        text = flat(page())
        for quote, source in QUOTED.items():
            with self.subTest(quote=quote[:40]):
                self.assertIn(quote, text)
                self.assertIn(quote, source.read_text())

    def test_cli_command_and_companion_files_exist(self):
        text = page()
        self.assertIn("camcraft-cli process examples/first-4axis-part.camproj -o first-4axis-part.nc",
                      text)
        for path in (PROJECT, PROGRAM, MODEL):
            with self.subTest(path=path.name):
                self.assertTrue(path.is_file())
                self.assertIn(f"`{path.relative_to(ROOT).as_posix()}`", text)
        self.assertIn("camcraft-cli process  <job.camproj> [-o <out.nc>]",
                      (ROOT / "crates/app/src/cli.rs").read_text())

    def test_page_is_linked_from_getting_started_first_run_and_multi_axis(self):
        guide = ROOT / "site/content/guide"
        link = "@/guide/getting-started/first-4axis-part.md"
        for path in ("getting-started/_index.md", "getting-started/first-run.md",
                     "multi-axis/_index.md"):
            with self.subTest(page=path):
                self.assertIn(link, (guide / path).read_text())
        first_run = flat((guide / "getting-started/first-run.md").read_text())
        self.assertRegex(first_run, r"first-router-part\.md\).{0,140}first-4axis-part\.md\)")

    def test_continuous_work_is_pointed_at_not_walked_through(self):
        text = flat(page())
        self.assertIn("issues/796", text)
        self.assertIn("is not walked through here", text)
        for section in (text.split("## The part and how it is held")[0],
                        text.split("## What the post refuses")[1]):
            with self.subTest(section=section[:60]):
                self.assertIn("hand-written motion tests in the PathPilot simulator "
                              "confirm the control behavior", section)
                self.assertIn("posted continuous program still needs a simulator "
                              "run and a hardware check", section)
                self.assertNotIn("has been run in the PathPilot simulator only", section)

    def test_continuous_feed_guide_states_the_tormach_exception_consistently(self):
        text = flat((ROOT / "site/content/guide/multi-axis/continuous-4-axis.md")
                    .read_text())
        feed = text.split("## Feed on a rotary move")[1].split("## Posts")[0]
        self.assertIn("`tormach.cps` uses inverse-time feed (`G93`) for "
                      "simultaneous A-about-X moves", feed)
        self.assertIn("The other shipped posts write a plain F word without "
                      "inverse-time or degrees-per-minute conversion", feed)
        self.assertIn("For those posts, a control that requires inverse-time feed", feed)
        missing = text.split("## Not implemented")[1]
        self.assertIn("Inverse-time feed in posts other than `tormach.cps`", missing)
        self.assertIn("Tormach supports `G93` for simultaneous A-about-X moves only", missing)


if __name__ == "__main__":
    unittest.main()
