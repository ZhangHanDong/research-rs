#!/usr/bin/env bash
# L2 — Single-source happy paths through the `research` CLI with REAL
# postagent + actionbook + json-ui binaries. No mocks.
#
# This is the first test that exercises the full stack. Expect ~1 minute.
set -u

RESEARCH_ROOT=/Users/zhangalex/Work/Projects/actionbook/research-api-adapter
RESEARCH_BIN="$RESEARCH_ROOT/target/release/research"
POSTAGENT_BIN="${POSTAGENT_BIN:-/Users/zhangalex/Work/Projects/actionbook/postagent/packages/postagent-core/target/debug/postagent-core}"
ACTIONBOOK_BIN="${ACTIONBOOK_BIN:-/Users/zhangalex/Work/Projects/actionbook/actionbook/packages/cli/target/release/actionbook}"
JSON_UI_BIN="${JSON_UI_BIN:-json-ui}"

# Fresh isolated research dir.
TEST_HOME=$(mktemp -d -t research-l2-XXXXXX)
export ACTIONBOOK_RESEARCH_HOME="$TEST_HOME"
export POSTAGENT_BIN ACTIONBOOK_BIN JSON_UI_BIN SYNTHESIZE_NO_OPEN=1

trap 'rc=$?; [ $rc -eq 0 ] && rm -rf "$TEST_HOME" || echo "artifacts kept in $TEST_HOME"; exit $rc' EXIT

pass() { printf "  \033[32m✅ %s\033[0m\n" "$*"; }
fail() { printf "  \033[31m❌ %s\033[0m\n" "$*"; exit 1; }

section() { printf "\n\033[1m=== %s ===\033[0m\n" "$*"; }

section "L1 contract smoke (sanity check binaries)"
command -v "$POSTAGENT_BIN" >/dev/null || fail "postagent not found: $POSTAGENT_BIN"
command -v "$ACTIONBOOK_BIN" >/dev/null || fail "actionbook not found: $ACTIONBOOK_BIN"
command -v "$JSON_UI_BIN" >/dev/null || fail "json-ui not found: $JSON_UI_BIN"
pass "postagent = $(basename "$POSTAGENT_BIN")"
pass "actionbook = $(basename "$ACTIONBOOK_BIN")"
pass "json-ui = $(command -v "$JSON_UI_BIN")"
pass "research = $RESEARCH_BIN"
pass "isolated HOME = $TEST_HOME"

section "L2.1 — new session"
"$RESEARCH_BIN" new "E2E Rust async" --slug e2e --preset tech --json > /tmp/new.json
jq -e '.ok == true' /tmp/new.json >/dev/null && pass "new returned ok" || fail "new failed: $(cat /tmp/new.json)"
[ -f "$TEST_HOME/e2e/session.md" ] && pass "session.md written" || fail "no session.md"
[ -f "$TEST_HOME/e2e/session.jsonl" ] && pass "session.jsonl written" || fail "no session.jsonl"

section "L2.2 — add HN item (real postagent API path)"
"$RESEARCH_BIN" add "https://news.ycombinator.com/item?id=42" --slug e2e --json > /tmp/add_hn.json 2>&1
jq -c '.ok, .data.route_decision.executor, .data.smell_pass, .data.trust_score' /tmp/add_hn.json 2>&1
if jq -e '.ok == true and .data.route_decision.executor == "postagent" and .data.smell_pass == true' /tmp/add_hn.json >/dev/null; then
  pass "HN item accepted via postagent, smell pass"
else
  cat /tmp/add_hn.json
  fail "HN item not accepted"
fi
RAW_PATH=$(jq -r '.data.raw_path' /tmp/add_hn.json)
[ -f "$TEST_HOME/e2e/$RAW_PATH" ] && pass "raw/ file exists: $RAW_PATH" || fail "raw missing at $TEST_HOME/e2e/$RAW_PATH"

section "L2.3 — add GitHub repo README (another API path)"
"$RESEARCH_BIN" add "https://github.com/rust-lang/rust" --slug e2e --json > /tmp/add_gh.json
if jq -e '.data.route_decision.kind == "github-repo-readme" and .data.smell_pass == true' /tmp/add_gh.json >/dev/null; then
  pass "GH repo README accepted"
