#!/usr/bin/env bash
# Extract the list of valid `actionbook browser` subcommand names
# from packages/cli/src/cli.rs. Prints one name per line on stdout.
#
# Used by other assertion scripts as the source of truth for which
# subcommands actually exist in the BrowserCommands enum.
#
# Note: The original task specified a gawk-style awk script using
# match($0, /re/, arr) capture groups, which is not supported by
# BSD awk (macOS default). This script uses BSD-awk-compatible awk
# plus a Python helper for CamelCase → kebab-case conversion.
set -euo pipefail

CLI_RS="${ACTIONBOOK_CLI_RS:-/Users/zhangalex/Work/Projects/actionbook/actionbook/packages/cli/src/cli.rs}"

if [[ ! -f "$CLI_RS" ]]; then
    echo "ERROR: cli.rs not found at $CLI_RS" >&2
    exit 2
fi

# Python is available on macOS and handles CamelCase→kebab-case cleanly.
# We use it as a helper to convert variant names extracted by awk.
camel_to_kebab() {
    python3 -c "
import re, sys
for line in sys.stdin:
    line = line.strip()
    if line:
        # Insert hyphen before each uppercase letter that follows a lowercase
        # or digit (standard CamelCase split rule)
        kebab = re.sub(r'(?<=[a-z0-9])([A-Z])', r'-\1', line).lower()
        print(kebab)
"
}

# Extract subcommand names from BrowserCommands enum.
# Strategy:
#   1. Detect entry into BrowserCommands enum block.
#   2. Track whether we are inside a multi-line #[command(...)] block.
#   3. On attribute lines, look for name = "..." (override) or alias = "..." (extra name).
#   4. On variant lines (leading spaces + UpperCase identifier), emit the name.
#      If a name = "..." override was seen for this variant, emit that instead.
#      Also emit any alias = "..." seen for this variant.
#   5. Exit on closing brace at column 1.
awk '
    BEGIN { in_enum = 0; in_attr_block = 0; pending_name = ""; pending_aliases = ""; }

    # Detect enum entry
    /pub enum BrowserCommands/ { in_enum = 1; next }

    # Stop at end of enum (closing brace at column 1)
    in_enum && /^\}/ { exit }

    !in_enum { next }

    # Handle attribute lines: #[command(...)]
    /^    #\[command\(/ {
        line = $0

        # Check for name = "..."
        if (match(line, /name = "/)) {
            rest = substr(line, RSTART + 8)
            end = index(rest, "\"")
            if (end > 0) {
                pending_name = substr(rest, 1, end - 1)
            }
        }

        # Check for alias = "..."
        if (match(line, /alias = "/)) {
            rest = substr(line, RSTART + 9)
            end = index(rest, "\"")
            if (end > 0) {
                alias = substr(rest, 1, end - 1)
                if (pending_aliases == "") {
                    pending_aliases = alias
                } else {
                    pending_aliases = pending_aliases "\n" alias
                }
            }
        }

        # Track if this attribute spans multiple lines (ends without ])
        if (index(line, "]") == 0) {
            in_attr_block = 1
        }
        next
    }

    # Continue multi-line attribute block
    in_attr_block {
        line = $0
        # Check for alias in continuation lines too
        if (match(line, /alias = "/)) {
            rest = substr(line, RSTART + 9)
            end = index(rest, "\"")
            if (end > 0) {
                alias = substr(rest, 1, end - 1)
                if (pending_aliases == "") {
                    pending_aliases = alias
                } else {
                    pending_aliases = pending_aliases "\n" alias
                }
            }
        }
        if (index(line, "]") > 0) {
            in_attr_block = 0
        }
        next
    }

    # Detect variant lines: 4 spaces + UpperCase letter
    /^    [A-Z]/ {
        line = $0
        # Extract variant name: everything up to first non-identifier char
        variant = line
        sub(/^    /, "", variant)           # strip leading spaces
        sub(/[^A-Za-z0-9].*$/, "", variant) # strip from first non-word char

        if (length(variant) == 0) { next }

        # Emit the name: override or derived CamelCase
        if (pending_name != "") {
            print pending_name
        } else {
            print variant  # will be converted to kebab via camel_to_kebab
        }

        # Emit any aliases (already lowercase)
        if (pending_aliases != "") {
            print pending_aliases
        }

        # Reset state
        pending_name = ""
        pending_aliases = ""
        next
    }

    # Comment lines and blank lines: reset pending state from previous iteration
    # (only if we did not just see a variant)
    /^    \/\// { next }
    /^$/ { next }
' "$CLI_RS" | camel_to_kebab | sort -u
