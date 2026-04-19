#!/usr/bin/env bash
# L4 — Autoresearch loop with a REAL LLM provider (Claude via cc-sdk).
# Codifies the v2 live smoke as a repeatable check with v2 acceptance
# assertions.
#
# This test **costs real Claude tokens** (cc-sdk subscription — no API
# key). Expect 3–10 minutes end-to-end. Run manually when validating v2
# autoresearch changes; NOT wired into CI.
#
# Env vars:
#   PROVIDER       = claude (default)  — currently only claude is wired
#   ITERATIONS     = 6
#   MAX_ACTIONS    = 30
#   TOPIC          = (default: a self-contained survey topic)
#   SLUG_PREFIX    = l4 (final slug: ${SLUG_PREFIX}-YYYYmmddHHMMSS)
#
# Acceptance (v2 spec §验收标准 live smoke):
#   A1. plan_written event on iteration 1  (strict)
#   A2. ≥ 3 source_digested events         (strict)
#   A3. ≥ 1 diagram_authored, ≥ 2 preferred (soft warning on <2)
#   A4. source_kind_diversity ≥ 3          (strict)
set -u

RESEARCH_ROOT=/Users/zhangalex/Work/Projects/actionbook/research-api-adapter
RESEARCH_BIN="${RESEARCH_BIN:-$RESEARCH_ROOT/target/debug/research}"
POSTAGENT_BIN="${POSTAGENT_BIN:-/Users/zhangalex/Work/Projects/actionbook/postagent/packages/postagent-core/target/debug/postagent-core}"
ACTIONBOOK_BIN="${ACTIONBOOK_BIN:-/Users/zhangalex/Work/Projects/actionbook/actionbook/packages/cli/target/release/actionbook}"
JSON_UI_BIN="${JSON_UI_BIN:-json-ui}"

PROVIDER="${PROVIDER:-claude}"
ITERATIONS="${ITERATIONS:-6}"
MAX_ACTIONS="${MAX_ACTIONS:-30}"
TOPIC="${TOPIC:-Self-Evolving Agent Protocol + ecosystem (L4 smoke)}"
SLUG="${SLUG_PREFIX:-l4}-$(date +%Y%m%d%H%M%S)"

TEST_HOME=$(mktemp -d -t research-l4-XXXXXX)
export ACTIONBOOK_RESEARCH_HOME="$TEST_HOME"
export POSTAGENT_BIN ACTIONBOOK_BIN JSON_UI_BIN SYNTHESIZE_NO_OPEN=1

trap 'rc=$?; echo ""; echo "session kept for inspection: $TEST_HOME/$SLUG"; exit $rc' EXIT

pass() { printf "  \033[32m✅ %s\033[0m\n" "$*"; }
soft() { printf "  \033[33m⚠️  %s\033[0m\n" "$*"; }
fail() { printf "  \033[31m❌ %s\033[0m\n" "$*"; exit 1; }
section() { printf "\n\033[1m=== %s ===\033[0m\n" "$*"; }

section "Preflight"
[ -x "$RESEARCH_BIN" ] || fail "research binary missing: $RESEARCH_BIN (build with --features 'autoresearch provider-claude')"
[ -x "$ACTIONBOOK_BIN" ] || fail "actionbook binary missing: $ACTIONBOOK_BIN"
[ "$PROVIDER" = "claude" ] || fail "only PROVIDER=claude is supported in L4 (got '$PROVIDER')"
pass "research = $RESEARCH_BIN"
pass "actionbook = $ACTIONBOOK_BIN"
pass "isolated HOME = $TEST_HOME"
pass "slug = $SLUG"

section "Setup — fresh session"
"$RESEARCH_BIN" new "$TOPIC" --slug "$SLUG" --preset tech --tag autoresearch-l4 --json >/dev/null \
  || fail "session creation failed"
pass "session created"

