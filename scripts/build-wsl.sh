#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo "usage: build-wsl.sh <repo> <project> <output-dylib> <mode>" >&2
  exit 2
fi
repo=$1
project=$2
output=$3
mode=$4
theos_root=${THEOS:-"$HOME/theos"}

for command_name in bash rsync make python3; do
  command -v "$command_name" >/dev/null 2>&1 || { echo "[dependency] missing command: $command_name" >&2; exit 10; }
done
[[ -d "$theos_root" ]] || { echo "[dependency] Theos was not found at $theos_root; set THEOS or install it" >&2; exit 11; }
[[ -d "$theos_root/sdks" ]] || { echo "[dependency] Theos SDK directory is missing: $theos_root/sdks" >&2; exit 12; }
[[ -x "$theos_root/toolchain/linux/iphone/bin/clang" ]] || {
  echo "[dependency] the Theos iOS compiler is missing: $theos_root/toolchain/linux/iphone/bin/clang" >&2; exit 13;
}
find "$theos_root/sdks" -maxdepth 1 -type d -name 'iPhoneOS*.sdk' -print -quit | grep -q . || {
  echo "[dependency] no iPhoneOS SDK was found under $theos_root/sdks" >&2; exit 14;
}
settings="$project/generated/IAOTGeneratedProject.mk"
[[ -f "$settings" ]] || { echo "[config] generated project settings are missing: $settings" >&2; exit 15; }
if grep -Eq '^IAOT_ENABLE_OVERLAY := 1$' "$settings"; then
  [[ -f "$repo/vendor/imgui/imgui.cpp" ]] || {
    echo "[dependency] Dear ImGui is required by this project; run scripts/fetch-imgui.ps1 or scripts/fetch-imgui.sh" >&2; exit 16;
  }
fi

stage=$(mktemp -d "${TMPDIR:-/tmp}/iaot-build.XXXXXX")
cleanup() { rm -rf -- "$stage"; }
trap cleanup EXIT
mkdir -p "$stage/repo" "$stage/project" "$(dirname "$output")"
rsync -a --delete --exclude='.git/' --exclude='patcher/target/' --exclude='build/' --exclude='.theos/' "$repo/" "$stage/repo/"
rsync -a --delete --exclude='.git/' --exclude='build/' --exclude='.theos/' "$project/" "$stage/project/"

export THEOS="$theos_root"
debug=1
[[ "$mode" == release ]] && debug=0
echo "[runtime] building arm64 dylib with Theos (mode=$mode)"
make -C "$stage/project" clean IAOT_ROOT="$stage/repo"
make -C "$stage/project" IAOT_ROOT="$stage/repo" DEBUG="$debug"
dylib=$(find "$stage/project/.theos" -type f -name '*.dylib' -print | head -n 1)
[[ -n "$dylib" && -f "$dylib" ]] || { echo "[runtime] Theos completed without producing a dylib" >&2; exit 20; }
cp "$dylib" "$output"
echo "[runtime] wrote $output"
