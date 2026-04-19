#!/usr/bin/env bash
set -euo pipefail
TARGET="${1:-$HOME/.claude/skills/active-research/SKILL.md}"

BODY=$(awk '/^## API-First Sources/{flag=1; next} /^## /{flag=0} flag' "$TARGET")

if echo "$BODY" | grep -q 'new-tab' && echo "$BODY" | grep -q 'wait network-idle' && echo "$BODY" | grep -q 'text'; then
    echo "fallback pattern present"
else
    echo "FAIL: fallback pattern must include new-tab, wait network-idle, text" >&2
    exit 1
fi
