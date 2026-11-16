#!/usr/bin/env bash
# post-merge.yml's reporting (#1451): one rolling issue "main is red
# (post-merge gate)". Red: open it, or comment on the open one, with the
# failing suites, the head sha and the run URL. Green: comment the green sha
# and close it. Never one issue per run.
# Inputs (env): STATUS (success|failure), SHA, RUN_URL, REPO, OUTPUT (the
# gate's stdout file), GH (gh binary override, tests use a fake).
# Priority (AGENTS.md "Priorities", P1..P9): a red gate on main is P2; it is
# P1 (wrong output) only when a failing suite is one of OUTPUT_SUITES below,
# the suites whose job is to compare posted NC or CL output with a golden or
# recorded stream. A compile error, a lathe/sim/motion law or a post-crate
# unit (lib) test stays P2. An open P2 issue is raised to P1 when a later run
# fails an output suite; it is never lowered here (a human does that).
# Provenance: our own AGENTS.md "Issue Law"; GitHub's public gh CLI docs.
set -uo pipefail
gh_bin="${GH:-gh}"
repo="${REPO:?REPO is required}"
sha="${SHA:?SHA is required}"
title="main is red (post-merge gate)"
short="${sha:0:10}"

# Whole test-binary names (the stem of tests/<name>.rs), matched anchored,
# never as substrings: "compost" or a "post suites" step note does not match.
# Taken from the batch tier's plan (cargo xtask gate batch --dry-run): app's
# golden CL/NC suites and posted-output suites, post's recorded-stream and
# per-dialect golden suites (script_<dialect>).
OUTPUT_SUITES='^(golden_cl|solid_law_goldens|recorded_post_streams|recorded_streams|posting|post_engine|post_cycle_feed_spindle|carvera_post|grblhal_post|lathe_post|lathe_post_css|lathe_post_g76|lathe_post_inch|lathe_post_tormach|lathe_post_tormach_thread|plasma_post|tinyg_post|tormach_post|wire_post|fanuc_post|doosan_dnm_post|inverted_post|script_[a-z0-9_]+)$'

# Only an agent-filed issue (source:agent) is ever commented on or closed here;
# a human-filed issue with the same title is left alone.
open=$("$gh_bin" issue list -R "$repo" --state open --label source:agent --search "\"$title\" in:title" \
  --limit 20 --json number,title \
  --jq "[.[] | select(.title == \"$title\")][0].number // empty") || open=""

if [ "${STATUS:-}" = success ]; then
  if [ -n "$open" ]; then
    "$gh_bin" issue comment "$open" -R "$repo" --body "Green again: the post-merge batch gate passed on \`$sha\` (${RUN_URL:-no run url}). Closing; a new red run opens a new issue." || exit 1
    "$gh_bin" issue close "$open" -R "$repo" --reason completed || exit 1
    echo "closed #$open"
  else
    echo "green, no open red issue"
  fi
  exit 0
fi

# Failing steps: the gate summary's "  FAIL  <secs>s  <step>  <note>" rows.
fails=$(sed -n '/^== gate summary/,$p' "${OUTPUT:-/dev/null}" 2>/dev/null | grep -E '^ +FAIL ' | head -40 || true)
[ -n "$fails" ] || fails="(no FAIL row in the gate summary: the gate did not finish or failed to build; see the run)"
# The FAIL rows' notes read "failed: tests/<name>.rs, ..." (xtask exec.rs read_log,
# which prints cargo's "Running <path>"); a directory suite reads tests/<name>/main.rs.
suites=$(printf '%s\n' "$fails" | grep -oE 'tests/[A-Za-z0-9_]+(/main)?\.rs' | sed -e 's#^tests/##' -e 's#\.rs$##' -e 's#/main$##' | sort -u || true)
out_hit=$(printf '%s\n' "$suites" | grep -E "$OUTPUT_SUITES" || true)

body=$(printf 'The post-merge batch gate failed on `%s`.\n\nRun: %s\n\nFailing steps:\n```\n%s\n```\n\nThe merge that broke it is among the commits up to this head (`git log` since the last green run). Fix it in the normal order; this issue closes itself on the next green run.\n' \
  "$sha" "${RUN_URL:-no run url}" "$fails")

if [ -n "$open" ]; then
  "$gh_bin" issue comment "$open" -R "$repo" --body "Still red on \`$short\` (${RUN_URL:-no run url}).

Failing steps:
\`\`\`
$fails
\`\`\`" || exit 1
  if [ -n "$out_hit" ]; then
    "$gh_bin" issue edit "$open" -R "$repo" --add-label P1 --remove-label P2 || exit 1
  fi
  echo "commented on #$open"
else
  prio=P2; [ -n "$out_hit" ] && prio=P1
  "$gh_bin" issue create -R "$repo" --title "$title" \
    --label "$prio" --label source:agent --label effort:low --label area:build \
    --body "$body" || exit 1
fi
