#!/usr/bin/env bash
set -euo pipefail
TARGET="${1:-$HOME/.claude/skills/active-research/SKILL.md}"

# Extract API-First Sources section body.
BODY=$(awk '/^## API-First Sources/{flag=1; next} /^## /{flag=0} flag' "$TARGET")

if echo "$BODY" | grep -q 'postagent' && echo "$BODY" | grep -q 'actionbook browser'; then
    echo "routing rule present"
else
    echo "FAIL: routing rule must mention both 'postagent' and 'actionbook browser'" >&2
    exit 1
fi
