## #1189 - a P1-P9 priority scale replaces P0/P1/P2 (2026-11-12)

**Decided (maintainer, question dialog, 2026-11-12).** "We might need a P1-P9
prioritization system now.. we're having issues with scope creep throughout
the issue list" -> "P1 to P9 scale: replace P0/P1/P2 with nine levels;
agents keep filing and rating their own issues as today". Read as "P4 is our
normal; P1 to P3 is fix now", with **only `P1` and `P2` blocking a release
cut**; a `P3` is worked ahead of planned work but does not hold a cut back.

**Chosen.** The nine-level table now in AGENTS.md "Priorities" (lower
number first). Rules changed: Pick order, enhancement-lane order, safety
floor (every `P1` is at least `effort:high`), red-gate filing (`P1` wrong
output / gouge, else `P2`), cut blocker (any open `P1` or `P2` - one rule, same words in "Priorities" and "Fixes are tested in the background"; a `P3` never), MCP: owed agent-surface work `P5`, a defect in MCP code rated `P1`-`P3` by severity;
no priority = untriaged (triaged by the first agent that reads it, before any pick; the gh-issues Pick snippet lists such issues first); filing rules (`P1`-`P3` defects only; `P4`
only with the maintainer's own words quoted; a follow-up found on the way is
`P7` unless it is a defect; never `P?`). `P?` is unchanged.

**Considered and dropped.**
- Keep `P0`-`P2` and add a maintainer-set scope marker label: leaves three
  levels to order 185 issues, and a second label per issue to keep in sync.
- A scale where only the maintainer may set `P1`-`P4`: would stall agent
  filing on his attention; the table's "defects only" and "his own words"
  rules carry the same restraint without a gate.
- Keeping the cut blocker scoped to "a `P1`/`P2` it (the red gate) opened":
  dropped on review - the maintainer chose "only P1 and P2 block a release
  cut", which is the wide reading.

**Mapping.** old `P0` -> `P1`. Old `P1`: red-main bugs -> `P2`; other bugs
-> `P3`; enhancements the maintainer asked for -> `P4`; `area:mcp` and
process -> `P5`; other enhancements -> `P6`. Old `P2`: bugs and polish ->
`P7`; enhancements -> `P8` (`P6` where an open PR works them).

**Watch.**
1. The meaning of `P1` flips (old "user hits wrong behaviour" becomes
   "unsafe output"). The relabel must be one pass at merge, old `P1` moved
   BEFORE old `P0` is renamed, or an old-`P1` issue reads as unsafe.
2. Agents still rate their own issues, so the scale orders the list but does
   not slow its growth.

**Decided on review of PR #1193 (orchestrator, from the maintainer's
answers).** (1) One cut rule: no minor/major cut starts while ANY `P1` or
`P2` issue is open. (2) `P5` is for the agent surface keeping up; an MCP
defect is rated `P1`-`P3`. (3) An issue with no priority is untriaged, not
lowest: triaged before any pick; the Pick snippets list it first. (4) The
claim-workflow copy of the snippet agrees with `SKILL.md`. (5-8) The
script below: `P?` issues are never given a number (a stray `P0`-`P2`
beside `P?` is removed, and listed apart, because a leftover `P2` would
mean "main is broken"); the "main is broken" `P2` set is an explicit list,
never guessed from labels; an old `P1` is a bug first (`P3`, or `P2` if
listed), then maintainer-requested (`P4`, explicit list), then
`area:mcp`/`area:build` enhancements (`P5`), then `P6`; old `P2`
enhancements go to `P8`, `P6` only with an open PR linked; the script is
idempotent through a plan file; the `P0` label is kept (re-described as
retired) because deleting it would erase it from closed issues.

**Migration script.** Not run by the PR. The merging session runs
`python migrate.py --dry-run --main-broken <numbers> --requested
530-548,617,661,1103,1104,...`, reads the plan and the candidate lists
(old-`P1` bugs with `area:build`, the `P?` list, the untyped old-`P1`s),
chooses the two lists, then runs it without `--dry-run`. `--dry-run` writes
nothing and calls only `gh label list`, `gh issue list`, `gh pr list`. It
touches open issues only; closed ones keep their history. A re-run reads
the same plan file and skips applied rows. Without a plan file, if any
issue already carries `P3`..`P9` it stops: "migration already started: pass
the plan file".

