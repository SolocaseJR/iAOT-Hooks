# Unity and IL2CPP

## Detection

The analyzer combines `UnityFramework.framework`, Unity data paths, standard `global-metadata.dat`, Mono/Managed files, IL2CPP exports/registration strings, imports, and module inventory. It reports evidence and uncertainty. `analysis.scripting_backend`, `application_type`, and `il2cpp_module` can override unusual layouts.

Metadata version is read only when the standard `0xFAB11BAF` header is present. Transformed metadata is reported, not guessed.

## Static analysis and static hooks

Primary hooks are installed offline. A runtime resolver is never required to generate their branch.

IAOT's stable adapter file is `il2cpp-methods.yaml`:

```yaml
schema_version: 1
methods:
  - id: "Assembly-CSharp::Example.Player::SetName/1"
    assembly: Assembly-CSharp
    namespace: Example
    class: Player
    method: SetName
    parameter_count: 1
    module: il2cpp
    rva: 305419896
```

A locator may use the full identity or `method_id`. Exactly one entry must match. Each entry contains exactly one audited `rva` or `vmaddr`. The patcher validates executable membership and alignment after resolving it.

[Cpp2IL/LibCpp2IL](https://github.com/SamboyCoding/Cpp2IL) is the preferred optional analyzer candidate as of the recorded audit because its active pre-release line includes an IPA handler, Mach-O/ARM64 work, and newer metadata-version support. Its output must be converted to the stable IAOT map. [Il2CppDumper](https://github.com/Perfare/Il2CppDumper) remains a useful MIT compatibility source for Mach-O and disassembler scripts but its published compatibility is older. [Il2CppInspector](https://github.com/djkaty/Il2CppInspector) has suspended development and is AGPL, so it is neither vendored nor the default.

IAOT does not silently download or execute analyzers. Pin the external tool/version in your build environment, retain its license, inspect its output, and commit only authorized maps.

## Runtime resolver decision

[Batchhh/IOS-Il2cppResolver](https://github.com/Batchhh/IOS-Il2cppResolver) was archived in April 2026 and uses a hardcoded `BINARY_NAME` plus fixed wait-time model. IAOT therefore does not copy it. The integrated resolver is a small, maintained exported-API layer with automatic module discovery, dyld notifications, background bounded waiting, capability flags, and no game-specific values.

When exports exist it supports:

- assembly/image, namespace/class, nested-class, method, field, and property lookup;
- overload-safe method enumeration by name and parameter count when the enumeration exports are available (ambiguous same-count overloads return null instead of guessing);
- static and instance field read/write and offsets;
- managed string creation and UTF-16 to UTF-8 conversion;
- `il2cpp_runtime_invoke` for static or instance methods;
- `il2cpp_resolve_icall` for common UnityEngine APIs;
- thread attachment where exported.

### Stripped exported API fallback

Shipping Unity IL2CPP builds can contain the complete managed-string runtime
while omitting the public `il2cpp_string_new`/domain API exports. For that
case, configure the audited RVA of the target's generated UTF-16
`System.String.CreateString(Char*)` helper:

```yaml
runtime:
  string_factory_rva: 0x8595F8C
```

The runtime first uses exported `il2cpp_string_new` when available. If it is
absent, it converts UTF-8 to UTF-16 and calls the configured in-image factory.
The RVA is target- and version-specific; it must come from an authorized dump
and must not be guessed or copied between app versions. The generated runtime
also normalizes `analysis.il2cpp_module: main` to the actual executable image
name before starting module discovery.

For heavily stripped or transformed builds, the offline resolver can consume
the same kind of audited output produced by a Cpp2IL analysis package. The
An audited metadata package can contain native method addresses plus managed
field offsets. The generated bindings use those values
to provide `FindClass`, `FindMethod`, `FindField`, typed method pointers, field
access, and internal-call addresses without pretending that the public
`il2cpp_*` exports exist. This follows the metadata-loader investigation model
described in Katy's IL2CPP loader analysis: the app still initializes its own
metadata cache; stripping only removes the convenient exported inspection API.

The offline handles are deliberately versioned. A new binary must regenerate
and audit the bindings; strings or a previous build's offsets are not a safe
substitute.

```cpp
auto* player = iaot::il2cpp::FindClass("Assembly-CSharp", "Example", "Player");
auto* method = iaot::il2cpp::FindMethod(player, "GetDisplayName", 0);
auto* result = iaot::il2cpp::Invoke(method, instance);
auto text = iaot::il2cpp::String::ToUtf8(reinterpret_cast<iaot::il2cpp::Il2CppString*>(result));
```

No stable exported API returns a native `MethodInfo` function pointer on every
Unity version. Without offline bindings, `NativeMethodPointer` therefore
returns zero unless the user supplies an explicitly versioned
`MethodPointerAdapter`. Runtime-installed hooks are not implemented. This is
intentional for the jailed/no-JIT premise.

## Offline metadata dumps (build-time name resolution)

Hand-typing native RVAs and struct field offsets into a manifest is fragile: a
game update shifts every address and the stale numbers fail silently. Instead,
point the manifest at a **metadata dump** produced by an offline static-analysis
tool and let `iaot generate` resolve every offline entry **by name**. The
resolved numbers are baked into the generated offline tables; a moved address
then surfaces as a loud `method ... not found` / `is ambiguous` error at build
time rather than a silent runtime failure.

The split layout keeps these keys under `runtime:` in
`il2cpp_config.yaml`:

```yaml
runtime:
  # A directory or a single file. Format is auto-detected from the file names.
  metadata_package: "Game Data & Dumps/Cpp2IL_AnalysisPackage"
  # Optional override: auto (default) | cpp2il | il2cpp_dumper
  metadata_format: auto
```

The file is selected from `configs.yaml`:

```yaml
il2cpp:
  enabled: true
  config_file: il2cpp_config.yaml
```

When `enabled` is false, IAOT does not open the IL2CPP config or its metadata
package.

Any `offline_methods`, `offline_fields`, `offline_internal_calls`, and
`string_factory` entry may then be **name-only** (no `rva:` / `offset:`). An
explicit value in the manifest always wins over dump resolution, so you can pin
a single tricky entry by hand while resolving the rest by name.

### Supported dump formats

You only need a **small subset** of a tool's output — usually one or two files.

**Cpp2IL analysis package** (`--output-as=analysis-package`; recommended, and the
format iAOT is primarily tested against). Provide the directory or either file:

| File | Provides | Needed when |
| --- | --- | --- |
| `cpp2il_methods.jsonl` | method native addresses, assembly, declaring type, parameter types | any `offline_methods` / `offline_internal_calls` / `string_factory` |
| `cpp2il_types.jsonl` | per-field name, offset, size | any `offline_fields` |

The rest of the package (`cpp2il_calls.jsonl`, `cpp2il_pseudocode.jsonl`, the
`.h` headers, the IDA/Ghidra `.py` scripts) is **not read** — ship only the two
`.jsonl` files. This format carries assemblies and managed parameter types, so
it is the only one that can disambiguate overloads by `parameter_types`.

**Il2CppDumper** (Perfare's tool — "the IDA/Ghidra dump"). Provide the directory
or a single file:

| File | Provides | Needed when |
| --- | --- | --- |
| `script.json` | method addresses keyed by `Type$$Method` (the file its `ida.py`/`ghidra.py` consume) | `offline_methods` / most `offline_internal_calls` |
| `dump.cs` | methods **and** instance field offsets (`// RVA: 0x..`, `// 0x..`) | `offline_fields` (script.json has none), or methods without a script.json |

If both files are present, `script.json` supplies methods and `dump.cs` supplies
field offsets. If a project has `offline_fields` but the folder has only
`script.json`, `iaot generate` fails with an explicit message telling you to add
`dump.cs`, switch to a Cpp2IL package, or hand-specify the offsets.

Because `script.json`/`dump.cs` carry no managed assembly names and no managed
parameter types, overloads are disambiguated only by `parameter_count`, and the
`INTERNAL_CALL_*` internal-call thunk names (a Cpp2IL concept) resolve only via
their plain method name. Use a Cpp2IL package when you need that precision.

### The RVA convention (important)

For the common `0x1_0000_0000` arm64 image base, each generated RVA is
`native_VA - 0x1_0000_0000`. Mach-O images whose `__TEXT` vmaddr is zero already
carry an image-relative value in Cpp2IL's `address`, so iAOT preserves that value.
The runtime adds the real load slide in either case. This deliberately avoids
Cpp2IL's `rva` JSON field, which some packages make relative to the tool's
`primaryExecutableAddress` after the Mach-O header.
Il2CppDumper's `Address`/`RVA` are already image-base-relative and used as-is.

The implementation and its unit tests live in
[`patcher/src/il2cpp_metadata.rs`](../patcher/src/il2cpp_metadata.rs).

### Regenerate per game version

Offline handles are deliberately version-specific. A new binary must regenerate
and re-audit the dump; a previous build's addresses or another app's dump are
never a safe substitute. The dumps are large (hundreds of MB) and are kept out
of the framework — regenerate them per game version with your analyzer.

## Compatibility and degradation

Status is one of not applicable, not detected, waiting for module, waiting for IL2CPP, initializing, ready, partially available, unsupported version, metadata missing, runtime functions unavailable, method unresolved, or initialization failed. Capabilities are independent bits. An IL2CPP failure never unregisters native hooks or disables the menu.
