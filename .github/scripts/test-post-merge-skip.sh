#!/usr/bin/env bash
# Test of post-merge-skip.sh's docs-only rule (#4054): builds a throwaway repo,
# commits one change set per case, and checks skip (exit 0) or run (exit 1).
# Usage: bash .github/scripts/test-post-merge-skip.sh   (SCRIPT overrides the script)
# Provenance: our own AGENTS.md "The post-merge gate".
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
script="${SCRIPT:-$here/post-merge-skip.sh}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# A fake gh that finds no cut run and no release PR, so only the docs rule decides.
printf '#!/usr/bin/env bash\nexit 0\n' >"$tmp/gh"
chmod +x "$tmp/gh"

repo="$tmp/repo"
git init -q "$repo"
cd "$repo" || exit 1
git config user.email t@example.invalid
git config user.name t
git config commit.gpgsign false
echo base >base.txt
git add . && git commit -qm base

fail=0
# run_case <name> <event> <expect: skip|run> <file>...
run_case() {
  local name=$1 event=$2 expect=$3 f out rc got
  shift 3
  for f in "$@"; do
    mkdir -p "$(dirname "$f")"
    echo "$name" >>"$f"
  done
  git add -A && git commit -qm "$name"
  out=$(EVENT_NAME=$event GH="$tmp/gh" REPO=o/r bash "$script")
  rc=$?
  if [ "$rc" -eq 0 ]; then got=skip; else got=run; fi
  if [ "$got" = "$expect" ]; then
    printf 'ok   %-22s %-17s -> %s %s\n' "$name" "$event" "$got" "$out"
  else
    printf 'FAIL %-22s %-17s -> %s, want %s\n' "$name" "$event" "$got" "$expect"
    fail=1
  fi
}

run_case docs-only push skip docs/a.txt site/index.html CONTRIBUTING.md
run_case claude-only push skip .claude/skills/x/SKILL.md AGENTS.md
# Documents consumed by Rust tests and the catalog are gate inputs too.
# Keep each input in its own commit so another run path cannot hide a skip.
run_case readme-input push run README.md
run_case gemini-input push run GEMINI.md
run_case agents-mcp-input push run .agents/skills/camcraft-mcp/SKILL.md
run_case claude-mcp-input push run .claude/skills/camcraft-mcp/SKILL.md
run_case docs-mcp-input push run docs/agents/mcp-tools.md
run_case tester-kit-program push run docs/tester-kits/sinumerik-810m-vmc200/air_cut_vmc200.mpf
run_case tester-kit-project push run docs/tester-kits/sinumerik-810m-vmc200/air_cut_vmc200.camproj
run_case tester-kit-readme push run docs/tester-kits/sinumerik-810m-vmc200/README.md
run_case site-mcp-input push run site/content/guide/automate/mcp.md
run_case site-ai-input push run site/content/guide/automate/ai.md
run_case troubleshooting-input push run site/content/guide/reference/troubleshooting.md
run_case getting-started-input push run site/content/guide/getting-started/first-part.md
run_case machines-input push run site/content/guide/check-and-post/machines.md
run_case operations-input push run site/content/guide/operations/contour.md
run_case multi-axis-input push run site/content/guide/multi-axis/swarf.md
run_case site-test-input push run site/tests/test_guide_navigation.py
run_case catalog-readme-input push run examples/catalog/example/README.md
run_case mixed-docs-code push run docs/b.md crates/app/src/lib.rs
run_case github-change push run .github/workflows/post-merge.yml
run_case cargo-lock push run Cargo.lock
run_case xtask-md push run xtask/README.md
run_case crates-md push run crates/app/src/cli_skill.md
run_case assets push run assets/ui/icon.png
run_case scripts push run scripts/gate.sh
run_case build-rs push run crates/app/build.rs
run_case unknown-root-file push run Makefile
run_case docs-only-dispatch workflow_dispatch run docs/c.md
exit "$fail"