Open issues with no priority at all are not in the plan (they are untriaged, set by hand); the end-state assertion lists them and exits 1 until they carry one.

```python
#!/usr/bin/env python3
# One-shot P0/P1/P2 -> P1..P9 relabel for #1189.
# migrate.py [--dry-run] [--plan FILE] [--main-broken 1021,...] [--requested 530-548,617,...]
import argparse, json, os, re, subprocess, sys

ap = argparse.ArgumentParser()
ap.add_argument("--dry-run", action="store_true")
ap.add_argument("--plan", default="migration-plan-1189.json")
ap.add_argument("--main-broken", default="")   # old-P1 bugs that become P2
ap.add_argument("--requested", default="")     # old-P1 enhancements the maintainer asked for -> P4
a = ap.parse_args()
DRY = a.dry_run

def nums(spec):
    out = set()
    for part in filter(None, (x.strip() for x in spec.split(","))):
        lo, _, hi = part.partition("-")
        out |= set(range(int(lo), int(hi or lo) + 1))
    return out

def gh(*args):
    return subprocess.run(["gh", *args], capture_output=True, text=True, check=True).stdout

def names(i): return {l["name"] for l in i["labels"]}
def numeric(n): return sorted(x for x in n if re.fullmatch(r"P[0-9]", x))

LABELS = [  # red to grey
    ("P1", "B60205", "Unsafe or wrong output: gouge, collision, wrong G-code, crash, data loss; blocks a cut"),
    ("P2", "D93F0B", "Main is broken or a regression with no workaround; blocks a cut"),
    ("P3", "E99695", "Wrong behaviour a user hits, with a workaround; fix now"),
    ("P4", "F9A03F", "Work the maintainer asked for, for the next cut"),
    ("P5", "FBCA04", "Agent surface and tooling keep up with what shipped"),
    ("P6", "C2E0C6", "Accepted enhancement, not scheduled"),
    ("P7", "BFD4F2", "Owed polish and small fixes"),
    ("P8", "C5DEF5", "Larger idea needing design or research before code"),
    ("P9", "D4D4D4", "Someday: not picked while anything else is free"),
]

def open_issues():
    return json.loads(gh("issue", "list", "--state", "open", "--limit", "2000",
                         "--json", "number,title,labels"))

# ---- plan: derived once, from the pre-migration labels, then frozen in a file
if os.path.exists(a.plan):
    plan = json.load(open(a.plan))
    print(f"using existing plan {a.plan}: {sum(r['done'] for r in plan['rows'])}/{len(plan['rows'])} rows applied")
else:
    started = gh("issue", "list", "--state", "all", "--limit", "1",
                 "--search", "label:P3,P4,P5,P6,P7,P8,P9", "--json", "number")
    if json.loads(started):
        sys.exit("migration already started: pass the plan file (--plan FILE)")
    main_broken, requested = nums(a.main_broken), nums(a.requested)
    prs = json.loads(gh("pr", "list", "--state", "open", "--limit", "500",
                        "--json", "number,body,closingIssuesReferences"))
    has_pr = set()
    for pr in prs:
        has_pr |= {r["number"] for r in pr.get("closingIssuesReferences") or []}
        has_pr |= {int(x) for x in re.findall(r"(?i)(?:fixes|closes|refs)\s+#(\d+)", pr.get("body") or "")}
    rows, pq, multi, untyped, cand = [], [], [], [], []
    for i in open_issues():
        n, num = names(i), i["number"]
        old = [x for x in numeric(n) if x in ("P0", "P1", "P2")]
        if "P?" in n:
            if old:   # paused: lose the stray number, keep P?
                pq.append((num, old))
                rows += [dict(issue=num, old=o, new=None, done=False, phase=4) for o in old]
            continue
        if len(old) > 1:
            multi.append((num, old)); continue
        if not old:
            continue
        o, bug = old[0], "enhancement" not in n
        if o == "P0":
            rows.append(dict(issue=num, old="P0", new="P1", done=False, phase=2))
        elif o == "P1":
            mcp = "area:mcp" in n or i["title"].startswith("MCP:")
            if "bug" in n and num in main_broken: new = "P2"
            elif "bug" in n: new = "P3"
            elif "enhancement" not in n and mcp: new = "P5"   # label-less MCP item: owed surface, not a defect
            elif bug and num in main_broken: new = "P2"
            elif bug: new = "P3"
            elif num in requested: new = "P4"
            elif "area:mcp" in n or "area:build" in n or "area:process" in n: new = "P5"
            else: new = "P6"
            if bug and "area:build" in n and num not in main_broken: cand.append(i)
            if "bug" not in n and "enhancement" not in n and not mcp: untyped.append(num)
            rows.append(dict(issue=num, old="P1", new=new, done=False, phase=1))
        elif o == "P2":
            new = "P7" if bug else ("P6" if num in has_pr else "P8")
            rows.append(dict(issue=num, old="P2", new=new, done=False, phase=3))
    plan = dict(rows=rows)
    print("(issue, old, new):")
    for r in sorted(rows, key=lambda r: (r["phase"], r["issue"])):
        print(f"  #{r['issue']}: {r['old']} -> {r['new'] or '(removed; P? kept)'}")
    print("\nP? issues that lose a stray number (never given P1..P9):", pq)
    print("Carry two old numbers (NOT in the plan; fix by hand):", multi)
    print("Old-P1 with neither bug nor enhancement, not MCP (planned as bugs; check):", untyped)
    print("Old-P1 bugs with area:build planned as P3 (choose --main-broken from these):")
    for i in cand: print(f"  #{i['number']} {i['title']}")
    print("Old-P2 enhancements planned as P6 (open PR linked):",
          [r["issue"] for r in rows if r["old"] == "P2" and r["new"] == "P6"])
    if not DRY:
        json.dump(plan, open(a.plan, "w"), indent=1)

if DRY:
    print("\n--dry-run: no label, issue or file was changed")
    sys.exit(0)

# ---- apply: labels, then rows in phase order (old P1 first, then P0 -> P1, then P2, then P? cleanup)
existing = {l["name"] for l in json.loads(gh("label", "list", "--limit", "500", "--json", "name"))}
for name, colour, desc in LABELS:
    gh("label", "edit" if name in existing else "create", name, "--color", colour, "--description", desc)
for r in sorted(plan["rows"], key=lambda r: (r["phase"], r["issue"])):
    if r["done"]: continue
    cmd = ["issue", "edit", str(r["issue"]), "--remove-label", r["old"]]
    if r["new"]: cmd += ["--add-label", r["new"]]
    gh(*cmd)
    r["done"] = True
    json.dump(plan, open(a.plan, "w"), indent=1)   # record each applied row
    print(f"#{r['issue']}: {r['old']} -> {r['new'] or '(removed)'}")
# P0 stays (history on closed issues); re-describe it as retired
gh("label", "edit", "P0", "--description", "Retired 2026-11-12: was the top priority; see P1")

# ---- end-state assertions
bad, counts = [], {}
for i in open_issues():
    n, nm = names(i), numeric(names(i))
    if "P0" in n: bad.append((i["number"], "still carries P0"))
    if "P?" in n and nm: bad.append((i["number"], f"P? beside {nm}"))
    elif "P?" not in n and len(nm) != 1: bad.append((i["number"], f"priorities: {nm}"))
    for x in nm + (["P?"] if "P?" in n else []): counts[x] = counts.get(x, 0) + 1
print("counts per label:", dict(sorted(counts.items())))
if bad:
    print("ASSERTION FAILED:")
    for n, w in bad: print(f"  #{n}: {w}")
    sys.exit(1)
print("assertions ok: every open issue has exactly one of P1..P9 or P? alone; no open P0")
```
