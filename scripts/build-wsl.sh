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

# --- Persistent incremental staging ---------------------------------------
# Two reasons the sources are staged onto WSL's own ext4 filesystem instead of
# built in place under /mnt/c:
#   1. Compiling against /mnt/c goes through the 9p bridge and is dramatically
#      slower for the many small reads/writes a C++ build performs.
#   2. Reusing the SAME staging directory across builds lets Theos' -MMD/-MP
#      dependency tracking (see makefiles/instance/rules.mk) skip every
#      translation unit whose sources and headers are unchanged -- above all
#      the five large, never-changing Dear ImGui units. Only files you actually
#      edited get recompiled, which is the difference between a multi-minute
#      cold build and a few-second incremental one.
# The stage is keyed by a hash of the source paths so distinct projects never
# collide, and `rsync --delete` keeps the mirror byte-identical to the source
# (it copies only changed files, so a no-op sync is a cheap stat pass).
cache_root=${IAOT_BUILD_CACHE:-"${XDG_CACHE_HOME:-$HOME/.cache}/iaot-build"}
stage_key=$(printf '%s\n%s' "$repo" "$project" | sha1sum | cut -c1-16)
stage="$cache_root/$stage_key"
mkdir -p "$stage/repo" "$stage/project" "$(dirname "$output")"

rsync -a --delete --exclude='.git/' --exclude='patcher/target/' --exclude='build*/' --exclude='artifacts/' --exclude='.theos/' "$repo/" "$stage/repo/"
# Reverse-engineering dumps and IDA databases are input material for analysis,
# not runtime source.  Besides making the sync needlessly large, an open IDA
# database on the Windows mount can be unreadable from WSL and abort a build.
rsync -a --delete --exclude='.git/' --exclude='build/' --exclude='artifacts/' --exclude='.theos/' --exclude='Game Data & Dumps/' "$project/" "$stage/project/"

export THEOS="$theos_root"
debug=1
[[ "$mode" == release ]] && debug=0

# A change to build configuration -- the build mode, the generated flags in
# IAOTGeneratedProject.mk, or the Makefile itself -- must force a clean rebuild,
# because Theos object files depend on their source/header inputs but NOT on
# these makefiles, so a stale object would otherwise be relinked with the wrong
# flags. Pure source edits leave this fingerprint untouched and stay fully
# incremental. Set IAOT_FORCE_CLEAN=1 to force a clean rebuild on demand.
fingerprint=$( { printf 'mode=%s\n' "$mode"; cat "$stage/project/Makefile" "$stage/project/generated/IAOTGeneratedProject.mk"; } 2>/dev/null | sha1sum | cut -d' ' -f1 )
fingerprint_file="$stage/.iaot-config-fingerprint"
if [[ "${IAOT_FORCE_CLEAN:-0}" == 1 || ! -f "$fingerprint_file" || "$(cat "$fingerprint_file" 2>/dev/null)" != "$fingerprint" ]]; then
  echo "[runtime] build configuration changed (or first build) -> clean rebuild"
  make -C "$stage/project" clean IAOT_ROOT="$stage/repo" >/dev/null 2>&1 || true
  rm -rf -- "$stage/project/.theos"
fi

# Memory-aware parallelism. Each Dear ImGui C++ translation unit can use several
# hundred MB of RAM to compile, and WSL is frequently memory-constrained, so the
# job count is capped by BOTH core count and available memory to avoid the OOM
# kills this step has historically hit. Override with IAOT_BUILD_JOBS.
if [[ -n "${IAOT_BUILD_JOBS:-}" ]]; then
  jobs=$IAOT_BUILD_JOBS
else
  cores=$(nproc 2>/dev/null || echo 1)
  avail_mb=$(awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo 2>/dev/null || echo 1024)
  mem_jobs=$(( avail_mb / 700 )); (( mem_jobs < 1 )) && mem_jobs=1
  jobs=$(( cores < mem_jobs ? cores : mem_jobs ))
  (( jobs < 1 )) && jobs=1
fi

echo "[runtime] building arm64 dylib with Theos (mode=$mode, jobs=$jobs)"
echo "[runtime] stage: $stage"
make -C "$stage/project" -j"$jobs" IAOT_ROOT="$stage/repo" DEBUG="$debug"

# Only record the fingerprint after a successful build so an interrupted or
# failed compile re-cleans next time instead of trusting a half-built cache.
printf '%s' "$fingerprint" > "$fingerprint_file"

# With a persistent .theos the freshly linked dylib is always the newest one,
# and a config change already wiped .theos above, so no stale runtime can be
# picked up here.
dylib=$(find "$stage/project/.theos" -type f -name '*.dylib' -printf '%T@ %p\n' | sort -nr | head -n 1 | cut -d ' ' -f 2-)
[[ -n "$dylib" && -f "$dylib" ]] || { echo "[runtime] Theos completed without producing a dylib" >&2; exit 20; }
cp "$dylib" "$output"
echo "[runtime] wrote $output"
