#!/usr/bin/env bash
# post-merge.yml's yield rule (#1451): exit 0 and print the reason when the
# post-merge batch gate should step aside for a cut, exit 1 (silent) when it
# should run. Uses only `gh` read calls (actions: read, pull-requests: read).
# Provenance: our own AGENTS.md "The cut runner"; GitHub's public gh CLI docs.
# GH overrides the gh binary (tests use a fake).
# Docs-only rule (#4054): on a push, a commit whose diff against its first
# parent touches only docs or process paths changes no code, so the run of the
# commit before it covers this code. EVENT_NAME=push turns the rule on; a
# workflow_dispatch (any other value, or none) always runs. A commit with no
# readable parent (a shallow clone of one commit) runs.
set -uo pipefail
gh_bin="${GH:-gh}"
repo="${REPO:?REPO is required}"

# Exit 0 when every changed file is a docs or process path. Code, Cargo.*,
# xtask/, assets/, .github/, build.rs and scripts/ are never docs. A *.md file
# inside crates/ is code too: cli_skill.md is compiled in with include_str!.
# Docs consumed by tests are gate inputs too, even when no Rust file changes.
docs_only() {
  local files f
  files=$(git diff --name-only --no-renames HEAD^ HEAD 2>/dev/null) || return 1
  [ -n "$files" ] || return 1
  while IFS= read -r f; do
    case "$f" in
      crates/* | xtask/* | assets/* | .github/* | scripts/* | Cargo.* | build.rs | */build.rs) return 1 ;;
      README.md | GEMINI.md | .agents/* | .claude/skills/camcraft-mcp/* | docs/agents/* | docs/tester-kits/* | site/content/* | site/tests/* | examples/*) return 1 ;;
      site/* | docs/* | .claude/* | *.md) ;;
      *) return 1 ;;
    esac
  done <<<"$files"
  return 0
}

if [ "${EVENT_NAME:-}" = push ] && docs_only; then
  echo "no code changed - the previous run covers this code"
  exit 0
fi

running=$("$gh_bin" run list -R "$repo" --workflow cut.yml --status in_progress --limit 5 \
  --json databaseId --jq '.[0].databaseId // empty') || running=""
if [ -z "$running" ]; then
  running=$("$gh_bin" run list -R "$repo" --workflow cut.yml --status queued --limit 5 \
    --json databaseId --jq '.[0].databaseId // empty') || running=""
fi
if [ -n "$running" ]; then
  echo "a cut.yml run ($running) is using the runner; the cut's release gate covers main"
  exit 0
fi

# An open release PR from this repo's own release/vX.Y.Z branch: a cut is
# between "release branch" and "tag", and its gate or tag job is about to
# need the runner. A fork's PR branch of the same name does not count.
rel=$("$gh_bin" pr list -R "$repo" --state open --limit 100 \
  --json headRefName,isCrossRepository \
  --jq '[.[] | select((.isCrossRepository | not) and (.headRefName | test("^release/v")))][0].headRefName // empty') || rel=""
if [ -n "$rel" ]; then
  echo "release PR branch $rel is open (a cut is in progress); the cut's release gate covers main"
  exit 0
fi
exit 1