else
  cat /tmp/add_gh.json
  fail "GH repo not accepted"
fi

section "L2.4 — add 404 source (rejected path)"
"$RESEARCH_BIN" add "https://github.com/definitely-nonexistent-xyz/nope" --slug e2e --json > /tmp/add_404.json
REASON=$(jq -r '.error.details.reject_reason // "none"' /tmp/add_404.json)
if [ "$REASON" = "api_error" ]; then
  pass "404 rejected as api_error"
else
  echo "actual envelope:"; cat /tmp/add_404.json
  fail "expected api_error, got $REASON"
fi

section "L2.5 — sources --rejected"
"$RESEARCH_BIN" sources e2e --rejected --json > /tmp/sources.json
ACCEPTED=$(jq '.data.accepted | length' /tmp/sources.json)
REJECTED=$(jq '.data.rejected | length' /tmp/sources.json)
if [ "$ACCEPTED" = "2" ] && [ "$REJECTED" = "1" ]; then
  pass "sources: 2 accepted + 1 rejected"
else
  cat /tmp/sources.json
  fail "expected 2 accepted + 1 rejected, got $ACCEPTED + $REJECTED"
fi

section "L2.6 — edit session.md to fill Overview/Findings/Notes"
cat > "$TEST_HOME/e2e/session.md" <<'MD'
# Research: E2E Rust async

## Objective
Validate the end-to-end research CLI against real binaries.

## Preset
tech

## Sources
<!-- research:sources-start -->
_(auto-managed)_
<!-- research:sources-end -->

## Overview
This is a real end-to-end smoke test of the research CLI. We added two API
sources (HN item + GitHub repo) and a rejected 404 to verify routing and
smell-test integration work against production binaries.

## Findings
### Routing works end-to-end
`research add` correctly routes HN and GitHub URLs to postagent and rejects
404 responses with structured error codes.

### Raw storage layout
Each accepted source lands in raw/<n>-<kind>-<host>.json and is referenced
from session.jsonl as expected.

## Notes
Confirms the 5-field observability envelope (route_decision, fetch_success,
smell_pass, bytes, warnings) returned by real postagent invocations match
what we assumed when the unit tests were written.
MD
pass "session.md edited"

section "L2.7 — synthesize"
"$RESEARCH_BIN" synthesize e2e --json > /tmp/syn.json
if jq -e '.ok == true' /tmp/syn.json >/dev/null; then
  pass "synthesize ok"
else
  cat /tmp/syn.json
  fail "synthesize failed"
fi
[ -f "$TEST_HOME/e2e/report.json" ] && pass "report.json exists" || fail "no report.json"
[ -f "$TEST_HOME/e2e/report.html" ] && pass "report.html exists" || fail "no report.html"

# Sanity checks on report content.
REPORT_JSON="$TEST_HOME/e2e/report.json"
jq -e '.type == "Report" and (.children | length) >= 6' "$REPORT_JSON" >/dev/null \
  && pass "report.json has canonical structure" || fail "report.json malformed"
jq -e '[.children[] | .props.title // empty] | contains(["Overview","Key Findings","Sources","Methodology"])' "$REPORT_JSON" >/dev/null \
  && pass "report has 4 required Sections" || fail "missing required sections"
HTML_BYTES=$(wc -c < "$TEST_HOME/e2e/report.html" | tr -d ' ')
if [ "$HTML_BYTES" -gt 2000 ]; then
  pass "report.html $HTML_BYTES bytes (> 2KB)"
else
  fail "report.html too small ($HTML_BYTES bytes)"
fi

section "L2.8 — close + list"
"$RESEARCH_BIN" close e2e --json > /tmp/close.json
jq -e '.ok == true' /tmp/close.json >/dev/null && pass "close ok" || fail "close failed"
"$RESEARCH_BIN" list --json > /tmp/list.json
jq -e '.data.sessions[0].status == "closed"' /tmp/list.json >/dev/null \
  && pass "session shows closed in list" || fail "list doesn't reflect close"

section "Summary"
printf "\n\033[32mAll L2 scenarios passed.\033[0m\n"
printf "Session artifacts: %s\n" "$TEST_HOME"
printf "Report: %s\n\n" "$TEST_HOME/e2e/report.html"
