#!/usr/bin/env bash
#
# trace-matrix.sh — forward/backward traceability for the Assumed Safety
# Requirements (ASRs) defined in docs/traceability.md (see docs/FUSA.md,
# Phase 4). Analogous to QP's Spexygen requirement tracing.
#
#   tools/trace-matrix.sh          print the matrix
#   tools/trace-matrix.sh --check  CI mode: exit non-zero on any traceability gap
#
# An ASR is "defined" by a `### ASR-NNN` heading in docs/traceability.md and
# "implemented" by an `ASR-NNN` tag in a doc-comment under crates/. The matrix
# is bidirectional: every ASR must have >=1 implementing tag, and every code tag
# must reference a defined ASR.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SPEC="$ROOT/docs/traceability.md"
SRC="$ROOT/crates"

CHECK=0
[ "${1:-}" = "--check" ] && CHECK=1

# Canonical requirement set: the `### ASR-NNN` headings in the spec.
defined_asrs="$(grep -oE 'ASR-[0-9]{3}' "$SPEC" | sort -u)"

# Code tags: `ASR-NNN` tokens anywhere under crates/ (the spec itself excluded).
# Format: ASR-NNN<TAB>relative/path:line
tags="$(grep -rnoE 'ASR-[0-9]{3}' "$SRC" 2>/dev/null \
        | sed -E "s#^$ROOT/##; s#^(.*):([0-9]+):(ASR-[0-9]{3})#\3\t\1:\2#" \
        | sort || true)"

echo "== Backward trace (code site -> ASR) =="
if [ -n "$tags" ]; then
    printf '%s\n' "$tags" | awk -F'\t' '{ printf "  %-9s %s\n", $1, $2 }'
else
    echo "  (no ASR tags found under crates/)"
fi

echo
echo "== Forward coverage (ASR -> #sites) =="
gaps=0
dangling=0
for asr in $defined_asrs; do
    n="$(printf '%s\n' "$tags" | awk -F'\t' -v a="$asr" '$1==a' | wc -l | tr -d ' ')"
    printf "  %-9s %s site(s)\n" "$asr" "$n"
    if [ "$n" -eq 0 ]; then
        gaps=$((gaps + 1))
    fi
done

# Dangling: a code tag that references an ASR not defined in the spec.
tagged_asrs="$(printf '%s\n' "$tags" | awk -F'\t' 'NF{print $1}' | sort -u)"
for asr in $tagged_asrs; do
    if ! printf '%s\n' "$defined_asrs" | grep -qx "$asr"; then
        echo "  !! dangling tag: $asr is tagged in code but not defined in docs/traceability.md"
        dangling=$((dangling + 1))
    fi
done

echo
if [ "$gaps" -ne 0 ] || [ "$dangling" -ne 0 ]; then
    echo "FAIL: $gaps ASR(s) with no implementing tag, $dangling dangling tag(s)."
    [ "$CHECK" -eq 1 ] && exit 1
else
    echo "OK: every ASR has >=1 implementing site and no dangling tags."
fi
