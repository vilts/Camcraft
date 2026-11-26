"""Provenance: released CamCraft 0.26.0 assets and documentation gaps in #3769."""

from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[2]
GUIDE = ROOT / "site/content/guide"

# Freeze the release inventory: the same guide changes also go to main.
POSTS_026 = (
    "agie-native", "carvera", "doosan-dnm", "fanuc", "fanuc-lathe",
    "g-edm", "grbl", "grblhal", "haas", "heidenhain",
    "heidenhain-klartext", "linuxcnc", "mach3", "mach4", "mazak",
    "richauto", "sinker-iso", "sinumerik", "sinumerik-810m", "sodick-wire",
    "tinyg", "tormach", "tormach-lathe", "tormach-plasma", "waterjet-iso",
    "wire-iso",
)
MACHINES_026 = (
    "3axis-vmc", "agiecut-classic-2s", "doosan-dnm", "doosan-dnm-3axis",
    "emco-vmc-200", "fehlmann-picomax55", "grbl-3axis", "grblhal-3axis",
    "haas-dm1", "haas-dt1", "haas-mini-mill", "haas-vf2", "haas-vf2ss",
    "lathe", "linuxcnc-3axis", "makera-carvera", "makera-carvera-air",
    "makera-z1", "millturn", "ooznest-workbee-1000x1500", "sienci-altmill-2x4",
    "sienci-altmill-4x4", "sienci-altmill-4x8", "sinker-edm", "sodick-alc400g",
    "tilthead", "tinyg-3axis", "tormach-1100m", "tormach-1100m-plus",
    "tormach-1100mx", "tormach-1300pl", "tormach-1500mx",
    "tormach-15l-slant-pro", "tormach-15lx-slant-pro", "tormach-24r",
    "tormach-770m", "tormach-770m-plus", "tormach-770mx",
    "tormach-770mx-microarc4", "tormach-770mx-ml", "tormach-8l",
    "tormach-pcnc1100-s1", "tormach-pcnc1100-s2", "tormach-pcnc1100-s3",
    "tormach-pcnc440", "tormach-pcnc770", "tormach-pcnc770-s3",
    "tormach-xstech", "tormach-xstech-pro", "vmc",
)


def page(path):
    return " ".join((GUIDE / path).read_text().lower().split())


def section(path, heading):
    source = (GUIDE / path).read_text()
    parts = re.split(r"^## (.+)$", source, flags=re.M)
    sections = dict(zip(parts[1::2], parts[2::2]))
    return " ".join(sections[heading].lower().split())


class Release026Gaps(unittest.TestCase):
    def test_post_inventory_names_each_shipped_script_and_control(self):
        source = (GUIDE / "check-and-post/posting.md").read_text()
        rows = [line.lower() for line in source.splitlines() if line.startswith("|")]
        for stem in POSTS_026:
            with self.subTest(post=stem):
                matches = [row for row in rows if f"`{stem}.cps`" in row]
                self.assertEqual(len(matches), 1, f"Add one inventory row for {stem}.cps")
                self.assertGreaterEqual(len(matches[0].split("|")), 5)
        for stem in MACHINES_026:
            with self.subTest(machine=stem):
                self.assertIn(f"`{stem}.mch`", source)
        for filename, facts in (
            ("mach3.cps", ("mach3", "mill")),
            ("sinumerik-810m.cps", ("810m", "emco-vmc-200.mch")),
            ("agie-native.cps", ("agie", "agiecut-classic-2s.mch")),
        ):
            with self.subTest(post=filename):
                row = next((row for row in rows if f"`{filename}`" in row), "")
                for fact in facts:
                    self.assertIn(fact, row)

    def test_do_not_touch_is_documented_on_each_added_operation(self):
        for path, heading in (
            ("operations/operations-2d.md", "Face"),
            ("operations/operations-2d.md", "Trace"),
            ("operations/operations-2d.md", "Engrave"),
            ("operations/operations-2d.md", "2D Adaptive"),
            ("operations/operations-3d.md", "3D Finish"),
        ):
            with self.subTest(operation=heading):
                text = section(path, heading)
                self.assertIn("do not touch", text)
                self.assertRegex(text, r"(boundar|geometry)")
                self.assertRegex(text, r"(protect|keep.out|clearance)")

    def test_outline_text_explains_creation_and_profile_use(self):
        text = page("set-up/setups.md")
        for fact in ("insert_outline_text", "truetype", '"font"', '"height"',
                     "stock", "profile", "undo"):
            with self.subTest(fact=fact):
                self.assertIn(fact, text)
        self.assertRegex(text, r"(no|not|without).{0,80}(dialog|font picker)")
        self.assertRegex(text, r"(shapes|outlines).{0,100}(sav|stor)")
        self.assertIn("@/guide/automate/mcp.md", text)

    def test_offset_warns_about_released_inside_outside_paths(self):
        text = section("operations/operations-2d.md", "2D Offset")
        self.assertRegex(text, r"known.{0,15}issue")
        for fact in ("0.26.0", "inside", "outside", "ramp", "lift"):
            with self.subTest(fact=fact):
                self.assertIn(fact, text)
        for number in (3741, 3744, 3745, 3746, 3747, 3748):
            with self.subTest(issue=number):
                self.assertIn(f"https://github.com/NeoTech/Camcraft/issues/{number}".lower(), text)

    def test_upgrade_notes_cover_format_regeneration_and_unit_question(self):
        text = page("reference/migrating.md")
        self.assertIn("0.26.0", text)
        self.assertRegex(text, r"format.{0,30}78")
        self.assertRegex(text, r"older.{0,80}refus")
        self.assertIn("part-off", text)
        self.assertRegex(text, r"(regenerat|generate.{0,20}again)")
        self.assertIn("https://github.com/neotech/camcraft/issues/3452", text)
        for fact in ("step", "millimet", "inch", "@/guide/reference/units-and-coordinates.md"):
            with self.subTest(fact=fact):
                self.assertIn(fact, text)


if __name__ == "__main__":
    unittest.main()
