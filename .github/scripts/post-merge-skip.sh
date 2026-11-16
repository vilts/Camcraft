#!/usr/bin/env bash
# post-merge.yml's yield rule (#1451): exit 0 and print the reason when the
# post-merge batch gate should step aside for a cut, exit 1 (silent) when it
# should run. Uses only `gh` read calls (actions: read, pull-requests: read).
# Provenance: our own AGENTS.md "The cut runner"; GitHub's public gh CLI docs.
# GH overrides the gh binary (tests use a fake).
set -uo pipefail
gh_bin="${GH:-gh}"
repo="${REPO:?REPO is required}"

running=$("$gh_bin" run list -R "$repo" --workflow cut.yml --status in_progress --limit 5 \
  --json databaseId --jq '.[0].databaseId // empty') || running=""
if [ -z "$running" ]; then
  running=$("$gh_bin" run list -R "$repo" --workflow cut.yml --status queued --limit 5 \
    --json databaseId --jq '.[0].databaseId // empty') || running=""
fi
if [ -n "$running" ]; then
  echo "a cut.yml run ($running) is using the runner"
  exit 0
fi

# An open release PR from this repo's own release/vX.Y.Z branch: a cut is
# between "release branch" and "tag", and its gate or tag job is about to
# need the runner. A fork's PR branch of the same name does not count.
rel=$("$gh_bin" pr list -R "$repo" --state open --limit 100 \
  --json headRefName,isCrossRepository \
  --jq '[.[] | select((.isCrossRepository | not) and (.headRefName | test("^release/v")))][0].headRefName // empty') || rel=""
if [ -n "$rel" ]; then
  echo "release PR branch $rel is open (a cut is in progress)"
  exit 0
fi
exit 1
