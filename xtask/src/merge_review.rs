//! Provenance: issue #2388; compare the latest collaborator review marker with the fetched PR head.

use std::collections::{BTreeMap, BTreeSet};

const MARKERS: &str = r#".[] | . as $entry |
    ((.body // "") | scan("<!-- pr-reviewer: ([0-9a-fA-F]{7,40}) -->") | .[0]) as $sha |
    [$entry.user.login, ($entry.submitted_at // $entry.created_at // ""), $sha] | @tsv"#;

/// Keep warnings on stderr so `merge-plan --json` remains machine-readable.
/// The gh callback uses the same working directory and error handling as the planner.
pub fn warn(
    repo: Option<&str>,
    heads: &BTreeMap<u32, String>,
    mut gh: impl FnMut(&[&str]) -> Result<String, String>,
) -> Result<(), String> {
    let repo = match repo {
        Some(repo) => repo.to_string(),
        None => gh(&[
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "--jq",
            ".nameWithOwner",
        ])?
        .trim()
        .to_string(),
    };
    let collaborators = gh(&[
        "api",
        &format!("repos/{repo}/collaborators"),
        "--paginate",
        "--jq",
        ".[] | select(.permissions.push) | .login",
    ])?;
    let collaborators: BTreeSet<&str> = collaborators.lines().collect();
    for (number, head) in heads {
        let mut latest: Option<(String, String)> = None;
        for endpoint in [
            format!("issues/{number}/comments"),
            format!("pulls/{number}/reviews"),
        ] {
            let rows = gh(&[
                "api",
                &format!("repos/{repo}/{endpoint}"),
                "--paginate",
                "--jq",
                MARKERS,
            ])?;
            for row in rows.lines() {
                let fields: Vec<&str> = row.split('\t').collect();
                let [login, date, sha] = fields.as_slice() else {
                    return Err(format!(
                        "bad review marker row for PR #{number}: expected login, date and SHA"
                    ));
                };
                if !collaborators.contains(login) || date.is_empty() {
                    continue;
                }
                // GitHub supplies UTC ISO timestamps. Break ties by SHA for stable output.
                let marker = (date.to_string(), sha.to_ascii_lowercase());
                if latest.as_ref().is_none_or(|previous| marker > *previous) {
                    latest = Some(marker);
                }
            }
        }
        if let Some((_, reviewed)) = latest
            && !head.to_ascii_lowercase().starts_with(&reviewed)
        {
            eprintln!(
                "WARNING: PR #{number} has a stale review marker: reviewed {reviewed}, head {head}; re-check the review before merging."
            );
        }
    }
    Ok(())
}
