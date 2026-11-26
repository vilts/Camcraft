"""Pin the "Your first continuous 4-axis program" guide page (#1904).

The page is held to what it describes: the UI sources (each label it prints
in bold is a string literal there), the example project (each number in its
tables is the project's), the posted program (each G-code excerpt is a run
of lines of `examples/first-continuous-4axis-part.nc`, which
`crates/app/tests/continuous_4axis_part.rs` holds to what the project
posts), the post and machine file (each quoted refusal and value), and its
pictures (drawn from the example by `site/tools`, never by hand).

Provenance: CamCraft's own guide, UI sources, example project and post.
"""

from pathlib import Path
import importlib.util
import json
import re
import unittest


ROOT = Path(__file__).resolve().parents[2]
GUIDE = ROOT / "site/content/guide"
PAGE = GUIDE / "getting-started/first-continuous-4axis-part.md"
PROJECT = ROOT / "examples/first-continuous-4axis-part.camproj"
PROGRAM = ROOT / "examples/first-continuous-4axis-part.nc"
MODEL = ROOT / "examples/models/first-continuous-4axis-part.obj"
MACHINE = ROOT / "assets/machines/tormach-770mx-microarc4.mch"
POST = ROOT / "assets/posts/tormach.cps"
PICTURES = ROOT / "site/static/guide-pictures"
TOOL = ROOT / "site/tools/first_continuous_4axis_pictures.py"
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
    "Apply": UI + "stock_page.rs",
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
    "Type": UI + "tool_settings_grid.rs",
    "Ball nose": UI + "tool_settings_grid.rs",
    "Name": UI + "tool_settings_grid.rs",
    "Diameter": UI + "tool_settings_grid.rs",
    "Flute length": UI + "tool_settings_grid.rs",
    "Shank diameter": UI + "tool_settings_grid.rs",
    "Spindle rpm": UI + "tool_settings_grid.rs",
    "Add tool": UI + "tool_settings.rs",
    "Rotary Parallel": "crates/app/src/operation/kinds.rs",
    "Multi-axis": "crates/app/src/operation/kinds.rs",
    "Tool": UI + "op_page/header.rs",
    "Source": UI + "op_page/header.rs",
    "Rename": UI + "op_page.rs",
    "Apply name": UI + "op_page.rs",
    "Generate": UI + "op_page.rs",
    "Heights": UI + "op_tabs.rs",
    "Passes": UI + "op_tabs.rs",
    "Feeds": UI + "op_tabs.rs",
    "Clearance Z": UI + "kind_editors.rs",
    "Top Z": UI + "kind_editors.rs",
    "Bottom Z": UI + "kind_editors.rs",
    "Stepover": UI + "kind_editors.rs",
    "Auto": UI + "kind_editors.rs",
    "Tool lead angle": UI + "kind_editors.rs",
    "Tool lag angle": UI + "kind_editors.rs",
    "Max tilt": UI + "kind_editors.rs",
    "Tolerance": UI + "kind_editors.rs",
    "Feed": UI + "kind_editors.rs",
    "Plunge feed": UI + "kind_editors.rs",
    "Climb milling": UI + "kind_editors.rs",
    "From": UI + "heights_page.rs",
    "Offset": "crates/ui/src/operations.rs",
}
# Bold text that is not a UI label: product names, operation names and lead-ins.
NOT_UI = {
    "Tormach 770MX", "microARC 4", "Rotary Contour", "Rotary Offset",
    "Air-cut this program before you cut a bar with it.",
    "Mount the machine.", "Pick the post.", "Set the unit.", "Import the model.",
    "Set the stock.", "Check the work offset.", "The clearance is a radius.",
    "Top and bottom keep the whole bar.", "The tolerance and the stepover are coarse.",
    "The free end face.", "The machine check.", "The radial check.",
    "Check the A direction.", "Empty the chuck.", "Know your G30 position.",
    "Run the start in single block.", "Air-cut the whole program.", "Cut only after a good air cut.",
}
# Text the page quotes from a refusal or a note, and the source that holds it.
QUOTED = {
    "simultaneous 4-axis motion": POST,
    "states no A feed rate": POST,
    "A counts on through 360 (PathPilot does not roll A over)": POST,
    "the post writes a turning A on straight G0 / G1 moves only": POST,
    "its path and time cannot be judged": POST,
    "still bows": POST,
    "the A axis about X cannot reach it": POST,
    "stock sim did not judge this program": ROOT / "crates/app/src/posting_sim.rs",
    "a rotary program turns the part under a posed tool": ROOT / "crates/app/src/stock_sim_rotary.rs",
    "that end face is left, and the tool stops at its plane":
        ROOT / "crates/motion/src/multiaxis/rotary.rs",
}


