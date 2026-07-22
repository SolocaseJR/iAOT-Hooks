# IAOT: iOS Ahead-of-Time Hooks

IAOT is a reusable C++17/Objective-C++ and Rust framework for offline instrumentation of decrypted iOS applications that you own or are authorized to modify. It targets jailed, no-JIT ARM64 deployments: hook callbacks, trampolines, and the runtime dylib are compiled before the final app is signed.

IAOT does not decrypt IPAs, bypass DRM, obtain signing identities, attach to installed processes, or install unsigned runtime code. See [Responsible Use](RESPONSIBLE_USE.md).

## What works

- Secure IPA extraction and `Info.plist` executable discovery, including XML and binary plists.
- Embedded framework/dylib inventory and ARM64/arm64e/fat architecture reporting.
- Evidence-based Unity IL2CPP, Unity Mono, Swift, Objective-C, Objective-C++, C++, mixed-native, and uncertain-native classification.
- Unity version and standard `global-metadata.dat` version detection where present.
- Metal, OpenGL ES, UIKit, Unity, SceneKit, and SpriteKit evidence reporting.
- Pattern, VA, VM address, RVA, symbol/export, xref, and offline IL2CPP method-manifest locators.
- Conservative ARM64 static redirects with callable originals for supported one-instruction entry bridges.
- Relocation of B/BL, B.cond, CBZ/CBNZ, TBZ/TBNZ, ADR, ADRP, and integer literal loads.
- Runtime flags, callbacks, original trampolines, isolated hook status, Objective-C method replacement, persistence, structured log levels, and a minimal ImGui Metal overlay.
- Optional IL2CPP exported-API resolution for metadata lookup, strings, fields, properties, managed invocation, and internal calls.
- Multi-module patching, dylib embedding, `LC_LOAD_DYLIB` injection, IPA repackaging, reopen validation, and YAML reports through one command.
- A drag-and-drop Windows entry point that builds the dylib in WSL/Theos and runs the complete pipeline.

## Honest limits

- Static mutation currently accepts thin, plain ARM64 `MH_EXECUTE`/`MH_DYLIB` images. Fat files and arm64e callback-pointer bridges are detected and rejected.
- One instruction is displaced per bridge. Supported PC-relative families are relocated; SIMD/FP literal loads, PRFM literals, and any unrecognized unsafe case fail closed.
- The target-to-cave branch and relocated branches must fit their ARM64 ranges. IAOT does not claim a near-island strategy for every binary layout.
- The header must have enough existing load-command padding. IAOT does not shift all sections to manufacture space.
- Typed originals support non-variadic AAPCS64 functions whose exact ABI is supplied by the user. A manifest string cannot prove a C++ ABI, structure-return convention, or proprietary object layout.
- Runtime-installed native/IL2CPP hooks are intentionally absent. They are unnecessary for the no-JIT design and are not reliable enough to be the framework default.
- The runtime IL2CPP resolver depends on exported `il2cpp_*` APIs. Stripped apps may provide partial or no runtime capabilities; native and Objective-C hooks remain usable.
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

- `MyHooks/hooks.yaml`: centralized paths, detection overrides, flags, callbacks, functions, and static patches.
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

On Windows, drag the IPA onto `build_iaot_ipa.bat`, or run:

```powershell
.\scripts\build-iaot.ps1 -InputIpa "C:\IPAs\OwnedApp-Decrypted.ipa" -Manifest ".\MyHooks\hooks.yaml"
```

The script checks Cargo, WSL, the selected distribution, Theos, SDKs, compiler tools, Python, Make, rsync, and ImGui when the menu is enabled; builds the runtime in an isolated WSL directory; then calls the complete pipeline. The output defaults to:

```text
build/windows/<input-name>-IAOT-unsigned.ipa
build/windows/reports/final-validation.yaml
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
2. Add a callback slot and flag in `hooks.yaml`:

```yaml
config:
  flags:
    - { name: example_hook_enabled, index: 0, default: false }
  callbacks:
    - name: example_callback
      index: 0
      symbol: IAOT_ExampleHook
      signature: "int(void*, int)"
      auto_register: true
```

3. Add the static bridge:

```yaml
patches:
  - name: ExampleHook
    enabled: true
    description: "Authorized example entry hook"
    module: main
    signature: "int(void*, int)"
    original_slot: 0
    failure_behavior: disable_hook
    locator:
      type: pattern
      pattern: "FD 7B BF A9 F4 4F 01 A9"
      expected_matches: 1
      segment: "__TEXT"
      section: "__text"
    patch:
      type: branch_to_cave
      action: { type: branch_to_callback, callback: example_callback }
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

## Static hooks versus runtime IL2CPP

Static hooks are resolved offline and written before signing. They never wait for IL2CPP initialization. For IL2CPP name-based static hooks, import an audited method map and use `locator.type: il2cpp`; see [IL2CPP](docs/IL2CPP.md).

The runtime resolver is additional functionality used inside an already-installed callback:

```cpp
std::string current = iaot::il2cpp::String::ToUtf8(originalName);
auto* replacement = iaot::il2cpp::String::New("Example Name");
```

It also supports class/method/field/property lookup and `runtime_invoke` when the required exports exist. Resolver failure does not disable static native hooks, Objective-C hooks, persistence, or the overlay.

## Reports

Generated artifacts are separate from user code:

- `ipa-analysis.yaml` and `.txt`
- `il2cpp-methods.yaml` skeleton when IL2CPP is detected
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
ctest --test-dir build-portable --output-on-failure
```

The Rust suite includes a synthetic complete IPA-to-IPA pipeline. A real decrypted IPA and iOS signing identity cannot be distributed as a test fixture, so device launch/signing remains a documented manual validation step.

IAOT is MIT licensed. Third-party components retain their own licenses; see [Third-Party Notices](THIRD_PARTY_NOTICES.md).
