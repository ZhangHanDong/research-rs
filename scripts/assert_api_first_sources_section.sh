#!/usr/bin/env bash
set -euo pipefail
TARGET="${1:-$HOME/.claude/skills/active-research/SKILL.md}"

LINE=$(grep -n -E '^## API-First Sources' "$TARGET" | head -1 | cut -d: -f1 || true)
if [[ -z "$LINE" ]]; then
    echo "FAIL: API-First Sources section not found" >&2
    exit 1
fi

# Body between this header and the next ## header.
BODY=$(awk -v start="$LINE" '
    NR > start && /^## / { exit }
    NR > start { print }
' "$TARGET")

BYTES=$(echo -n "$BODY" | wc -c | tr -d ' ')
if [[ "$BYTES" -lt 200 ]]; then
    echo "FAIL: section body too short ($BYTES bytes)" >&2
    exit 1
fi

echo "section found at line $LINE, body bytes >= 200"
