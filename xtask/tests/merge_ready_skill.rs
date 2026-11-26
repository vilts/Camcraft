//! Provenance: issue #1828's author-only merge-ready incident and the repository review law.
//! These pins check the skill's instructions, not a GitHub review-classification implementation.

const SKILL: &str = include_str!("../../.claude/skills/merge-ready/SKILL.md");

fn section(start: &str, end: &str) -> &'static str {
    SKILL
        .split_once(start)
        .expect("skill section exists")
        .1
        .split_once(end)
        .expect("next section exists")
        .0
}

#[test]
fn merge_ready_collects_paginated_review_evidence() {
    let collect = section("## 1. Collect", "## 2. Plan");
    for endpoint in ["pulls/N/reviews", "issues/N/comments", "issues/N/events"] {
        assert!(
            collect
                .lines()
                .any(|line| line.starts_with("gh api ") && line.contains("--paginate") && line.contains(endpoint)),
            "collection must explicitly fetch every page of {endpoint} before classifying a PR"
        );
    }
}

#[test]
fn merge_ready_refuses_author_only_evidence_without_independent_agent_provenance() {
    let collect = section("## 1. Collect", "## 2. Plan");
    let prose = collect.split_whitespace().collect::<Vec<_>>().join(" ");
    for required in ["author-only", "no counted review - not offered", "different agent", "did not write"] {
        assert!(prose.contains(required), "collection must state the independence rule: {required}");
    }
    assert!(
        collect
            .lines()
            .any(|line| line.starts_with('|') && line.contains("author-only") && line.contains("no counted review - not offered")),
        "a worked example must reject author-only markers even when the author is a collaborator"
    );
}

#[test]
fn merge_ready_plans_only_the_filtered_pr_numbers() {
    let plan = section("## 2. Plan", "## 3. Ask");
    let commands: Vec<_> = plan.lines().filter(|line| line.starts_with("cargo xtask merge-plan ")).collect();
    assert!(!commands.is_empty(), "the skill must retain its planning commands");
    assert!(
        commands.iter().all(|line| !line.contains("--label")),
        "planning by label reintroduces PRs excluded for missing independent reviews"
    );
    assert!(
        plan.contains("step 1"),
        "planning must consume the collection step's filtered PR numbers"
    );
}

#[test]
fn merge_ready_checks_the_merged_wave_before_the_first_merge() {
    let plan = section("## 2. Plan", "## 3. Ask");
    for text in [
        "--check",
        "cargo check --tests",
        "exit 1",
        "excluded",
        "before the first merge",
        "base",
        "head",
    ] {
        assert!(plan.contains(text), "premerge validation must explain {text}");
    }
    assert!(plan.contains("no test binaries"));
    assert!(plan.contains("temporary worktree"));
}
