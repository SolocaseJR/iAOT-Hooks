# IAOT: iOS Ahead-of-Time Hooks

IAOT is a reusable C++17/Objective-C++ and Rust framework for offline instrumentation of decrypted iOS applications that you own or are authorized to modify. It targets jailed, no-JIT ARM64 deployments: hook callbacks, trampolines, and the runtime dylib are compiled before the final app is signed.

In simple words: iAOT is a framework for applying hooks and menu overlays to
iOS apps on jailed (non-jailbroken) devices. I made it because I found
virtually nothing explaining how to build toggleable mods and working hooks
for jailed, no-JIT iOS apps.

IAOT does not decrypt IPAs, bypass DRM, obtain signing identities, attach to installed processes, or install unsigned runtime code. See [Responsible Use](RESPONSIBLE_USE.md).

## What works

- Secure IPA extraction and `Info.plist` executable discovery, including XML and binary plists.
- Embedded framework/dylib inventory and ARM64/arm64e/fat architecture reporting.
- Evidence-based Unity IL2CPP, Unity Mono, Swift, Objective-C, Objective-C++, C++, mixed-native, and uncertain-native classification.
- Unity version and standard `global-metadata.dat` version detection where present.
- Metal, OpenGL ES, UIKit, Unity, SceneKit, and SpriteKit evidence reporting.
- Build-time IL2CPP managed-name locators that read the input IPA's own
  metadata, with signatures available as stale-pattern cross-checks.
- Cross-version signature/pattern and xref locators, plus symbol/export and
  explicit VA/RVA locators for native and instruction-level targets.
- Conservative ARM64 static redirects with callable originals for supported one-instruction entry bridges.
- Relocation of B/BL, B.cond, CBZ/CBNZ, TBZ/TBNZ, ADR, ADRP, and integer literal loads.
- Runtime flags, callbacks, original trampolines, isolated hook status, Objective-C method replacement, persistence, structured log levels, and a minimal ImGui Metal overlay.
- IL2CPP access by name at runtime: classes, overload-exact methods, fields (including on generic bases), native method pointers, strings, managed invocation, and internal calls. No dump, no addresses, nothing to regenerate per game version.
- Multi-module patching, dylib embedding, `LC_LOAD_DYLIB` injection, IPA repackaging, reopen validation, and YAML reports through one command.
- A Windows entry point that accepts a project `hooks.yaml`, reads its split
  configuration, builds the dylib in WSL/Theos, and runs the complete pipeline.

## Honest limits

- Static mutation currently accepts thin, plain ARM64 `MH_EXECUTE`/`MH_DYLIB` images. Fat files and arm64e callback-pointer bridges are detected and rejected.
- One instruction is displaced per bridge. Supported PC-relative families are relocated; SIMD/FP literal loads, PRFM literals, and any unrecognized unsafe case fail closed.
- The target-to-cave branch and relocated branches must fit their ARM64 ranges. IAOT does not claim a near-island strategy for every binary layout.
- The header must have enough existing load-command padding. IAOT does not shift all sections to manufacture space.
- Typed originals support non-variadic AAPCS64 functions whose exact ABI is supplied by the user. A manifest string cannot prove a C++ ABI, structure-return convention, or proprietary object layout.
- Runtime-installed native/IL2CPP hooks are intentionally absent. They are unnecessary for the no-JIT design and are not reliable enough to be the framework default.
- The runtime IL2CPP resolver uses the game's exported `il2cpp_*` API. A player that strips those exports gets no managed lookups (native and Objective-C hooks remain usable); capabilities degrade independently.
- IAOT removes stale code-signature commands from patched images but does not sign. The final app, frameworks, and dylib must be signed externally.

## Repository layout

```text
runtime/include/IAOT/      public runtime APIs
runtime/src/               runtime, diagnostics, persistence, and IL2CPP resolver
runtime/overlay/           optional independent Metal/ImGui overlay
patcher/src/               analyzer, manifest, ARM64 bridge writer, IPA pipeline
templates/theos-dylib/     reusable project template
  user/                    files a normal user edits
  generated/               generated glue; never hand-edit
examples/                  isolated generic examples
scripts/                   ImGui fetch and Windows/WSL build helpers
tests/                     portable C++ and synthetic Rust/IPA tests
docs/                      design, setup, hook, IL2CPP, and troubleshooting guides
```

## Quick start

