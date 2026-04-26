use serde_json::json;

use crate::fetch::postagent;
use crate::output::{Envelope, not_implemented};

const CMD: &str = "research github-audit";
const DEPTHS: &[&str] = &["repo", "stargazers", "timeline"];
const GITHUB_API: &str = "https://api.github.com";
const FETCH_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone)]
struct RepoInput {
    owner: String,
    repo: String,
}

struct GithubResponse {
    endpoint: EndpointRecord,
    value: Option<serde_json::Value>,
}

#[derive(Clone)]
struct EndpointRecord {
    path: String,
    status: Option<i32>,
    body_bytes: u64,
}

pub fn run(repo: &str, depth: &str, sample: usize, out: Option<&str>) -> Envelope {
    if !DEPTHS.contains(&depth) {
        return Envelope::fail(
            CMD,
            "INVALID_ARGUMENT",
            "invalid --depth; expected one of: repo, stargazers, timeline",
        )
        .with_details(json!({
            "argument": "depth",
            "value": depth,
            "allowed": DEPTHS,
        }));
    }

    if !(1..=1000).contains(&sample) {
        return Envelope::fail(
            CMD,
            "INVALID_ARGUMENT",
            "--sample must be between 1 and 1000",
        )
        .with_details(json!({
            "argument": "sample",
            "value": sample,
            "min": 1,
            "max": 1000,
        }));
    }

    let repo_input = match parse_repo_input(repo) {
        Ok(repo_input) => repo_input,
        Err(envelope) => return envelope,
    };

    if depth != "repo" {
        let _ = out;
        return not_implemented(CMD);
    }

    collect_repo_depth(&repo_input)
}

fn parse_repo_input(input: &str) -> Result<RepoInput, Envelope> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(invalid_repo_input(input));
    }

    let path = if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
        if rest.contains('?') || rest.contains('#') {
            return Err(invalid_repo_input(input));
        }
        rest.trim_matches('/')
    } else if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Err(invalid_repo_input(input));
    } else {
        trimmed.trim_matches('/')
    };

    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() != 2 || segments.iter().any(|s| s.is_empty()) {
        return Err(invalid_repo_input(input));
    }

    Ok(RepoInput {
        owner: segments[0].to_string(),
        repo: segments[1].to_string(),
    })
}

fn invalid_repo_input(input: &str) -> Envelope {
    Envelope::fail(
        CMD,
        "INVALID_ARGUMENT",
        "repo must be owner/repo or https://github.com/owner/repo",
    )
    .with_details(json!({
        "argument": "repo",
        "value": input,
    }))
}

