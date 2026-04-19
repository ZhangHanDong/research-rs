//! Integration tests for research-synthesize.spec.md scenarios.
//!
//! Uses a fake json-ui binary via `JSON_UI_BIN` env var. The fake writes
//! an HTML stub to the `-o <path>` argument.

use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn research_bin() -> String {
    env!("CARGO_BIN_EXE_research").to_string()
}

struct Env {
    _tmp: TempDir,
    home: String,
    bin_dir: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().to_string_lossy().into_owned();
        let bin_dir = tmp.path().join("_bin");
        fs::create_dir_all(&bin_dir).unwrap();
        Self { _tmp: tmp, home, bin_dir }
    }

    fn write_fake_bin(&self, name: &str, script: &str) -> PathBuf {
        let path = self.bin_dir.join(name);
        fs::write(&path, script).unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
        path
    }

    fn research(&self, args: &[&str], env_overrides: &[(&str, &str)]) -> (Value, i32, String) {
        let mut cmd = Command::new(research_bin());
        cmd.args(args);
        cmd.env("ACTIONBOOK_RESEARCH_HOME", &self.home);
        // Default: skip `--open` side effects even if tests forget.
        cmd.env("SYNTHESIZE_NO_OPEN", "1");
        for (k, v) in env_overrides {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("spawn research");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let json_line = stdout.lines().find(|l| l.trim_start().starts_with('{'));
        let v: Value = match json_line {
            Some(l) => serde_json::from_str(l).unwrap_or(Value::Null),
            None => Value::Null,
        };
        (v, out.status.code().unwrap_or(-1), stderr)
    }

    fn session_dir(&self, slug: &str) -> PathBuf {
        PathBuf::from(&self.home).join(slug)
    }
}

fn fake_json_ui() -> String {
    // Writes a tiny HTML file to whichever path follows `-o`.
    r#"#!/bin/sh
# Usage: json-ui render <input.json> -o <output.html>
out=""
while [ $# -gt 0 ]; do
  if [ "$1" = "-o" ]; then
    shift
    out="$1"
  fi
  shift
done
if [ -n "$out" ]; then
  echo "<html><body>fake render</body></html>" > "$out"
fi
exit 0
"#.to_string()
}

fn write_session_md(dir: &PathBuf, body: &str) {
    fs::write(dir.join("session.md"), body).unwrap();
}

fn sample_md() -> &'static str {
    "\
# Research: T

## Overview
Overview body with enough content to not be a placeholder.

## Findings
### Finding A
Body for A.

### Finding B
Body for B.

## Notes
Long-form analysis here.

## Sources
<!-- research:sources-start -->
<!-- research:sources-end -->
"
}

#[test]
fn synthesize_happy_path_writes_json_and_html() {
    let env = Env::new();
    env.research(&["new", "topic", "--slug", "s1", "--json"], &[]);
    write_session_md(&env.session_dir("s1"), sample_md());
    let ui = env.write_fake_bin("json-ui", &fake_json_ui());

    let (v, code, stderr) = env.research(
        &["synthesize", "s1", "--json"],
        &[("JSON_UI_BIN", ui.to_str().unwrap())],
    );
    assert_eq!(code, 0, "stderr: {stderr}; v={v}");
    assert_eq!(v["data"]["accepted_sources"], 0);
    assert_eq!(v["data"]["rejected_sources"], 0);
    assert!(env.session_dir("s1").join("report.json").exists());
    assert!(env.session_dir("s1").join("report.html").exists());

    let jsonl = fs::read_to_string(env.session_dir("s1").join("session.jsonl")).unwrap();
    assert!(jsonl.contains("synthesize_started"));
    assert!(jsonl.contains("synthesize_completed"));
}

#[test]
fn synthesize_missing_overview_is_fatal() {
    let env = Env::new();
    env.research(&["new", "t2", "--slug", "s2", "--json"], &[]);
    // md template has placeholder Overview → should be treated as missing
    let ui = env.write_fake_bin("json-ui", &fake_json_ui());
    let (v, code, _) = env.research(
        &["synthesize", "s2", "--json"],
        &[("JSON_UI_BIN", ui.to_str().unwrap())],
    );
    assert_ne!(code, 0);
    assert_eq!(v["error"]["code"], "MISSING_OVERVIEW");
}