section "Run — research loop --provider $PROVIDER --iterations $ITERATIONS --max-actions $MAX_ACTIONS"
echo "  (costs real Claude tokens; 3–10 min typical) ..."
ENVELOPE=/tmp/l4_envelope.json
"$RESEARCH_BIN" loop "$SLUG" \
  --provider "$PROVIDER" \
  --iterations "$ITERATIONS" \
  --max-actions "$MAX_ACTIONS" \
  --json > "$ENVELOPE" 2>/tmp/l4_stderr.log

jq -e '.ok == true' "$ENVELOPE" >/dev/null \
  || { cat /tmp/l4_stderr.log; cat "$ENVELOPE"; fail "loop returned non-ok"; }

ITERS=$(jq -r '.data.iterations_run' "$ENVELOPE")
EXECUTED=$(jq -r '.data.actions_executed' "$ENVELOPE")
REJECTED=$(jq -r '.data.actions_rejected' "$ENVELOPE")
DURATION=$(jq -r '.data.duration_ms' "$ENVELOPE")
TERM=$(jq -r '.data.termination_reason' "$ENVELOPE")
pass "loop ok: iters=$ITERS actions=$EXECUTED rejected=$REJECTED duration=${DURATION}ms termination=$TERM"

section "v2 acceptance checks"
JSONL="$TEST_HOME/$SLUG/session.jsonl"
[ -f "$JSONL" ] || fail "missing $JSONL"

# A1 — plan_written on iteration 1
PLAN_ITER=$(grep '"event":"plan_written"' "$JSONL" | jq -r '.iteration' | head -1)
if [ "$PLAN_ITER" = "1" ]; then
  pass "A1 plan_written on iter 1"
else
  fail "A1 FAIL — plan_written iter=$PLAN_ITER (want 1)"
fi

# A2 — ≥ 3 source_digested events
DIGESTED=$(grep -c '"event":"source_digested"' "$JSONL" || true)
if [ "$DIGESTED" -ge 3 ]; then
  pass "A2 source_digested count = $DIGESTED (≥ 3)"
else
  fail "A2 FAIL — source_digested=$DIGESTED (want ≥ 3)"
fi

# A3 — ≥ 1 diagram_authored (≥ 2 preferred, warn if 1)
AUTHORED=$(grep -c '"event":"diagram_authored"' "$JSONL" || true)
REJECTED_SVG=$(grep -c '"event":"diagram_rejected"' "$JSONL" || true)
if [ "$AUTHORED" -ge 2 ]; then
  pass "A3 diagram_authored = $AUTHORED (≥ 2) [rejected: $REJECTED_SVG]"
elif [ "$AUTHORED" -eq 1 ]; then
  soft "A3 diagram_authored = 1 (spec wants ≥ 2) [rejected: $REJECTED_SVG]"
else
  fail "A3 FAIL — no diagram_authored (rejected=$REJECTED_SVG)"
fi

# A4 — source_kind_diversity ≥ 3 (from final coverage)
DIVERSITY=$(jq -r '.data.final_coverage.source_kind_diversity' "$ENVELOPE")
if [ "$DIVERSITY" -ge 3 ]; then
  pass "A4 source_kind_diversity = $DIVERSITY (≥ 3)"
  KINDS=$(grep '"event":"source_accepted"' "$JSONL" | jq -r '.kind' | sort -u | paste -sd, -)
  echo "     kinds seen: $KINDS"
else
  fail "A4 FAIL — source_kind_diversity=$DIVERSITY (want ≥ 3)"
fi

section "Coverage snapshot"
jq -r '.data.final_coverage' "$ENVELOPE"

section "Session report section headings"
grep -n "^##" "$TEST_HOME/$SLUG/session.md" | head -20

section "Diagrams authored"
if [ -d "$TEST_HOME/$SLUG/diagrams" ]; then
  ls -la "$TEST_HOME/$SLUG/diagrams/"
fi

section "Event breakdown"
grep -oE '"event":"[a-z_]+"' "$JSONL" | sort | uniq -c

section "Summary"
if [ "$AUTHORED" -lt 2 ]; then
  printf "\n\033[33mL4 pass (with soft warning on diagrams=%d < 2)\033[0m\n" "$AUTHORED"
else
  printf "\n\033[32mL4 all green — v2 autoresearch loop end-to-end validated.\033[0m\n"
fi