fn collect_repo_depth(repo: &RepoInput) -> Envelope {
    let repo_path = format!("/repos/{}/{}", repo.owner, repo.repo);
    let contributors_path = format!("{repo_path}/contributors?per_page=100");
    let subscribers_path = format!("{repo_path}/subscribers?per_page=100");
    let commit_activity_path = format!("{repo_path}/stats/commit_activity");
    let stats_contributors_path = format!("{repo_path}/stats/contributors");

    let repo_response = match github_get_required(&repo_path) {
        Ok(response) => response,
        Err(envelope) => return envelope,
    };
    let contributors_response = match github_get_required(&contributors_path) {
        Ok(response) => response,
        Err(envelope) => return envelope,
    };
    let subscribers_response = match github_get_required(&subscribers_path) {
        Ok(response) => response,
        Err(envelope) => return envelope,
    };
    let commit_activity_response = match github_get_required(&commit_activity_path) {
        Ok(response) => response,
        Err(envelope) => return envelope,
    };
    let stats_contributors_response = match github_get_required(&stats_contributors_path) {
        Ok(response) => response,
        Err(envelope) => return envelope,
    };

    let mut endpoints = vec![
        endpoint_json(&repo_response.endpoint),
        endpoint_json(&contributors_response.endpoint),
        endpoint_json(&subscribers_response.endpoint),
        endpoint_json(&commit_activity_response.endpoint),
        endpoint_json(&stats_contributors_response.endpoint),
    ];
    let mut unavailable = Vec::new();
    for path in [
        format!("{repo_path}/traffic/views"),
        format!("{repo_path}/traffic/clones"),
        format!("{repo_path}/traffic/popular/referrers"),
    ] {
        match github_get_optional(&path) {
            Ok(response) => endpoints.push(endpoint_json(&response.endpoint)),
            Err(record) => unavailable.push(json!({
                "endpoint": record.path.clone(),
                "path": record.path,
                "status": record.status,
                "reason": "unavailable",
            })),
        }
    }

    let repo_json = repo_response.value.unwrap_or_else(|| json!({}));
    let contributors_count = array_len(contributors_response.value.as_ref());
    let subscribers_count = array_len(subscribers_response.value.as_ref());
    let commit_activity_source = if commit_activity_response
        .value
        .as_ref()
        .is_some_and(|v| v.is_array())
    {
        "github_native_stats"
    } else {
        "unavailable"
    };

    let stars = repo_json
        .get("stargazers_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let forks = repo_json
        .get("forks_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let open_issues = repo_json
        .get("open_issues_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let html_url = repo_json
        .get("html_url")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("https://github.com/{}/{}", repo.owner, repo.repo));

    Envelope::ok(
        CMD,
        json!({
            "repository": {
                "owner": repo.owner,
                "repo": repo.repo,
                "html_url": html_url,
                "stars": stars,
                "forks": forks,
                "open_issues": open_issues,
            },
            "depth": "repo",
            "sample": {
                "requested": 0,
                "fetched": 0,
                "pages": 0,
            },
            "risk": {
                "score": 0,
                "band": "low",
                "confidence": 0.5,
                "reasons": [],
            },
            "signals": {
                "repo": {
                    "stars": stars,
                    "forks": forks,
                    "open_issues": open_issues,
                    "contributors_count": contributors_count,
                    "subscribers_count": subscribers_count,
                    "commit_activity_source": commit_activity_source,
                    "watchers_count_ignored": true,
                },
                "stargazers": {},
                "timeline": {},
            },
            "github_api": {
                "authenticated": false,
                "endpoints": endpoints,
                "unavailable": unavailable,
                "rate_limit_remaining_min": null,
            },
        }),
    )
}

fn github_get_required(path: &str) -> Result<GithubResponse, Envelope> {
    let response = github_get(path).map_err(|message| {
        Envelope::fail(CMD, "FETCH_FAILED", message).with_details(json!({ "path": path }))
    })?;

    if response.endpoint.status != Some(200) {
        return Err(
            Envelope::fail(CMD, "GITHUB_API_ERROR", "GitHub API request failed").with_details(
                json!({
                    "path": response.endpoint.path,
                    "status": response.endpoint.status,
                }),
            ),
        );
    }

    if response.value.is_none() {
        return Err(
            Envelope::fail(CMD, "GITHUB_API_ERROR", "GitHub API response was not JSON")
                .with_details(json!({
                    "path": response.endpoint.path,
                    "status": response.endpoint.status,
                })),
        );
    }

    Ok(response)
}

fn github_get_optional(path: &str) -> Result<GithubResponse, EndpointRecord> {
    match github_get(path) {
        Ok(response) if matches!(response.endpoint.status, Some(403) | Some(404)) => {
            Err(response.endpoint)
        }
        Ok(response) => Ok(response),
        Err(_) => Err(EndpointRecord {
            path: path.to_string(),
            status: None,
            body_bytes: 0,
        }),
    }
}

fn github_get(path: &str) -> Result<GithubResponse, String> {
    let url = format!("{GITHUB_API}{path}");
    let args = vec![
        "send".to_string(),
        url,
        "-H".to_string(),
        "Accept: application/vnd.github+json".to_string(),
    ];
    let raw = postagent::run_args(&args, FETCH_TIMEOUT_MS)?;
    let parsed = postagent::parse(&raw).ok_or_else(|| "parse postagent output".to_string())?;
    let value = if parsed.status == Some(200) {
        Some(
            serde_json::from_slice(&raw.raw_stdout)
                .map_err(|e| format!("parse GitHub JSON for {path}: {e}"))?,
        )
    } else {
        None
    };

    Ok(GithubResponse {
        endpoint: EndpointRecord {
            path: path.to_string(),
            status: parsed.status,
            body_bytes: parsed.body_bytes,
        },
        value,
    })
}

fn endpoint_json(record: &EndpointRecord) -> serde_json::Value {
    json!({
        "endpoint": record.path.clone(),
        "path": record.path.clone(),
        "status": record.status,
        "body_bytes": record.body_bytes,
    })
}

fn array_len(value: Option<&serde_json::Value>) -> usize {
    value.and_then(|v| v.as_array()).map_or(0, Vec::len)
}
