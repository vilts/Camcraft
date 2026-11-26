"""Exercise package-macos.sh with local files and fake macOS tools."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "package-macos.sh"
IDENTITY = "A" * 40
SETTINGS = {
    "CAMCRAFT_MACOS_CERT_P12_BASE64": "Zml4dHVyZS1jZXJ0",
    "CAMCRAFT_MACOS_CERT_PASSWORD": "fixture certificate password",
    "CAMCRAFT_MACOS_NOTARY_KEY_BASE64": "Zml4dHVyZS1rZXk=",
    "CAMCRAFT_MACOS_NOTARY_KEY_ID": "FIXTUREKEY",
    "CAMCRAFT_MACOS_NOTARY_ISSUER_ID": "fixture-issuer",
}
OUTPUT = "camcraft-1.2.3-macos-universal"
MAC_ENV_PREFIX = "CAMCRAFT_MACOS_"

# The archive/image stubs record whether the source app had a ticket when copied.
# No stub contacts Apple or uses a real keychain, identity or certificate.
FAKE_TOOL = r"""
import base64
import json
import os
from pathlib import Path
import shutil
import sys

tool = Path(sys.argv[0]).name
args = sys.argv[1:]
target = Path(args[-1]) if args else None
mode = os.environ.get("FAKE_FAILURE")
with open(os.environ["TOOL_LOG"], "a") as log:
    log.write(json.dumps([tool, *args]) + "\n")

def option(name):
    return args[args.index(name) + 1]

if tool == "uname":
    print("Darwin")
elif tool == "cargo":
    print(json.dumps({"packages": [{"name": "camcraft-ui", "version": "1.2.3"}]}))
elif tool == "lipo" and "-create" in args:
    target.write_bytes(Path(args[1]).read_bytes())
elif tool in ("sips", "iconutil"):
    target.touch()
elif tool == "hdiutil":
    source = Path(option("-srcfolder"))
    assert (source / "Applications").readlink() == Path("/Applications")
    if mode == "image":
        sys.exit(1)
    app_ticket = source / "CamCraft.app/ticket"
    target.write_text("stapled app" if app_ticket.is_file() else "unstapled app")
elif tool == "ditto":
    source = Path(args[-2])
    if "--keepParent" in args:
        target.write_text("stapled app" if (source / "ticket").is_file() else "unstapled app")
    else:
        shutil.copytree(source, target)
elif tool == "base64":
    assert args == ["-D"]
    sys.stdout.buffer.write(base64.b64decode(sys.stdin.buffer.read(), validate=True))
elif tool == "openssl":
    print("fixture-keychain-password")
elif tool == "security":
    # A successful signed run proves the configured secrets were removed
    # from the child environment before a security command was started.
    assert all(not key.startswith("CAMCRAFT_MACOS_") for key in os.environ)
    if args[0] == "create-keychain":
        target.touch()
    elif args[0] == "import":
        assert Path(args[1]).read_bytes() == b"fixture-cert"
        assert option("-P") == "fixture certificate password"
        if mode == "import":
            sys.exit(1)
    elif args[0] == "find-identity":
        print('  1) ' + "A" * 40 + ' "Developer ID Application: Fixture (TEAM)"')
    elif args[0] == "delete-keychain":
        assert target.is_file()
        target.unlink()
elif tool == "codesign" and "--sign" in args:
    if option("--sign") != "-":
        assert Path(option("--keychain")).is_file()
        if mode == "sign" and target.suffix == ".dmg":
            sys.exit(1)
elif tool == "xcrun":
    assert all(not key.startswith("CAMCRAFT_MACOS_") for key in os.environ)
    if args[0] == "notarytool":
        archive = Path(args[2])
        assert args[1] == "submit"
        assert "--wait" in args
        assert option("--output-format") == "json"
        assert option("--key-id") == "FIXTUREKEY"
        assert option("--issuer") == "fixture-issuer"
        key = Path(option("--key"))
        assert key.read_bytes() == b"fixture-key"
        assert not list(key.parent.glob("*.p12"))
        if archive.suffix == ".zip":
            assert archive.read_text() == "unstapled app"
        status = "Invalid" if mode == "reject" else "Accepted"
        print("unexpected output" if mode == "json" else json.dumps({
            "status": status, "id": "fixture-submission",
        }))
        if mode == "submit" and archive.suffix == ".dmg":
            sys.exit(1)
    elif args[:2] == ["stapler", "staple"]:
        assert target.suffix in (".app", ".dmg")
        if target.suffix == ".app":
            (target / "ticket").touch()
        else:
            with target.open("a") as image:
                image.write("; stapled image")
    elif args[:2] == ["stapler", "validate"]:
        if target.suffix == ".app":
            assert (target / "ticket").is_file()
        elif mode == "validate":
            sys.exit(1)
        else:
            assert target.read_text() == "stapled app; stapled image"
