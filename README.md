# iOS Ahead-Of-Time Hooks (IAOT)

IAOT is a public, C++-first instrumentation SDK/template for **iOS apps you own or are explicitly authorized to test**. It targets jailed, non-jailbroken devices: hook functions are compiled ahead of time, included in the app package, and signed before execution. Runtime configuration may select data and enable precompiled behavior; it never creates native code.

IAOT is not an injector, jailbreak tweak, JIT, bypass toolkit, DRM circumvention tool, or a way to attach to arbitrary installed apps. It has no dependency on Substrate, Substitute, ElleKit, Frida server, `task_for_pid`, writable-and-executable memory, or unsigned code.

## Model

1. Write C++/Objective-C++ hook functions under `hooks/`.
2. Generate explicit registration and typed function wrappers from `hooks.yaml`.
3. Build those functions and the source-distributed runtime with Theos (or directly in an owned host target).
4. Embed the result in the authorized app package and sign/re-sign the complete package.
5. At runtime, signed code can read fixed-layout config, choose values, and call existing signed app functions.

Changing a string or flag at runtime is supported. Adding or compiling a new native hook at runtime is not.

## Quickstart

```sh
cargo install --path patcher
iaot new MyHooks
cd MyHooks
iaot generate hooks.yaml
make
```

Theos produces a dylib; that alone is not deployable on jailed iOS. Embed it in an app you are authorized to modify, ensure it is loaded by that app, then sign/re-sign the final package using your normal authorized workflow.

See the complete [`OwnedDemoProject`](examples/OwnedDemoProject/README.md), which demonstrates a runtime-selected return string, Objective-C method replacement, generated hook registration, and typed calls such as:

```cpp
auto value = iaot::generated::ReturningSomeString().Call();
auto get = iaot::AppFunction<int(void*)>::FromVmAddr(0x100123456);
int result = get.Call(object);
```

Addresses in the public demo are intentionally fake/demo-only. A wrong address or C++ ABI signature can crash.

## Components

- `runtime/`: C++17/Objective-C++ source library; no Rust runtime dependency.
- `patcher/`: optional Rust `iaot` CLI for projects, manifests, generation, inspection, conservative offline Mach-O patching, and generic IPA member replacement.
- `templates/`: Theos dylib starter.
- `examples/OwnedDemoProject/`: owned demo symbols only.
- `docs/`: architecture, no-JIT boundaries, manifest, packaging, and safety details.

`patch-macho` supports thin ARM64 `MH_EXECUTE` files, audited cave templates, config/cave segment insertion, stale signature removal, redirects, and reopen verification. It deliberately refuses unsafe displaced instructions, ambiguous matches, out-of-range branches, overlapping segments, and binaries without enough load-command padding. `repack-ipa` stages one explicitly named member and does not sign it.

## Development checks

```sh
cargo test --manifest-path patcher/Cargo.toml
cmake -S . -B build
cmake --build build
ctest --test-dir build
```

The portable tests cover config layout/access, hook order, and typed function calls. Apple runtime sources are built by Theos on macOS/iOS toolchains.

Licensed under MIT. Authorization, signing, permissions, and legal compliance remain the user's responsibility.
