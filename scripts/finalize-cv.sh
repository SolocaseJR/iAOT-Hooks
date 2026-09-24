#!/usr/bin/env bash
set -euo pipefail

protected="$1"
verifier="$2"
toolchain="${THEOS:-$HOME/theos}/toolchain/linux/iphone/bin/strip"
[[ -f "$protected" ]] || { echo '[cv] protected dylib missing' >&2; exit 1; }
[[ -x "$toolchain" ]] || { echo '[cv] Theos Mach-O strip tool missing' >&2; exit 1; }
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
cp -- "$protected" "$stage/protected.dylib"
"$toolchain" -x -S "$stage/protected.dylib"
python3 "$verifier" --scrub-final-symbols "$stage/protected.dylib"
python3 "$verifier" --final-symbols "$stage/protected.dylib"
cp -- "$stage/protected.dylib" "$protected"
