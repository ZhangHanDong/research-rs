#!/usr/bin/env bash
# Recipe test: anonymous arXiv API query via postagent.
set -euo pipefail

# Binary resolution: use global postagent if available, otherwise fall back
# to the cargo debug binary from postagent-core.
POSTAGENT="${POSTAGENT:-$(command -v postagent 2>/dev/null || echo /Users/zhangalex/Work/Projects/actionbook/postagent/packages/postagent-core/target/debug/postagent-core)}"

URL="http://export.arxiv.org/api/query?search_query=ti:rust&max_results=3"

OUTPUT=$("$POSTAGENT" send --anonymous "$URL" 2>&1) || {
    EXIT=$?
    if echo "$OUTPUT" | grep -q -E '(unexpected argument|unrecognized)'; then
        echo "FAIL: postagent is too old; --anonymous flag not recognized" >&2
        echo "Fix: update postagent per spec postagent-anonymous-flag" >&2
        exit 2
    fi
    echo "FAIL: postagent send exited $EXIT" >&2
    echo "$OUTPUT" >&2
    exit 1
}

if ! echo "$OUTPUT" | grep -q '<feed'; then
    echo "FAIL: response does not contain <feed> (expected Atom XML)" >&2
    echo "$OUTPUT" | head -c 200 >&2
    exit 1
fi

echo "recipe_arxiv_anonymous: PASS"