#[test]
fn synthesize_no_render_skips_html() {
    let env = Env::new();
    env.research(&["new", "t3", "--slug", "s3", "--json"], &[]);
    write_session_md(&env.session_dir("s3"), sample_md());

    let (v, code, _) = env.research(
        &["synthesize", "s3", "--no-render", "--json"],
        &[("JSON_UI_BIN", "/no/such/thing")],
    );
    assert_eq!(code, 0);
    assert!(env.session_dir("s3").join("report.json").exists());
    assert!(!env.session_dir("s3").join("report.html").exists());
    assert!(v["data"]["report_html_path"].is_null());
}

#[test]
fn synthesize_render_failed_when_json_ui_missing() {
    let env = Env::new();
    env.research(&["new", "t4", "--slug", "s4", "--json"], &[]);
    write_session_md(&env.session_dir("s4"), sample_md());

    let (v, code, _) = env.research(
        &["synthesize", "s4", "--json"],
        &[("JSON_UI_BIN", "/no/such/binary/json-ui-xxx")],
    );
    assert_ne!(code, 0);
    assert_eq!(v["error"]["code"], "RENDER_FAILED");
    // report.json must still be written
    assert!(env.session_dir("s4").join("report.json").exists());
    // jsonl has synthesize_failed with stage=render
    let jsonl = fs::read_to_string(env.session_dir("s4").join("session.jsonl")).unwrap();
    let failed_line = jsonl
        .lines()
        .find(|l| l.contains("synthesize_failed"))
        .unwrap();
    let v: Value = serde_json::from_str(failed_line).unwrap();
    assert_eq!(v["stage"], "render");
}

#[test]
fn synthesize_report_has_canonical_structure() {
    let env = Env::new();
    env.research(&["new", "t5", "--slug", "s5", "--json"], &[]);
    write_session_md(&env.session_dir("s5"), sample_md());
    let ui = env.write_fake_bin("json-ui", &fake_json_ui());
    env.research(
        &["synthesize", "s5", "--json"],
        &[("JSON_UI_BIN", ui.to_str().unwrap())],
    );

    let text = fs::read_to_string(env.session_dir("s5").join("report.json")).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["type"], "Report");
    let children = v["children"].as_array().unwrap();
    // BrandHeader + Overview + Findings + Analysis + Sources + Methodology + BrandFooter = 7
    // (Metrics and Conclusion optional; absent in our sample)
    let types: Vec<&str> = children.iter().map(|c| c["type"].as_str().unwrap()).collect();
    assert!(types.contains(&"BrandHeader"));
    assert!(types.contains(&"BrandFooter"));
    let titles: Vec<&str> = children
        .iter()
        .filter_map(|c| c["props"]["title"].as_str())
        .collect();
    assert!(titles.contains(&"Overview"));
    assert!(titles.contains(&"Key Findings"));
    assert!(titles.contains(&"Analysis"));
    assert!(titles.contains(&"Sources"));
    assert!(titles.contains(&"Methodology"));
}

#[test]
fn synthesize_is_idempotent_rewrite() {
    let env = Env::new();
    env.research(&["new", "t6", "--slug", "s6", "--json"], &[]);
    write_session_md(&env.session_dir("s6"), sample_md());
    let ui = env.write_fake_bin("json-ui", &fake_json_ui());
    let args: &[&str] = &["synthesize", "s6", "--json"];
    let envs: &[(&str, &str)] = &[("JSON_UI_BIN", ui.to_str().unwrap())];

    let (_, code1, _) = env.research(args, envs);
    assert_eq!(code1, 0);
    let first = fs::metadata(env.session_dir("s6").join("report.json")).unwrap();

    // Rewrite findings to have only 1 entry.
    let modified = "\
# Research: T

## Overview
Overview body with enough content to not be a placeholder.

## Findings
### Only One
The one finding.

## Notes
Notes.

## Sources
<!-- research:sources-start -->
<!-- research:sources-end -->
";
    write_session_md(&env.session_dir("s6"), modified);

    let (_, code2, _) = env.research(args, envs);
    assert_eq!(code2, 0);
    let second = fs::metadata(env.session_dir("s6").join("report.json")).unwrap();
    // modification time should advance (or at least not be earlier)
    assert!(second.modified().unwrap() >= first.modified().unwrap());

    // Content reflects 1 finding, not 2
    let text = fs::read_to_string(env.session_dir("s6").join("report.json")).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    let children = v["children"].as_array().unwrap();
    let findings = children
        .iter()
        .find(|c| c["props"]["title"] == "Key Findings")
        .unwrap();
    let items = findings["children"][0]["props"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Only One");
}