def page():
    return PAGE.read_text()


def flat(text):
    return " ".join(text.split())


def rows(header):
    """The Markdown table whose header row is `header`, as rows of cells."""
    found, out = False, []
    for line in page().splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")] if line.startswith("|") else None
        if cells == header:
            found = True
        elif found and cells is None:
            return out
        elif found and not all(set(c) <= set("-: ") for c in cells):
            out.append(cells)
    raise AssertionError(f"no table headed {header}")


def entities(kind):
    project = json.loads(PROJECT.read_text())
    return {e["name"]: e[kind] for e in project["entities"] if kind in e}


def program():
    return PROGRAM.read_text().splitlines()


def a_words():
    return [float(w[1:]) for line in program() if not line.startswith("(")
            for w in line.split() if w[0] == "A"]


class FirstContinuousFourAxisProgram(unittest.TestCase):
    def test_each_bold_ui_label_is_a_string_in_its_source(self):
        bold = set(re.findall(r"\*\*([^*]+)\*\*", page()))
        unknown = bold - set(UI_STRINGS) - NOT_UI
        self.assertFalse(unknown, f"bold text with no source: {sorted(unknown)}")
        for label, source in UI_STRINGS.items():
            with self.subTest(label=label):
                self.assertIn(label, bold, "the table names a label the page does not print")
                self.assertIn(f'"{label}"', (ROOT / source).read_text(),
                              f"{source} does not draw this label")
        self.assertIn('"Fixed value"', (ROOT / "crates/app/src/operation/types/height.rs").read_text())
        self.assertIn("Fixed value", page())

    def test_machine_table_is_the_machine_file(self):
        text = flat(page())
        machine = json.loads(MACHINE.read_text())
        name = machine["general"]["model"]
        self.assertIn(f"`{name}`", text)
        project = json.loads(PROJECT.read_text())
        self.assertEqual(project["machine"]["machine"]["model"], name)
        self.assertEqual(project["post"], {"named": "Tormach PathPilot"})
        self.assertEqual(project["units"], "in")
        self.assertIn("`Tormach PathPilot`", text)
        table = {r[0]: r[1] for r in rows(["In the machine file", "Value", "What uses it"])}
        raw = MACHINE.read_text()
        self.assertRegex(raw, r'"name": "A", "type": "rotary"')
        axes = machine["kinematics"]["default"]["parts"][0]["parts"]
        rotary = next(p for p in axes if p["name"] == "A")
        self.assertNotIn("travel", rotary)
        self.assertEqual(rotary["axis"], [1.0, 0.0, 0.0])
        self.assertEqual(rotary["nose_to_axis"], 10.6)
        saved_a = next(a for a in project["machine"]["machine"]["axes"] if a["name"] == "A")
        self.assertIsNone(saved_a["travel"], "the example uses the shipped unbounded A axis")
        self.assertRegex(raw, r'"A": \{ "coordinate": 3, "max_normal_speed": 360\.0 \}')
        self.assertEqual(table["A rotary about X"], "`axis` 1, 0, 0")
        self.assertEqual(table["A travel"], "Not stated")
        self.assertEqual(table["`nose_to_axis`"], "10.6 (inches)")
        self.assertEqual(table["A `max_normal_speed`"], "360 (degrees per minute)")

    def test_continuous_a_claims_match_the_shipped_machine(self):
        text = flat(page())
        reference = flat((GUIDE / "multi-axis/continuous-4-axis.md").read_text())
        self.assertIn("travel is unstated in both the shipped file and this example project", text)
        self.assertIn("former -360..360 travel refusal no longer applies", text)
        self.assertIn("former -360..360 travel refusal no longer applies", reference)
        # #3271: no check reads docs/decisions/: the cut moves each file
        # into RECOMMENDED.md. The shipped A axis has no travel: the
        # project test above reads it from the saved machine.
        self.assertIn("chart scale is not a machine limit", text)
        index = flat((GUIDE / "multi-axis/_index.md").read_text())
        self.assertIn("A about X, no stated A travel limit, `nose_to_axis` 10.6", index)
        for source in (text, reference):
            self.assertNotIn("past the A travel and the post refuses", source)
            self.assertNotIn("with its A travel of -360 to 360", source)
        picture = (PICTURES / "first-continuous-4axis-a-count.svg").read_text()
        self.assertIn("Chart scale only; the shipped file states no A travel limit", picture)
        self.assertNotIn("A travel in the machine file: -360 to 360", picture)

    def test_stock_and_model_numbers_are_the_projects(self):
        text = flat(page())
        stock = [e for e in json.loads(PROJECT.read_text())["entities"] if "stock" in e][0]
        box = stock["stock"]["box"]
        for label, value in (("Length (X)", box["dx"]), ("Width (Y)", box["dy"]),
                             ("Height (Z)", box["dz"]), ("X center", stock["translation"][0]),
                             ("Y center", stock["translation"][1]),
                             ("Z bottom", stock["translation"][2])):
            with self.subTest(label=label):
                self.assertIn(f"**{label}** {round(value / IN, 4):g}", text)
        xs, radii = [], []
        for line in MODEL.read_text().splitlines():
            if line.startswith("v "):
                x, y, z = (float(n) for n in line.split()[1:4])
                xs.append(x), radii.append((y * y + z * z) ** 0.5)
        self.assertAlmostEqual((max(xs) - min(xs)) / IN, 1.25, places=5)
        self.assertAlmostEqual(max(xs), 0.0)
        self.assertAlmostEqual(2 * max(radii) / IN, 0.680, places=5)
        self.assertIn("0.680 in across and 1.25 in long", text)

    def test_tool_table_is_the_projects_tool(self):
        table = {r[0].strip("*"): r[1] for r in rows(["Field", "T1"])}
        tools = entities("tool")
        self.assertEqual(list(tools), [table["Name"]])
        tool = tools[table["Name"]]
        self.assertEqual(tool["kind"], "ball_nose")
        for label, key, scale in (("Diameter", "diameter", IN), ("Flute length", "flute_len", IN),
                                  ("Shank diameter", "shank_d", IN), ("Spindle rpm", "rpm", 1.0),
                                  ("Feed (in/min)", "feed", IN)):
            with self.subTest(field=label):
                self.assertAlmostEqual(float(table[label]), tool[key] / scale, places=6)

    def test_operation_table_is_the_projects_operation(self):
        ops = entities("operation")
        self.assertEqual(list(ops), ["Journal"])
        self.assertIn("type `Journal`", page())
        op = ops["Journal"]
        self.assertEqual(op["tool"], "BN 1/8")
        self.assertEqual(op["src"], {"entity": {"name": "first-continuous-4axis-part"}})
        self.assertIn("`first-continuous-4axis-part`", page())
        params = op["params"]["rotary_parallel"]
        table = {r[1].strip("*"): (r[0].strip("*"), r[2]) for r in rows(["Tab", "Setting", "Value"])}
        for label, key in (("Clearance Z", "clearance_z"), ("Top Z", "top_z"), ("Bottom Z", "bottom_z")):
            with self.subTest(height=label):
                tab, value = table[label]
                self.assertEqual(tab, "Heights")
                self.assertEqual(params[key]["reference"], "fixed")
                found = re.fullmatch(r"\*\*From\*\* Fixed value, \*\*Offset\*\* (-?[\d.]+)", value)
                self.assertIsNotNone(found, value)
                self.assertAlmostEqual(float(found.group(1)), params[key]["offset"] / IN, places=6)
        for label, key, scale, tab in (("Stepover", "stepover", IN, "Passes"),
                                       ("Tool lead angle", "lead_deg", 1.0, "Passes"),
                                       ("Tool lag angle", "lag_deg", 1.0, "Passes"),
                                       ("Max tilt", "max_tilt_deg", 1.0, "Passes"),
                                       ("Tolerance", "tolerance", IN, "Passes"),
                                       ("Feed", "feed", IN, "Feeds"),
                                       ("Plunge feed", "plunge_feed", IN, "Feeds")):
            with self.subTest(setting=label):
                self.assertEqual(table[label][0], tab)
                self.assertAlmostEqual(float(table[label][1].split()[0]), params[key] / scale, places=6)
        self.assertEqual((table["Climb milling"][1], params["climb"]), ("on", True))

    def test_gcode_excerpts_are_runs_of_the_posted_program(self):
        text = PROGRAM.read_text()
        blocks = re.findall(r"```gcode\n(.*?)```", page(), flags=re.S)
        self.assertEqual(len(blocks), 4)
        for number, block in enumerate(blocks):
            with self.subTest(excerpt=number):
                self.assertTrue(block.strip())
                self.assertIn("\n" + block, "\n" + text,
                              "the excerpt is not a run of whole lines of the program")
        self.assertTrue(text.startswith(blocks[0]), "the first excerpt is the program's start")
        self.assertTrue(text.rstrip("\n").endswith(blocks[-1].rstrip("\n")),
                        "the last excerpt is the program's end")

    def test_stated_lines_and_numbers_are_the_programs(self):
        text, lines = flat(page()), program()
        self.assertIn(f"({len(lines)} lines)", text)
        self.assertEqual(lines[254 - 1], "G93 G1 X-0.0005 Y0 Z0.3399 A8.194 F7.347")
        self.assertIn("Line 254, `G93 G1 X-0.0005 Y0 Z0.3399 A8.194 F7.347`", text)
        self.assertEqual(next(i for i, l in enumerate(lines, 1) if "G93" in l.split()), 254,
                         "line 254 is the first G93 line")
        self.assertEqual(lines[2685 - 1], "G1 A180 F360")
        self.assertEqual(lines[2682 - 1:2687], ["G1 X0.0625 Y0 Z-0.0322 A183.75 F154.109", "G94",
                                                "G1 A182.334 F360", "G1 A180 F360",
                                                "G1 A178.581 F360", "G1 A176.25 F360"])
        self.assertIn("At line 2685 the A position is 180", text)
        words = a_words()
        self.assertEqual((min(words), max(words)), (-85.708, 265.687))
        self.assertEqual(text.count("between -85.708 and 265.687"), 3)
        self.assertEqual(words[0], 44.604)
        self.assertEqual(words[-1], 265.686)
        self.assertIn("A at its last angle, 265.686", text)
        # The end-face facts (#2343): the free end face is cut to the
        # centreline; the chuck-side end face is left.
        def word(line, letter):
            return next((float(w[1:]) for w in line.split() if w[0] == letter), None)
        feeds = [l for l in lines if "G1" in l.split() and word(l, "X") is not None]
        deep = [l for l in feeds if " X0.0625 " in l and word(l, "Z") < 0]
        self.assertTrue(deep, "the program feeds below Z0 at X0.0625")
        self.assertIn("feed moves at X0.0625 with Z below 0", text)
        self.assertEqual(min(word(l, "X") for l in feeds), -1.25)
        self.assertIn("The program has no feed move past X-1.25", text)
        at_end = [word(l, "Z") for l in feeds if word(l, "X") == -1.25 and word(l, "Z") is not None]
        self.assertGreater(min(at_end), 0.339, "at X-1.25 the tool stays at the radius of the model")
        notes = entities("operation")["Journal"]["cache"]["notes"]
        # The held end's note and, since #2970, the full-turn note.
        self.assertEqual(len(notes), 2)
        for note in notes:
            self.assertIn(note, page())
        # The two timings the page works out.
        self.assertAlmostEqual(60 / 7.347, 8.2, places=1)
        self.assertAlmostEqual((186.086 - 183.75) * 154.109, 360.0, delta=0.5)

    def test_g93_and_g94_are_as_the_page_says(self):
        self.assertIn("The path length bound includes the curved trajectory relative to the part "
                      "between the rounded XYZ and A words.", flat(page()))
        g93 = False
        for line in program():
            words = line.split()
            if "G93" in words:
                g93 = True
            if "G94" in words:
                g93 = False
            feed = words[:1] == ["G1"] or words[1:2] == ["G1"]
            turns = any(w[0] == "A" for w in words)
            moves = any(w[0] in "XYZ" for w in words[1:])
            if feed and not line.startswith("("):
                self.assertEqual(g93, turns and moves, line)
            if words[:1] in (["G0"], ["M5"], ["M30"]):
                self.assertFalse(g93, line)

    def test_quoted_refusals_and_notes_are_in_their_sources(self):
        text = flat(page())
        for quote, source in QUOTED.items():
            with self.subTest(quote=quote[:40]):
                self.assertIn(quote, text)
                self.assertIn(quote, source.read_text())

    def test_pictures_are_drawn_from_the_example(self):
        spec = importlib.util.spec_from_file_location("pictures", TOOL)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        drawn = module.render()
        self.assertEqual(sorted(drawn), sorted(p.name for p in PICTURES.glob("*.svg")))
        for name, text in drawn.items():
            with self.subTest(picture=name):
                self.assertEqual((PICTURES / name).read_text(), text,
                                 f"{name} is stale; run python3 {TOOL.relative_to(ROOT)}")
                found = re.search(rf"!\[([^\]]+)\]\(\.\./\.\./\.\./guide-pictures/{re.escape(name)}\)", page())
                self.assertIsNotNone(found, f"the page does not show {name}")
                self.assertGreater(len(found.group(1).split()), 8, "describe the picture")
        self.assertFalse(list(PICTURES.glob("*.png")), "pictures are drawn, not screenshots")

    def test_page_says_air_cut_first_and_recommends_no_feeds(self):
        text = flat(page())
        self.assertIn("**Air-cut this program before you cut a bar with it.**", text)
        self.assertNotIn("air cut only", text)
        self.assertIn("CamCraft knows only the stock that you set.", text)
        self.assertIn("issues/2343", text)
        self.assertIn("issues/2344", text)
        self.assertIn("issues/796", text)
        self.assertIn("stop: do not put a bar in the chuck for this program.", text)
        self.assertIn("They are not a recommendation for your machine, tool or material.", text)
        self.assertIn("No complete program has been run in the simulator or on a machine yet.", text)
        for reading in ("Hub reading 1", "Hub reading 2", "Hub reading 3", "Hub reading 4"):
            self.assertIn(reading, text)

    def test_cli_command_and_companion_files_exist(self):
        text = page()
        self.assertIn("camcraft-cli process examples/first-continuous-4axis-part.camproj "
                      "-o first-continuous-4axis-part.nc", text)
        for path in (PROJECT, PROGRAM, MODEL):
            with self.subTest(path=path.name):
                self.assertTrue(path.is_file())
                self.assertIn(f"`{path.relative_to(ROOT).as_posix()}`", text)

    def test_page_is_linked_from_getting_started_and_multi_axis(self):
        link = "@/guide/getting-started/first-continuous-4axis-part.md"
        for path in ("getting-started/_index.md", "getting-started/first-run.md",
                     "getting-started/first-4axis-part.md", "multi-axis/_index.md",
                     "multi-axis/continuous-4-axis.md"):
            with self.subTest(page=path):
                self.assertIn(link, (GUIDE / path).read_text())
        continuous = flat((GUIDE / "multi-axis/continuous-4-axis.md").read_text())
        self.assertNotIn("CamCraft has no inverse-time feed", continuous)
        self.assertNotIn("indexes A only", (GUIDE / "multi-axis/_index.md").read_text())


if __name__ == "__main__":
    unittest.main()
