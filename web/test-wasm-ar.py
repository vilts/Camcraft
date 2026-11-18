#!/usr/bin/env python3
"""Exercise archiver selection before the wasm build downloads or compiles."""
# Provenance: regression cases for #1569, using isolated fake toolchains.
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class WasmArchiver(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="wasm-ar-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        (self.bin / "dirname").symlink_to("/usr/bin/dirname")
        (self.bin / "sed").symlink_to("/usr/bin/sed")
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith("AR")}
        self.env.update(PATH=str(self.bin), TEST_OS="Darwin")
        self.tool("uname", 'printf "%s\\n" "$TEST_OS"')
        source = Path(__file__).with_name("build-app-wasm.sh").read_text()
        self.script = self.root / "selection.sh"
        self.script.write_text(source.split('\nTOOLS=', 1)[0] +
                               '\n/bin/bash -c \'printf "SELECTED=%s\\n" '
                               '"${AR_wasm32_unknown_unknown-}"\'\n')

    def tool(self, name, body, directory=None):
        path = (directory or self.bin) / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("#!/bin/bash\n" + body + "\n")
        path.chmod(0o755)
        return str(path)

    def run_selection(self):
        return subprocess.run(["/bin/bash", str(self.script)], env=self.env,
                              text=True, capture_output=True, check=False)

    def selected(self, expected):
        result = self.run_selection()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("SELECTED=" + expected + "\n", result.stdout)

    def test_macos_selects_path_llvm_ar(self):
        ar = self.tool("llvm-ar", "exit 0")
        self.selected(ar)

    def test_macos_selects_homebrew_llvm_ar_with_spaces(self):
        prefix = self.root / "Homebrew LLVM"
        ar = self.tool("llvm-ar", "exit 0", prefix / "bin")
        self.env["TEST_BREW_PREFIX"] = str(prefix)
        self.tool("brew", '[[ "$*" == "--prefix llvm" ]] || exit 1\n'
                          'printf "%s\\n" "$TEST_BREW_PREFIX"')
        self.selected(ar)

    def test_macos_selects_rustup_llvm_tools(self):
        sysroot = self.root / "rust toolchain"
        host = "aarch64-apple-darwin"
        ar = self.tool("llvm-ar", "exit 0",
                       sysroot / "lib" / "rustlib" / host / "bin")
        self.env["TEST_SYSROOT"] = str(sysroot)
        self.tool("rustc", 'case "$*" in\n'
                          '  "--print sysroot") printf "%s\\n" "$TEST_SYSROOT" ;;\n'
                          '  "-vV") printf "host: aarch64-apple-darwin\\n" ;;\n'
                          '  *) exit 1 ;;\nesac')
        self.selected(ar)

    def test_preserves_explicit_target_archiver(self):
        for platform in ("Darwin", "Linux"):
            with self.subTest(platform=platform):
                self.env.update(TEST_OS=platform,
                                AR_wasm32_unknown_unknown="/custom tools/ar")
                self.selected("/custom tools/ar")

    def test_linux_does_not_choose_llvm_ar(self):
        self.env["TEST_OS"] = "Linux"
        self.tool("llvm-ar", "exit 0")
        self.selected("")

    def test_preserves_explicit_empty_target_archiver(self):
        self.env["AR_wasm32_unknown_unknown"] = ""
        self.tool("llvm-ar", "exit 0")
        self.selected("")

    def test_macos_refuses_without_llvm_ar(self):
        result = self.run_selection()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("llvm-ar", result.stderr)
        self.assertIn("brew install llvm", result.stderr)
        self.assertIn("rustup component add llvm-tools", result.stderr)


if __name__ == "__main__":
    unittest.main()
