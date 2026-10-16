# IAOT — iOS Ahead-of-Time Hooks

**IAOT lets you add toggleable mods, function hooks, and an on-screen menu to an
iOS app you own, without a jailbreak and without a JIT.**

It does all of its patching *offline, on your computer, before the app is
signed.* There is no runtime code injection, no process attachment, and no
jailbreak loader. You describe the hooks you want in a small YAML file and write
your callbacks in C++/Objective-C++; IAOT analyzes your decrypted IPA, rewrites
the binary, embeds a precompiled runtime dylib, and hands you back an unsigned
IPA ready for your own signing workflow.

I built this because when I went looking for how to make working, toggleable
hooks and menu overlays for **jailed (non-jailbroken), no-JIT iOS apps**, I found
almost nothing that explained it end to end. IAOT is that missing explanation,
as a reusable framework.

> ### ⚠️ Scope and responsible use
>
> IAOT is for software **you own or are explicitly authorized to analyze and
> modify** — education, research, interoperability, instrumentation, and proofs
> of concept. It does **not** decrypt IPAs, bypass DRM, obtain signing
> identities, attach to running processes, or install unsigned code. You supply
> an already-decrypted, authorized IPA and your own signing setup. See
> [Responsible Use](RESPONSIBLE_USE.md).

---

## Table of contents

