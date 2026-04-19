#!/usr/bin/env bash
# For every `actionbook browser <name>` occurrence in SKILL.md,
# verify <name> exists in the current BrowserCommands enum.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TARGET="${1:-$HOME/.claude/skills/active-research/SKILL.md}"

VALID=$(bash "$SCRIPT_DIR/cli_enum_source.sh")

# Extract all `actionbook browser <word>` occurrences.
# Tolerate optional global flags before `browser`.
MATCHES=$(grep -oE 'actionbook[^`]*browser +[a-z][a-z-]*' "$TARGET" \
    | sed -E 's/.*browser +([a-z-]+).*/\1/' \
    | sort -u)

FAIL=0
if [[ -z "$MATCHES" ]]; then
    echo "all references match (0 references found)"
    exit 0
fi

while IFS= read -r cmd; do
    if ! echo "$VALID" | grep -q -Fx "$cmd"; then
        echo "unknown subcommand: $cmd" >&2
        FAIL=1
    fi
done <<< "$MATCHES"

if [[ "$FAIL" -eq 0 ]]; then
    echo "all references match"
else
    exit 1
fi
