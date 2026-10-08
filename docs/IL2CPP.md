# Unity and IL2CPP

## The short version

IAOT never needs an IL2CPP dump, a native address, or a field offset to build.

- **Managed method-entry hooks** are found by name from the input IPA's own
  metadata. Byte signatures can cross-check that result. Native hooks and
  instruction sites inside a managed method use signatures; see
  [Hooks](Hooks.md) and [Managed-name locator](ManagedLocator.md).
- **Managed classes, methods and fields** are found by name while the game
  runs, through the game's own exported `il2cpp_*` API. Names survive updates
  far better than addresses, and a missing name returns null instead of a
  stale address.

Updating to a new app version is therefore: decrypt the new IPA, point
`paths.input_ipa` in `configs.yaml` at it, and build. If something changed,
the build or the runtime log names the exact signature or member.

Tools such as Cpp2IL, Il2CppDumper, IDA or Ghidra are still the best way to
*discover* what to hook or call when you add a feature. Their output is
research material only; nothing in the build reads it.

## Detection

The analyzer combines `UnityFramework.framework`, Unity data paths, standard `global-metadata.dat`, Mono/Managed files, IL2CPP exports/registration strings, imports, and module inventory. It reports evidence and uncertainty. `analysis.scripting_backend`, `application_type`, and `il2cpp_module` can override unusual layouts.

Metadata version is read only when the standard `0xFAB11BAF` header is present. Transformed metadata is reported, not guessed.

## Static hooks

Primary hooks are installed offline. A runtime resolver is never required to
generate their branch. Prefer a managed name for an IL2CPP method entry and
use `signatures` locators for native functions or instruction sites inside a
method.

A hook can instead name its target by managed class and method; IAOT resolves
that name to an address at build time from the `global-metadata.dat` and IL2CPP
module inside the IPA being patched, with no dump and nothing to regenerate per
version. A name can also be paired with signatures as a mutual cross-check. See
[Managed-name locator](ManagedLocator.md).

An `il2cpp` locator that reads an audited `il2cpp-methods.yaml` map still
exists for one-off pinned builds, but its RVAs belong to one binary and must be
regenerated for every update. Prefer signatures or managed names.

## Runtime resolver

[Batchhh/IOS-Il2cppResolver](https://github.com/Batchhh/IOS-Il2cppResolver) was archived in April 2026 and uses a hardcoded `BINARY_NAME` plus fixed wait-time model. IAOT therefore does not copy it. The integrated resolver is a small, maintained exported-API layer with automatic module discovery, dyld notifications, background bounded waiting, capability flags, and no game-specific values.

It waits until the IL2CPP runtime has booted (the corlib image is published),
then supports:

- class lookup by namespace and name, in a named assembly or in every loaded
  assembly; nested types as `Outer+Inner` or `Outer/Inner`;
- method lookup by name and parameter count, searching base classes too, with
  overload-exact lookup by parameter type names;
- field lookup on a class or directly on an object's live class (required for
  fields declared on generic bases such as `Player`1` or `List`1`), static and
  instance reads/writes, and offsets;
- native method entry points (`MethodInfo::methodPointer`) for direct calls;
- managed array element size/class;
- managed string creation and UTF-16 to UTF-8 conversion;
- `il2cpp_runtime_invoke`, `il2cpp_resolve_icall`, GC handles, and thread
  attachment where exported.

```cpp
using namespace iaot::il2cpp;

// The assembly name is optional; a null or non-assembly name searches all.
auto* player = FindClass("Example.Game", "Player");

// Same-arity overloads: name the parameter types.
auto* setScore = FindMethod(player, "SetScore", {"System.String", "System.Int64"});
using SetScoreFn = void (*)(Il2CppObject*, Il2CppString*, std::int64_t, const MethodInfo*);
auto call = reinterpret_cast<SetScoreFn>(NativeMethodPointer(setScore));
call(instance, String::New("best"), 42, setScore);

// Fields on generic bases: go through the object.
Il2CppObject* account = nullptr;
if (auto* field = FindObjectField(instance, "<Account>k__BackingField"))
  Field::GetInstance(instance, field, account);
```

Pass the `MethodInfo` as the final argument of a direct call. Shared generic
code reads its generic context from it; non-generic code ignores it.

`FindMethod(klass, name, count)` returns null and logs when several overloads
share that arity. Use the parameter-type form to pick one. Type names are the
managed full names (`System.Char*`, `System.Object[]`,
`UnityEngine.UI.DefaultControls+Resources`); a name without a namespace
(`Int32`) matches by simple name.

### Value-type layouts

`Field::Offset` is relative to an object, including its 16-byte header.
Elements of a struct array (for example `Dictionary`2+Entry[]`) have no
header, so use `Field::ValueOffset` on the element class's fields and
`Array::ElementSize` for the stride:

```cpp
auto* entryClass = Array::ElementClass(entries);
const auto stride = Array::ElementSize(entries);
const auto valueAt = Field::ValueOffset(FindField(entryClass, "value"));
```

### Stripped string export

Some players omit `il2cpp_string_new`. `String::New` then calls the game's own
`System.String.Ctor(char*, int, int)` (or `CreateString`), found by name and
parameter types like any other method. No configuration is needed.

### What the runtime does not do

Runtime-installed native hooks are not implemented. This is intentional for
the jailed/no-JIT premise; hooks are written into the binary before signing.

## Upgrading older projects

Earlier versions resolved names against a Cpp2IL/Il2CppDumper dump at build
time and baked the resulting addresses into the dylib. Build-time name
resolution is back, but it reads the IPA's own metadata rather than a dump; see
[Managed-name locator](ManagedLocator.md). The old dump settings stay gone:
they are ignored with a warning, and the dump folder is no longer read.

- `runtime.metadata_package`, `runtime.metadata_format`
- `runtime.string_factory`, `runtime.string_factory_rva`
- `runtime.offline_classes`, `offline_methods`, `offline_fields`,
  `offline_internal_calls`
- `fallback` / `fallback_rva` on patch locators

Delete them. Code that called `OfflineMethodPointer` should call
`FindMethod` + `NativeMethodPointer`; code that read raw field offsets should
use `FindObjectField`.

## Compatibility and degradation

Status is one of not applicable, not detected, waiting for module, waiting for IL2CPP, initializing, ready, partially available, unsupported version, metadata missing, runtime functions unavailable, method unresolved, or initialization failed. Capabilities are independent bits. An IL2CPP failure never unregisters native hooks or disables the menu.