- [Is this for me?](#is-this-for-me)
- [What you need](#what-you-need)
- [How it works in one picture](#how-it-works-in-one-picture)
- [Install](#install)
- [Quick start: from zero to a patched IPA](#quick-start-from-zero-to-a-patched-ipa)
- [Writing your first hook](#writing-your-first-hook)
- [Updating to a new app version](#updating-to-a-new-app-version)
- [Unity / IL2CPP apps](#unity--il2cpp-apps)
- [What IAOT can and cannot do](#what-iaot-can-and-cannot-do)
- [Generated files and reports](#generated-files-and-reports)
- [Repository layout](#repository-layout)
- [Documentation](#documentation)
- [Building and testing IAOT itself](#building-and-testing-iaot-itself)
- [License](#license)

---

## Is this for me?

IAOT is a good fit if **all** of the following are true:

- You have an iOS app you own or are authorized to modify, and you can already
  produce a **decrypted** IPA of it.
- You are targeting **jailed, no-JIT ARM64** devices (ordinary, non-jailbroken
  iPhones/iPads), where runtime hooking frameworks like Substrate are not an
  option.
- You can sign and install the finished IPA through your own authorized workflow
  (your own certificate, a sideloading tool, etc.).
- You are comfortable writing a little C++/Objective-C++ for your hook bodies.

IAOT is **not** the right tool if you need to decrypt an app, defeat DRM, attach
to an already-installed process, or hook a jailbroken device at runtime — those
are explicitly out of scope.

## What you need

| Requirement | Why |
| --- | --- |
| **Rust / Cargo** (host machine) | Builds and runs the `iaot` command-line tool. |
| **WSL2 Ubuntu** (Windows users) | Runs Theos to compile the runtime dylib. |
| **Theos + an iPhoneOS SDK** | Cross-compiles the ARM64 runtime. IAOT does not ship Apple SDKs. |
| **A decrypted, authorized IPA** | The input IAOT patches. IAOT has no decryptor. |
| **Your own signing identity** | The output IPA is unsigned; you sign it yourself. |

Full, step-by-step environment setup (WSL, Theos, SDKs, dependency checks) is in
**[docs/Setup.md](docs/Setup.md)**.

## How it works in one picture

```text
  your decrypted IPA ─┐
                      │   ┌──────────────────────────────────────────────┐
  hooks.yaml ─────────┼──▶│  iaot  (offline Rust tool, runs on your PC)    │
  configs.yaml ───────┤   │                                                │
  your C++ callbacks ─┘   │  1. analyze the IPA (type, arch, Unity, ...)   │
                          │  2. resolve each hook target in the binary     │
                          │  3. compile your callbacks into a runtime dylib│
                          │  4. write static ARM64 bridges into the binary │
                          │  5. embed the dylib + repackage the IPA        │
                          │  6. re-open and validate everything            │
                          └───────────────────────┬────────────────────────┘
                                                   ▼
                                    patched, UNSIGNED IPA  ──▶  you sign + install
```

Three layers stay strictly separated (see [docs/Architecture.md](docs/Architecture.md)):

1. **The offline tool** (`patcher/`, Rust) reads the IPA, finds your targets,
   writes the ARM64 bridges, injects the dylib load command, repackages, and
   validates.
2. **The runtime dylib** (`runtime/`, C++/Obj-C++) ships your compiled callbacks,
   the flag/toggle system, persistence, the optional ImGui menu, and the IL2CPP
   name resolver.
3. **Your code** describes exact function signatures and behavior in `hooks.yaml`
   and a couple of small source files. Generated glue connects the two.

Everything that executes on the device lives in **signed, read-only executable
pages written before launch.** The runtime never maps writable-and-executable
memory, never compiles code, and never patches live pages. See
[docs/JailedNoJITModel.md](docs/JailedNoJITModel.md).

## Install

```powershell
# From the repository root, build and install the `iaot` CLI:
cargo install --path patcher

# The default project template includes an ImGui menu, so fetch the
# pinned Dear ImGui release once (Windows):
./scripts/fetch-imgui.ps1
# ...or on Linux/WSL:
bash scripts/fetch-imgui.sh
```

If you prefer not to install globally, every `iaot ...` command below also works
as `cargo run --manifest-path patcher/Cargo.toml -- ...`.

## Quick start: from zero to a patched IPA

### 1. Scaffold a project

```powershell
iaot new MyHooks
```

This creates a `MyHooks/` folder. The only files you normally edit are:

| File | What goes in it |
| --- | --- |
| `MyHooks/configs.yaml` | Project identity, input/output paths, build settings, optional app branding. |
| `MyHooks/hooks.yaml` | The hooks themselves: flags, target functions, and patches. |
| `MyHooks/il2cpp_config.yaml` | Optional Unity/IL2CPP settings. No dumps or address tables. |
| `MyHooks/user/UserHooks.mm` | Your typed callback implementations. |
| `MyHooks/user/UserMenu.mm` | The on-screen ImGui menu. |
| `MyHooks/user/UserFeatures.hpp` | Shared toggle helpers. |

Everything under `MyHooks/generated/` is machine-owned — never edit it by hand.

### 2. Point it at your IPA and look before you patch

Set `paths.input_ipa` in `configs.yaml`, then analyze **without changing
anything**:

```powershell
iaot analyze-ipa --in "C:\IPAs\OwnedApp-Decrypted.ipa" --reports .\build\analysis
```

Read `build/analysis/ipa-analysis.yaml` and confirm the detected executable,
architecture, app type (native / Unity Mono / Unity IL2CPP / Swift / ...), and
rendering backend look right before going further.

### 3. Build the patched IPA

**On Windows** (one command does analyze → generate → compile → patch →
repackage → validate):

```powershell
.\build_iaot_ipa.bat .\MyHooks\hooks.yaml
```

Or, equivalently, with the PowerShell entry point:

```powershell
.\scripts\build-iaot.ps1 -InputIpa "C:\IPAs\OwnedApp-Decrypted.ipa" -Manifest ".\MyHooks\hooks.yaml"
```

The script verifies Cargo, WSL, Theos, SDKs, compiler tools, and ImGui; builds
the runtime inside an isolated WSL staging directory; then runs the full
pipeline. Input/output/build/report paths default to `configs.yaml`, and CLI
arguments override them.

**On Linux / WSL**, run the pipeline directly:

```bash
iaot pipeline \
  --manifest ./MyHooks/hooks.yaml \
  --in ./OwnedApp-Decrypted.ipa \
  --out ./build/OwnedApp-IAOT-unsigned.ipa \
  --build-runtime \
  --reports ./build/reports
```

(Already have a compiled dylib? Pass `--dylib path/to/MyHooks.dylib` instead of
`--build-runtime`.)

For a freshly scaffolded project the defaults are:

```text
build/<project-name>-unsigned.ipa
generated/reports/final-validation.yaml
```

### 4. Sign and install

The output IPA is **deliberately unsigned** — IAOT strips stale code signatures
but never signs. Sign the main executable, every embedded framework, and the
injected dylib with a compatible provisioning/entitlement setup, then install
through your authorized workflow. See
[docs/PackagingAndSigning.md](docs/PackagingAndSigning.md).

### (Optional) Rebrand the app while you are at it

```yaml
# in configs.yaml
app:
  display_name: "My Instrumented App"
  icon_path: assets/app-icon.png
```

The icon must be a square PNG of at least 180×180 (1024×1024 recommended). IAOT
flattens alpha against white, generates the full iPhone/iPad icon set, and
updates the display name before repackaging.

## Writing your first hook

The template ships with one **disabled** example hook. Here is the whole shape of
a replacement hook with a callable original.

**1. Declare a feature flag** (optional, for a menu toggle) in `hooks.yaml`:

```yaml
config:
  flags:
    - { name: example_hook_enabled, index: 0, default: false }
```

**2. Declare the patch** in `hooks.yaml`. Callback and original-trampoline slots
are allocated for you:

```yaml
patches:
  - name: ExampleHook
    enabled: true
    description: "Authorized example entry hook"
    replacement: IAOT_ExampleHook
    preserve_original: true
    module: main
    signature: "int(void*, int)"
    failure_behavior: disable_hook
    locator:
      type: pattern
      pattern: "FD 7B BF A9 F4 4F 01 A9"
      expected_matches: 1
      segment: "__TEXT"
      section: "__text"
    patch:
      type: branch_to_cave
    expect: auto
```

**3. Implement the matching ABI** in `user/UserHooks.mm`:

```cpp
extern "C" int IAOT_ExampleHook(void* instance, int value) {
    auto original = iaot::generated::Original_ExampleHook();
    if (!Features::ExampleHookEnabled())
        return original.Call(instance, value);   // pass through when off
    return 123;                                   // modified behavior when on
}
```

**4. Wire the menu checkbox** to the same flag in `user/UserMenu.mm` (the
template already shows the safe local-`bool` ImGui pattern and persistence).

**5. Build** (step 3 above). The pipeline analyzes, generates, compiles, writes
the static bridge, injects the dylib, repackages, re-opens every patched Mach-O,
validates the load command, and writes the final report.

During development, `expect: auto` records the displaced bytes for you. For a
release build, pin those exact four bytes so a future app update *fails loudly*
instead of patching the wrong code.

More hook recipes — constant return, blocking, argument/result modification,
Objective-C method replacement, in-place branch rewrites — are in
[examples/HookPatterns.md](examples/HookPatterns.md) and
[docs/Hooks.md](docs/Hooks.md).

## Updating to a new app version

IAOT is designed so an app update is a **path change, not a research project**:

1. Decrypt the new version yourself.
2. Change `paths.input_ipa` in `configs.yaml` — that is the only input, and it
   lives in exactly one place.
3. Build.

What survives an update automatically:

- **Managed (IL2CPP) hooks named by class + method** are re-resolved from the new
  IPA's own metadata at build time. No dump, no regenerated address table.
- **IL2CPP classes, methods, and fields used from your callbacks** are looked up
  **by name at runtime**. A renamed member resolves to null and is logged,
  instead of calling a stale address and crashing.
- **Signature-based hooks** report *by name* which byte patterns went stale, so
  you refresh them deliberately rather than discovering breakage on-device.

Nothing in the build reads a Cpp2IL/Il2CppDumper dump or a hardcoded address.
Use those tools to *research* new hooks; encode what you find as managed names or
signatures. See [docs/ManagedLocator.md](docs/ManagedLocator.md).

## Unity / IL2CPP apps

**The short version: IAOT never needs an IL2CPP dump, a native address, or a
field offset to build.**

- Hook a managed method by naming it, the way the game's own source does:
  `type: managed`, `method: "Example.Game.Player::ConsumePower(System.String, System.Int32)"`.
  IAOT resolves that to an address from the IPA's own `global-metadata.dat`.
- Inside your callbacks, find anything by name while the game runs, through the
  game's exported `il2cpp_*` API:

```cpp
using namespace iaot::il2cpp;
auto* klass  = FindClass("Example.Game", "Player");             // any assembly
auto* method = FindMethod(klass, "SetName", {"System.String"}); // overload-exact
auto  call   = reinterpret_cast<void (*)(Il2CppObject*, Il2CppString*, const MethodInfo*)>(
                   NativeMethodPointer(method));
call(player, String::New("Example Name"), method);
```

The resolver also handles fields (including fields on generic base classes),
properties, arrays, managed string creation, and `runtime_invoke`. If a player
strips the `il2cpp_*` exports, managed lookups degrade to null while your native
and Objective-C hooks keep working. Full details:
[docs/IL2CPP.md](docs/IL2CPP.md).

Build-time managed name resolution currently supports `global-metadata.dat`
**version 39** (Unity 2022/2023-era). Unsupported or encrypted metadata fails
with an actionable error and can fall back to byte signatures.

## What IAOT can and cannot do

### Can

- Secure IPA extraction and executable discovery (XML and binary plists).
- Evidence-based classification: Unity IL2CPP/Mono, Swift, Objective-C(++), C++,
  mixed, and uncertain-native — plus Unity version, `global-metadata.dat`
  version, and renderer (Metal/GL ES/UIKit/Unity/SceneKit/SpriteKit) detection.
- Target resolution by **managed name, byte signature, xref, symbol/export, or
  explicit VA/RVA.**
- Conservative static ARM64 redirects with callable originals, relocating B/BL,
  B.cond, CBZ/CBNZ, TBZ/TBNZ, ADR, ADRP, and integer literal loads.
- Runtime flags, callbacks, original trampolines, per-hook status, Objective-C
  method replacement, persistence, structured logging, and a minimal ImGui Metal
  overlay.
- IL2CPP access **by name** at runtime (classes, overload-exact methods, fields,
  native pointers, strings, invocation, internal calls).
- Multi-module patching, dylib embedding, `LC_LOAD_DYLIB` injection, IPA
  repackaging, reopen validation, and YAML reports — through one command.

### Cannot (by design, and honestly)

- **No decryption, no DRM bypass, no signing, no runtime injection, no process
  attachment, no jailbreak loader.**
- Static mutation accepts **thin, plain ARM64** `MH_EXECUTE`/`MH_DYLIB` images.
  Fat files and arm64e authenticated callback-pointer bridges are *detected and
  rejected*, not silently mishandled.
- One instruction is displaced per bridge. SIMD/FP literal loads, PRFM literals,
  and any unrecognized unsafe case **fail closed** rather than guessing.
- Bridges must fit their ARM64 branch range, and the header must have enough
  existing load-command padding. IAOT does not shift all sections to manufacture
  space.
- Typed originals support non-variadic AAPCS64 functions whose ABI *you* supply.
  A YAML string cannot prove a C++ ABI, struct-return convention, or proprietary
  object layout — a wrong signature is dangerous.
- Runtime-installed native/IL2CPP hooks are intentionally absent; they are
  unnecessary for the no-JIT design.

IAOT **fails closed** on ambiguous targets, unsupported architecture or
relocation, bad addresses, out-of-range branches, insufficient space, and
malformed input. See [docs/SafetyAndScope.md](docs/SafetyAndScope.md).

## Generated files and reports

Generated code and build artifacts are kept separate from your source. Every
generated C++ file starts with a do-not-edit header.

- **Generated glue:** `IAOTGeneratedConfig.hpp`, `Functions.hpp`, `Originals.hpp`,
  `Project.hpp`, `Hooks.mm`, `IAOTGeneratedProject.mk`.
- **Reports:** `ipa-analysis.yaml`/`.txt` (analysis), `hook-manifest.yaml`
  (configured hooks), `hook-preflight.yaml` (proof that resolution and space
  checks passed before building), and `final-validation.yaml`/`.txt` (the
  re-opened, verified output).

See [docs/GeneratedArtifacts.md](docs/GeneratedArtifacts.md) and
[docs/Troubleshooting.md](docs/Troubleshooting.md).

## Repository layout

```text
runtime/include/IAOT/      public runtime APIs
runtime/src/               runtime, diagnostics, persistence, IL2CPP resolver
runtime/overlay/           optional independent Metal/ImGui overlay
patcher/src/               analyzer, manifest, ARM64 bridge writer, IPA pipeline
templates/theos-dylib/     the project template `iaot new` copies
  user/                    files you edit
  generated/               generated glue; never hand-edit
examples/                  self-contained, synthetic examples (no real app data)
scripts/                   ImGui fetch and Windows/WSL build helpers
tests/                     portable C++ and synthetic Rust/IPA tests
docs/                      design, setup, hook, IL2CPP, and troubleshooting guides
```

## Documentation

| Topic | Guide |
| --- | --- |
| Install WSL/Theos/SDKs | [docs/Setup.md](docs/Setup.md) |
| How the pieces fit together | [docs/Architecture.md](docs/Architecture.md) |
| The jailed/no-JIT execution model | [docs/JailedNoJITModel.md](docs/JailedNoJITModel.md) |
| Writing hooks & locators | [docs/Hooks.md](docs/Hooks.md) · [examples/HookPatterns.md](examples/HookPatterns.md) |
| Naming IL2CPP targets at build time | [docs/ManagedLocator.md](docs/ManagedLocator.md) |
| Unity / IL2CPP runtime resolver | [docs/IL2CPP.md](docs/IL2CPP.md) |
| Project config file format | [docs/ManifestFormat.md](docs/ManifestFormat.md) |
| End-to-end pipeline stages | [docs/Workflow.md](docs/Workflow.md) |
| The Mach-O patcher internals | [docs/MachOPatcher.md](docs/MachOPatcher.md) · [docs/RuntimeConfig.md](docs/RuntimeConfig.md) |
| Typed app/original calls | [docs/AppFunction.md](docs/AppFunction.md) |
| The ImGui overlay | [docs/Overlay.md](docs/Overlay.md) |
| Packaging & signing | [docs/PackagingAndSigning.md](docs/PackagingAndSigning.md) |
| Bringing over existing work | [docs/ImportExistingWork.md](docs/ImportExistingWork.md) |
| Generated artifacts & reports | [docs/GeneratedArtifacts.md](docs/GeneratedArtifacts.md) |
| Safety, scope & failure modes | [docs/SafetyAndScope.md](docs/SafetyAndScope.md) |
| Testing & verification | [docs/Testing.md](docs/Testing.md) |
| When something goes wrong | [docs/Troubleshooting.md](docs/Troubleshooting.md) |

Two self-contained example projects live in `examples/` — both use **synthetic
symbols and impossible addresses** and target no real application.

## Building and testing IAOT itself

If you are hacking on IAOT (not using it), these are the checks CI-style
development relies on:

```powershell
cargo fmt --manifest-path patcher/Cargo.toml -- --check
cargo clippy --manifest-path patcher/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path patcher/Cargo.toml --locked
cmake -S . -B build-portable
cmake --build build-portable
ctest --test-dir build-portable -C Debug --output-on-failure
```

The Rust suite includes a complete **synthetic** IPA-to-IPA pipeline test. A real
decrypted IPA and signing identity can't be shipped as a fixture, so launching
and signing on a device remains a documented manual step (see
[docs/Testing.md](docs/Testing.md)).

## License

IAOT is [MIT licensed](LICENSE). Third-party components keep their own licenses;
see [Third-Party Notices](THIRD_PARTY_NOTICES.md). Dear ImGui is fetched at a
pinned tag by `scripts/fetch-imgui.*` and is not vendored into the repository.
