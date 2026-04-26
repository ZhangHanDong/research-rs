use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn binary() -> String {
    env!("CARGO_BIN_EXE_ascent-research").to_string()
}

struct Env {
    _tmp: TempDir,
    home: String,
    bin_dir: PathBuf,
    postagent_log: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().to_string_lossy().into_owned();
        let bin_dir = tmp.path().join("_bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let postagent_log = tmp.path().join("postagent-requests.log");
        Self {
            _tmp: tmp,
            home,
            bin_dir,
            postagent_log,
        }
    }

    fn write_fake_bin(&self, name: &str, script: &str) -> PathBuf {
        let path = self.bin_dir.join(name);
        fs::write(&path, script).unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
        path
    }

    fn research(&self, args: &[&str]) -> (Value, String, String, i32) {
        self.research_with_postagent(args, None)
    }

    fn research_with_postagent(
        &self,
        args: &[&str],
        postagent: Option<&PathBuf>,
    ) -> (Value, String, String, i32) {
        let mut cmd = Command::new(binary());
        cmd.args(args)
            .env("ACTIONBOOK_RESEARCH_HOME", &self.home)
            .env("POSTAGENT_REQUEST_LOG", &self.postagent_log);
        if let Some(postagent) = postagent {
            cmd.env("POSTAGENT_BIN", postagent);
        }
        let out = cmd.output().expect("spawn research binary");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let json_line = stdout.lines().find(|l| l.trim_start().starts_with('{'));
        let v: Value = match json_line {
            Some(line) => serde_json::from_str(line).unwrap_or(Value::Null),
            None => Value::Null,
        };
        (v, stdout, stderr, out.status.code().unwrap_or(-1))
    }

    fn postagent_log(&self) -> String {
        fs::read_to_string(&self.postagent_log).unwrap_or_default()
    }
}

fn fake_github_postagent() -> String {
    r#"#!/bin/sh
if [ -n "$POSTAGENT_REQUEST_LOG" ]; then
  printf '%s\n' "$*" >> "$POSTAGENT_REQUEST_LOG"
fi

case "$*" in
  *"/repos/dagster-io/dagster/traffic/"*)
    printf '%s\n' '⚠ 403 — endpoint requires authorization at https://api.github.com/repos/dagster-io/dagster/traffic' >&2
    printf '%s\n' 'HTTP 403 Forbidden' >&2
    exit 0 ;;
  *"/repos/owner/repo/traffic/"*)
    printf '%s\n' '⚠ 403 — endpoint requires authorization at https://api.github.com/repos/owner/repo/traffic' >&2
    printf '%s\n' 'HTTP 403 Forbidden' >&2
    exit 0 ;;
  *"/repos/dagster-io/dagster/contributors"*)
    cat <<'JSON'
[{"login":"alice"},{"login":"bob"},{"login":"carol"}]
JSON
    exit 0 ;;
  *"/repos/owner/repo/contributors"*)
    cat <<'JSON'
[{"login":"owner"}]
JSON
    exit 0 ;;
  *"/repos/dagster-io/dagster/subscribers"*)
    cat <<'JSON'
[{"login":"watcher1"},{"login":"watcher2"}]
JSON
    exit 0 ;;
  *"/repos/owner/repo/subscribers"*)
    cat <<'JSON'
[]
JSON
    exit 0 ;;
  *"/repos/dagster-io/dagster/stats/commit_activity"*)
    cat <<'JSON'
[{"week":1711843200,"total":42}]
JSON
    exit 0 ;;
  *"/repos/owner/repo/stats/commit_activity"*)
    cat <<'JSON'
[{"week":1711843200,"total":1}]
JSON
    exit 0 ;;
  *"/repos/dagster-io/dagster/stats/contributors"*)
    cat <<'JSON'
[{"total":100,"author":{"login":"alice"}}]
JSON
    exit 0 ;;
  *"/repos/owner/repo/stats/contributors"*)
    cat <<'JSON'
[{"total":1,"author":{"login":"owner"}}]
JSON
    exit 0 ;;
  *"/repos/dagster-io/dagster"*)
    cat <<'JSON'
{"name":"dagster","full_name":"dagster-io/dagster","owner":{"login":"dagster-io"},"html_url":"https://github.com/dagster-io/dagster","stargazers_count":12345,"forks_count":2100,"open_issues_count":321,"watchers_count":12345}
JSON
    exit 0 ;;
  *"/repos/owner/repo"*)
    cat <<'JSON'
{"name":"repo","full_name":"owner/repo","owner":{"login":"owner"},"html_url":"https://github.com/owner/repo","stargazers_count":10,"forks_count":2,"open_issues_count":1,"watchers_count":10}
JSON
    exit 0 ;;
esac

printf '%s\n' "⚠ 404 — endpoint does not exist at $2" >&2
printf '%s\n' 'HTTP 404 Not Found' >&2
exit 0
"#
    .to_string()
}

#[test]
fn github_audit_rejects_invalid_depth_and_sample() {
    let env = Env::new();
    let (v, _, _, code) =
        env.research(&["--json", "github-audit", "owner/repo", "--depth", "full"]);
    assert_ne!(code, 0);
    assert_eq!(v["error"]["code"], "INVALID_ARGUMENT");

    let (v, _, _, code) = env.research(&["--json", "github-audit", "owner/repo", "--sample", "0"]);
    assert_ne!(code, 0);
    assert_eq!(v["error"]["code"], "INVALID_ARGUMENT");
}

#[test]
fn github_audit_repo_depth_anonymous_success() {
    let env = Env::new();
    let postagent = env.write_fake_bin("postagent", &fake_github_postagent());

    let (v, stdout, _, code) = env.research_with_postagent(
        &[
            "--json",
            "github-audit",
            "dagster-io/dagster",
            "--depth",
            "repo",
        ],
        Some(&postagent),
    );

    assert_eq!(code, 0, "{v:#?}");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["depth"], "repo");
    assert_eq!(v["data"]["repository"]["owner"], "dagster-io");
    assert_eq!(v["data"]["repository"]["repo"], "dagster");
    assert_eq!(
        v["data"]["repository"]["html_url"],
        "https://github.com/dagster-io/dagster"
    );
    assert_eq!(v["data"]["repository"]["stars"], 12345);
    let score = v["data"]["risk"]["score"].as_i64().unwrap();
    assert!((0..=100).contains(&score));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("GITHUB.TOKEN"));

    let log = env.postagent_log();
    assert!(log.contains("/repos/dagster-io/dagster"));
    assert!(log.contains("Accept: application/vnd.github+json"));
    assert!(!log.contains("Authorization"));
    assert!(!log.contains("GITHUB.TOKEN"));
}

#[test]
fn github_audit_accepts_github_url_input() {
    let env = Env::new();
    let postagent = env.write_fake_bin("postagent", &fake_github_postagent());

    let (v, _, _, code) = env.research_with_postagent(
        &[
            "--json",
            "github-audit",
            "https://github.com/dagster-io/dagster",
            "--depth",
            "repo",
        ],
        Some(&postagent),
    );

    assert_eq!(code, 0, "{v:#?}");
    assert_eq!(v["data"]["repository"]["owner"], "dagster-io");
    assert_eq!(v["data"]["repository"]["repo"], "dagster");
}