"""


class MacPackageTest(unittest.TestCase):
    def package(self, settings=None, failure="", trace=False):
        # Each execution gets independent output and signing directories, including
        # test cases that run the packager several times with different failures.
        self.temp = tempfile.TemporaryDirectory(prefix="camcraft-macos-test-")
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name).resolve()
        tools = root / "bin"
        tools.mkdir()
        stub = tools / "fixture-tool"
        stub.write_text(f"#!{sys.executable}\n" + FAKE_TOOL)
        stub.chmod(0o755)
        for tool in (
            "uname", "cargo", "lipo", "sips", "iconutil", "codesign", "plutil",
            "ditto", "hdiutil", "base64", "openssl", "security", "xcrun",
        ):
            (tools / tool).symlink_to(stub)
        # The packager uses python3 for cargo metadata and notary responses.
        (tools / "python3").symlink_to(sys.executable)
        script = root / "scripts" / SCRIPT.name
        script.parent.mkdir()
        shutil.copyfile(SCRIPT, script)
        for arch in ("aarch64", "x86_64"):
            target = root / "target" / f"{arch}-apple-darwin" / "release"
            target.mkdir(parents=True)
            for name in ("camcraft-ui", "camcraft-cli", "post"):
                executable = target / name
                executable.write_text("fixture executable")
                executable.chmod(0o755)
        for folder in ("posts", "machines", "ui", "scripts", "themes", "logo/icons"):
            (root / "assets" / folder).mkdir(parents=True)
        (root / "assets/logo/icons/camcraft-512.png").touch()
        (root / "examples").mkdir()
        (root / "examples/first-part.camproj").touch()
        for name in ("RELEASE_NOTES.md", "TESTING.md", "LICENSE", "NOTICE", "README.md"):
            (root / name).touch()

        self.dist = root / "output with spaces"
        self.signing_temp = root / "signing with spaces"
        self.signing_temp.mkdir()
        log = root / "tools.jsonl"
        env = {key: value for key, value in os.environ.items() if not key.startswith(MAC_ENV_PREFIX)}
        env.pop("BASH_ENV", None)
        env.update({
            "PATH": f"{tools}{os.pathsep}{os.defpath}",
            "CAMCRAFT_DIST_DIR": str(self.dist),
            "RUNNER_TEMP": str(self.signing_temp),
            "TOOL_LOG": str(log),
            "FAKE_FAILURE": failure,
            **(settings or {}),
        })
        cmd = ["bash", "-x", str(script)] if trace else ["bash", str(script)]
        process = subprocess.run(cmd, cwd=root, env=env, capture_output=True, text=True, timeout=30)
        self.calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
        self.assertEqual(list(self.signing_temp.iterdir()), [])
        if self.dist.exists():
            self.assertEqual(list(self.dist.glob(".package-macos.*")), [])
        return process

    def packages(self):
        return sorted(p.name for p in self.dist.iterdir()) if self.dist.exists() else []

    def test_unsigned_packages_without_apple_credentials(self):
        result = self.package({key: "" for key in SETTINGS})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.packages(), [f"{OUTPUT}.dmg", f"{OUTPUT}.zip"])
        self.assertFalse(any(tool in ("security", "xcrun") for tool, *_ in self.calls))
        # Apple lipo versions may fail when both architectures share one check.
        checks = [(Path(args[0]).name, args[2:]) for tool, *args in self.calls
                  if tool == "lipo" and "-verify_arch" in args]
        self.assertEqual(checks, [
            (binary, [arch])
            for binary in ("camcraft-ui", "camcraft-cli", "post")
            for arch in ("arm64", "x86_64")
        ])
        signatures = [call for call in self.calls if call[0] == "codesign" and "--sign" in call]
        self.assertEqual([Path(call[-1]).name for call in signatures], ["camcraft-cli", "post", "CamCraft.app"])
        for call in signatures:
            self.assertEqual(call[call.index("--sign") + 1], "-")
            self.assertIn("--timestamp=none", call)
        self.assertEqual((self.dist / f"{OUTPUT}.zip").read_text(), "unstapled app")
        self.assertEqual((self.dist / f"{OUTPUT}.dmg").read_text(), "unstapled app")

    def test_partial_signing_settings_fail_before_packaging(self):
        result = self.package({"CAMCRAFT_MACOS_NOTARY_KEY_ID": SETTINGS["CAMCRAFT_MACOS_NOTARY_KEY_ID"]})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("incomplete signing configuration", result.stderr)
        self.assertIn("CAMCRAFT_MACOS_CERT_P12_BASE64", result.stderr)
        self.assertEqual([call[0] for call in self.calls], ["uname"])
        self.assertEqual(self.packages(), [])

    def test_signed_zip_and_dmg_include_validated_tickets(self):
        result = self.package(SETTINGS, trace=True)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(self.packages(), [f"{OUTPUT}.dmg", f"{OUTPUT}.zip"])
        self.assertEqual((self.dist / f"{OUTPUT}.zip").read_text(), "stapled app")
        self.assertEqual((self.dist / f"{OUTPUT}.dmg").read_text(), "stapled app; stapled image")
        for secret in SETTINGS.values():
            self.assertNotIn(secret, result.stdout + result.stderr)

        steps = []
        for tool, *args in self.calls:
            target = Path(args[-1]).name if args else ""
            if tool == "codesign":
                if "--sign" in args:
                    self.assertEqual(args[args.index("--sign") + 1], IDENTITY)
                    self.assertIn("--timestamp", args)
                    self.assertNotIn("--deep", args)
                    if target.endswith(".dmg"):
                        self.assertEqual(args[args.index("--identifier") + 1], "org.camcraft.dmg")
                    else:
                        self.assertEqual(args[args.index("--options") + 1], "runtime")
                    steps.append(("sign", target))
                elif "--verify" in args:
                    self.assertIn("--strict", args)
                    if target == "CamCraft.app":
                        self.assertIn("--deep", args)
                    steps.append(("verify", target))
            elif tool == "ditto" and "--keepParent" in args:
                steps.append(("archive", target))
            elif tool == "hdiutil":
                steps.append(("image", target))
            elif tool == "xcrun" and args[0] == "notarytool":
                steps.append(("submit", Path(args[2]).name))
            elif tool == "xcrun" and args[0] == "stapler":
                steps.append((args[1], target))
            elif tool == "security" and args[0] == "delete-keychain":
                steps.append(("delete", target))
        self.assertEqual(steps, [
            ("sign", "camcraft-cli"),
            ("sign", "post"),
            ("sign", "CamCraft.app"),
            ("verify", "CamCraft.app"),
            ("archive", "CamCraft-notary.zip"),
            ("submit", "CamCraft-notary.zip"),
            ("staple", "CamCraft.app"),
            ("validate", "CamCraft.app"),
            ("archive", f"{OUTPUT}.zip"),
            ("image", f"{OUTPUT}.dmg"),
            ("sign", f"{OUTPUT}.dmg"),
            ("verify", f"{OUTPUT}.dmg"),
            ("submit", f"{OUTPUT}.dmg"),
            ("staple", f"{OUTPUT}.dmg"),
            ("validate", f"{OUTPUT}.dmg"),
            ("delete", "camcraft.keychain-db"),
        ])

    def test_failed_signed_package_cleans_up_and_publishes_nothing(self):
        for failure in ("import", "sign", "reject", "json", "submit", "validate"):
            with self.subTest(failure=failure):
                result = self.package(SETTINGS, failure)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.packages(), [])
                self.assertEqual(self.calls[-1][:2], ["security", "delete-keychain"])
                if failure in ("reject", "json", "submit"):
                    self.assertIn("notarization did not finish successfully", result.stderr)
                    self.assertNotEqual(self.calls[-2][:2], ["xcrun", "stapler"])

    def test_failed_unsigned_image_publishes_nothing(self):
        result = self.package(failure="image")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.packages(), [])


if __name__ == "__main__":
    unittest.main()