Requirements are Rust/Cargo on the host, WSL2 Ubuntu for the Windows workflow, Theos with an iPhoneOS SDK, and an already decrypted authorized IPA. Full installation steps are in [Setup](docs/Setup.md).

```powershell
cargo install --path patcher
iaot new MyHooks
```

The only normal source files to edit are:

- `MyHooks/hooks.yaml`: hook-local flags, functions, and single-declaration
  patches; callback/trampoline slots are allocated automatically.
- `MyHooks/configs.yaml`: project identity, paths, output name, build settings,
  app selection/branding, and general analysis settings.
- `MyHooks/il2cpp_config.yaml`: optional IL2CPP analysis settings (backend and
  module). No dump or address table is needed.
- `MyHooks/user/UserHooks.mm`: typed callbacks.
- `MyHooks/user/UserMenu.mm`: the minimal ImGui menu.
- `MyHooks/user/UserFeatures.hpp`: shared toggle helpers.

Fetch the pinned Dear ImGui release once:

```powershell
./scripts/fetch-imgui.ps1
```

Analyze without changing the IPA:

```powershell
iaot analyze-ipa --in "C:\IPAs\OwnedApp-Decrypted.ipa" --reports .\build\analysis
```

Prepare target-aware generated code:

```powershell
iaot prepare --manifest .\MyHooks\hooks.yaml --in "C:\IPAs\OwnedApp-Decrypted.ipa" --reports .\build\reports
```

On Windows, run `build_iaot_ipa.bat .\MyHooks\hooks.yaml`; optionally provide an
input IPA as its second argument to override `configs.yaml`. Or run:

```powershell
.\scripts\build-iaot.ps1 -InputIpa "C:\IPAs\OwnedApp-Decrypted.ipa" -Manifest ".\MyHooks\hooks.yaml"
```

The script checks Cargo, WSL, the selected distribution, Theos, SDKs, compiler
tools, Python, Make, rsync, and ImGui when the menu is enabled; builds the
runtime in an isolated WSL directory; then calls the complete pipeline. Input,
output, build, and report paths default to `configs.yaml`; CLI parameters remain
explicit overrides.

Optional app branding is configured without editing the target manually:

```yaml
app:
  display_name: "My Instrumented App"
  icon_path: assets/app-icon.png
```

The icon source must be a square PNG at least 180x180 pixels; 1024x1024 is
recommended. IAOT removes alpha against an opaque white background, generates
the complete iPhone/iPad icon set, updates the primary icon declarations, and
updates localized `InfoPlist.strings` display names before repackaging.

For a freshly scaffolded project the defaults are:

```text
build/<project-name>-unsigned.ipa
generated/reports/final-validation.yaml
```

On Linux/WSL, build directly through the CLI:

```bash
iaot pipeline \
  --manifest ./MyHooks/hooks.yaml \
  --in ./OwnedApp-Decrypted.ipa \
  --out ./build/OwnedApp-IAOT-unsigned.ipa \
  --build-runtime \
  --reports ./build/reports
```

Or pass an already built dylib using `--dylib path/to/MyHooks.dylib`.

## Complete hook tutorial

The template contains a disabled example. Replace its locator and ABI before enabling it.

1. Analyze the IPA and read `ipa-analysis.yaml`. Confirm the selected executable/module, architecture, application type, Unity backend, and rendering evidence.
2. Add any feature flag needed by the hook:

```yaml
config:
  flags:
    - { name: example_hook_enabled, index: 0, default: false }
```

3. Declare the static hook once. Callback and original-trampoline slots are
   allocated automatically:

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

4. Implement the same ABI in `user/UserHooks.mm`:

```cpp
extern "C" int IAOT_ExampleHook(void* instance, int value) {
    auto original = iaot::generated::Original_ExampleHook();
    if (!Features::ExampleHookEnabled())
        return original.Call(instance, value);
    return 123;
}
```

5. Connect the menu checkbox to the same flag. The template already demonstrates the safe local-`bool` ImGui pattern and persistence.
6. Run the Windows script or `iaot pipeline`. The pipeline analyzes, generates, builds or accepts the dylib, applies static bridges, injects the dylib, repackages, reopens every patched Mach-O, validates the load command, and writes the final report.
7. Sign/re-sign the complete IPA externally and install it using an authorized workflow.

`expect: auto` records the displaced bytes for development. Pin those exact four bytes for a release build so an application update fails before mutation.

