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

```cpp
auto* player = iaot::il2cpp::FindClass("Assembly-CSharp", "Example", "Player");
auto* method = iaot::il2cpp::FindMethod(player, "GetDisplayName", 0);
auto* result = iaot::il2cpp::Invoke(method, instance);
auto text = iaot::il2cpp::String::ToUtf8(reinterpret_cast<iaot::il2cpp::Il2CppString*>(result));
```

No stable exported API returns a native `MethodInfo` function pointer on every Unity version. `NativeMethodPointer` therefore returns zero unless the user supplies an explicitly versioned `MethodPointerAdapter`. Runtime-installed hooks are not implemented. This is intentional for the jailed/no-JIT premise.

## Compatibility and degradation

Status is one of not applicable, not detected, waiting for module, waiting for IL2CPP, initializing, ready, partially available, unsupported version, metadata missing, runtime functions unavailable, method unresolved, or initialization failed. Capabilities are independent bits. An IL2CPP failure never unregisters native hooks or disables the menu.
