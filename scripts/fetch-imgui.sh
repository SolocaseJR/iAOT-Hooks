#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/vendor/imgui"
REF="${IMGUI_REF:-v1.92.8}"
if [[ -d "$DEST/.git" ]]; then
  git -C "$DEST" fetch --tags --depth 1 origin "$REF"
  git -C "$DEST" checkout --detach FETCH_HEAD
elif [[ -e "$DEST" ]]; then
  echo "error: $DEST exists but is not a Git checkout" >&2
  exit 1
else
  git clone --depth 1 --branch "$REF" https://github.com/ocornut/imgui.git "$DEST"
fi
[[ -f "$DEST/imgui.cpp" ]] || { echo "error: Dear ImGui checkout is missing imgui.cpp" >&2; exit 2; }
[[ -f "$DEST/LICENSE.txt" ]] || { echo "error: Dear ImGui checkout is missing LICENSE.txt" >&2; exit 3; }
echo "Dear ImGui $REF is ready at $DEST"