For a state-preserving in-place ARM64 branch rewrite, use
`branch_to_instruction_target`. The patcher reads an existing B/BL, B.cond,
CBZ/CBNZ, or TBZ/TBNZ instruction and calculates its destination from the
encoded PC-relative immediate. It then replaces the selected instruction with
an unconditional branch to that destination:

```yaml
  - name: ExampleCheckedEpilogue
    enabled: true
    description: "Skip the positive path after the checked state is recorded"
    module: owned_module
    failure_behavior: abort
    architectures: [arm64]
    locator:
      type: pattern
      pattern: "00 00 00 34 AA BB CC DD"
      expected_matches: 1
      segment: __TEXT
      section: __text
    patch: { type: branch_to_instruction_target }
```

By default the target is decoded from the instruction at the matched patch
site. Set `instruction_offset` to a signed byte offset when a nearby branch
encodes the desired destination, for example
`{ type: branch_to_instruction_target, instruction_offset: -12 }`.

## Updating to a new app version

IAOT is built so an update is a path change, not a research project:

1. Decrypt the new version yourself.
2. Change `paths.input_ipa` in `configs.yaml`. It is the only input and is
   defined only there.
3. Build. A `managed` locator resolves the method from the new IPA. A
   `signatures` locator with `managed:` and `use_managed: true` uses that name
   as the target and reports which byte signatures became stale. Native and
   instruction-level signature-only hooks must still match the updated code.
   `hook-preflight.yaml` records how every patch was found.
4. Managed classes, methods and fields used from callbacks are looked up by
   name at runtime, so they need no generated address table. A renamed member
   resolves to null and is logged instead of calling a stale address.

Nothing in this workflow reads a Cpp2IL/Il2CppDumper dump or a hardcoded
address from a previous version. Use those tools to research new hooks; encode
IL2CPP method-entry hooks as managed names, optionally retain signatures as
cross-checks, and use signature-only locators for native or internal
instruction sites. Build-time managed lookup currently supports metadata
version 39; unsupported or encrypted metadata fails with an actionable error
and can fall back to supplied signatures.

## Static hooks versus runtime IL2CPP

Static hooks are resolved offline and written before signing. They never wait
for IL2CPP initialization. On a supported IL2CPP app, prefer a managed name for
a method-entry hook and optionally retain signatures as cross-checks. Use
signature locators for native functions and instruction sites inside a method.
See [Hooks](docs/Hooks.md) and [Managed-name locator](docs/ManagedLocator.md).

The runtime IL2CPP resolver is used inside an already-installed callback and
finds everything by name:

```cpp
using namespace iaot::il2cpp;
auto* klass = FindClass("Example.Game", "Player");          // any assembly
auto* method = FindMethod(klass, "SetName", {"System.String"});
auto call = reinterpret_cast<void (*)(Il2CppObject*, Il2CppString*, const MethodInfo*)>(
    NativeMethodPointer(method));
call(player, String::New("Example Name"), method);
```

It also supports fields (including fields of generic base classes through
`FindObjectField`), properties, arrays, and `runtime_invoke`. Resolver failure
does not disable static native hooks, Objective-C hooks, persistence, or the
overlay. See [IL2CPP](docs/IL2CPP.md).

## Reports

Generated artifacts are separate from user code:

- `ipa-analysis.yaml` and `.txt`
- `il2cpp-methods.yaml` skeleton when IL2CPP is detected (only for the optional pinned `il2cpp` locator)
- `generated/hook-manifest.yaml`
- `IAOTGeneratedConfig.hpp`, `Functions.hpp`, `Originals.hpp`, `Project.hpp`, and `Hooks.mm`
- `IAOTGeneratedProject.mk` and `hook-preflight.yaml`
- `final-validation.yaml` and `.txt`

See [Generated Artifacts](docs/GeneratedArtifacts.md) and [Troubleshooting](docs/Troubleshooting.md).

## Development verification

```powershell
cargo fmt --manifest-path patcher/Cargo.toml -- --check
cargo clippy --manifest-path patcher/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path patcher/Cargo.toml --locked
cmake -S . -B build-portable
cmake --build build-portable
ctest --test-dir build-portable -C Debug --output-on-failure
```

The Rust suite includes a synthetic complete IPA-to-IPA pipeline. A real decrypted IPA and iOS signing identity cannot be distributed as a test fixture, so device launch/signing remains a documented manual validation step.

IAOT is MIT licensed. Third-party components retain their own licenses; see [Third-Party Notices](THIRD_PARTY_NOTICES.md).
