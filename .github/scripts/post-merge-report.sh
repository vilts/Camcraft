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
# Order (#3184): each push to main has its own run, so runs can finish out of
# order. The rolling issue body keeps the last reported commit in a hidden
# marker (MARK below). A run whose commit is an ancestor of that commit, and
# not the same commit, reports nothing: an older green run never closes the
# issue of a newer red run. This needs the full history (fetch-depth: 0).
# When git cannot decide (an unknown sha), the run reports as before.
# Runner stall (#3303): a red run with no FAIL row, but with a STALL row (the
# xtask watchdog killed a silent step) or the "gate step TIMEOUT" line (the
# workflow's step timeout stopped the gate), is a runner stop, not a red main.
# It opens and comments on no issue: it writes one warning that names the step
# and the runner (env RUNNER), and exits 0. The coordinator logs it on the
# runner-stop issue and re-runs the run one time (AGENTS.md "Runner stops").
# Provenance: our own AGENTS.md "Issue Law"; GitHub's public gh CLI docs.
set -uo pipefail
gh_bin="${GH:-gh}"
repo="${REPO:?REPO is required}"
sha="${SHA:?SHA is required}"
title="main is red (post-merge gate)"
short="${sha:0:10}"

if [ "${STATUS:-}" != success ]; then
  gate_out="${OUTPUT:-/dev/null}"
  summary=$(sed -n '/^== gate summary/,$p' "$gate_out" 2>/dev/null || true)
  # A FAIL anywhere is a red main: a FAIL row of the summary, or a "FAIL in"
  # line that the gate prints as each step ends. A later hang can stop the
  # gate before its summary, so the progress lines count too (#3303). No
  # grep -q in a pipe here: under pipefail its early exit can give the writer
  # SIGPIPE, and a match then reads as a miss.
  red_rows=$(grep -E '^ +FAIL ' <<<"$summary" || true)
  red_progress=$(grep -E '^ +FAIL in ' "$gate_out" 2>/dev/null || true)
  if [ -z "$red_rows" ] && [ -z "$red_progress" ]; then
    stalls=$(grep -E '^ +STALL ' <<<"$summary" || true)
    [ -n "$stalls" ] || stalls=$(grep -E '^gate step TIMEOUT ' "$gate_out" 2>/dev/null || true)
    if [ -n "$stalls" ]; then
      stalls=$(head -n 10 <<<"$stalls")
      step=$(sed -E -e 's/^ +STALL +[0-9]+s +//' -e 's/^gate step TIMEOUT .*last started step: //' -e 's/  .*//' <<<"$stalls" | paste -sd ';' -)
      echo "::warning title=runner stall::runner stall: $step on ${RUNNER:-unknown runner} at $short (${RUN_URL:-no run url}); not a red main, no issue opened"
      printf 'runner stall: %s on %s\n%s\n' "$step" "${RUNNER:-unknown runner}" "$stalls"
      exit 0
    fi
  fi
fi

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
# The newest rolling issue, open or closed: it holds the order marker.
latest=$("$gh_bin" issue list -R "$repo" --state all --label source:agent --search "\"$title\" in:title" \
  --limit 20 --json number,title \
  --jq "[.[] | select(.title == \"$title\")][0].number // empty") || latest=""
[ -n "$latest" ] || latest="$open"

MARK="post-merge-last-sha"
marker="<!-- $MARK: $sha -->"

# The body of issue $1, or nothing.
issue_body() { "$gh_bin" issue view "$1" -R "$repo" --json body --jq .body 2>/dev/null || true; }

# Write this run's commit into the marker of issue $1. The old marker line is
# replaced, never kept: the body holds one marker.
set_marker() {
  local kept
  kept=$(issue_body "$1" | sed "/<!-- $MARK: /d")
  "$gh_bin" issue edit "$1" -R "$repo" --body "$kept

$marker" || exit 1
}

# Skip a run whose commit is strictly older than the last reported commit.
if [ -n "$latest" ]; then
  last=$(issue_body "$latest" | sed -n "s/.*<!-- $MARK: \([0-9a-f]\{7,40\}\) -->.*/\1/p" | tail -n 1)
  if [ -n "$last" ]; then
    this_full=$(git rev-parse --verify --quiet "$sha^{commit}" 2>/dev/null || true)
    last_full=$(git rev-parse --verify --quiet "$last^{commit}" 2>/dev/null || true)
    if [ -n "$this_full" ] && [ -n "$last_full" ] && [ "$this_full" != "$last_full" ] &&
      git merge-base --is-ancestor "$this_full" "$last_full" 2>/dev/null; then
      echo "::notice title=post-merge report skipped::$short is older than the last reported commit ${last:0:10} (#$latest); nothing reported"
      exit 0
    fi
  fi
fi

if [ "${STATUS:-}" = success ]; then
  if [ -n "$open" ]; then
    "$gh_bin" issue comment "$open" -R "$repo" --body "Green again: the post-merge batch gate passed on \`$sha\` (${RUN_URL:-no run url}). Closing; a new red run opens a new issue." || exit 1
    set_marker "$open"
    "$gh_bin" issue close "$open" -R "$repo" --reason completed || exit 1
    echo "closed #$open"
  else
    # Record the green commit on the last (closed) issue, so an older red run
    # that finishes later does not open a new issue.
    [ -z "$latest" ] || set_marker "$latest"
    echo "green, no open red issue"
  fi
  exit 0
fi

# Failing steps: the gate summary's "  FAIL  <secs>s  <step>  <note>" rows.
# Without a summary, the "FAIL in" lines of the steps that ended red.
fails=$(head -n 40 <<<"$red_rows")
[ -n "$fails" ] || fails=$(head -n 40 <<<"$red_progress")
[ -n "$fails" ] || fails="(no FAIL row in the gate summary: the gate did not finish or failed to build; see the run)"
# The FAIL rows' notes read "failed: tests/<name>.rs, ..." (xtask exec.rs read_log,
# which prints cargo's "Running <path>"); a directory suite reads tests/<name>/main.rs.
suites=$(printf '%s\n' "$fails" | grep -oE 'tests/[A-Za-z0-9_]+(/main)?\.rs' | sed -e 's#^tests/##' -e 's#\.rs$##' -e 's#/main$##' | sort -u || true)
out_hit=$(printf '%s\n' "$suites" | grep -E "$OUTPUT_SUITES" || true)

body=$(printf 'The post-merge batch gate failed on `%s`.\n\nRun: %s\n\nFailing steps:\n```\n%s\n```\n\nEach push to main has its own run, so this run gates exactly this commit. If the run of the commit before it is green, this commit broke main. Fix it in the normal order; this issue closes itself on the next green run.\n\n%s\n' \
  "$sha" "${RUN_URL:-no run url}" "$fails" "$marker")

if [ -n "$open" ]; then
  "$gh_bin" issue comment "$open" -R "$repo" --body "Still red on \`$short\` (${RUN_URL:-no run url}).

Failing steps:
\`\`\`
$fails
\`\`\`" || exit 1
  set_marker "$open"
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
