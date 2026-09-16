# Hook Development

## Locators

Supported static locators are `pattern`, `xref`, `va`/`vmaddr`, `rva`, `symbol`, `export`, and `il2cpp`. A `custom` locator is a named extension point but is rejected until an offline adapter implements it; arbitrary code from YAML is never executed.

Patterns use two-digit bytes, `?`/`??` wildcards, and `||` alternatives. They search executable segments by default. `segment`, `section`, `expected_matches`, and `match_index` make intent explicit. Multiple matches without an index fail.

For a version-pinned build, a pattern locator may also declare
`fallback_rva`. The patcher uses it only when pattern resolution fails (no
match, an unexpected match count, or an ambiguous match). The resolved RVA is
still subjected to normal alignment, range, architecture, version, and
`expect` validation.

Hardcoded VM addresses and RVAs are version-dependent. Use `versions` and pinned `expect` bytes for build-specific definitions.

## Replacement with callable original

Declare the replacement directly on the patch. IAOT creates and registers the
callback, assigns the first free callback slot, wires `branch_to_callback`, and,
when `preserve_original` is true, assigns the first free trampoline slot. The
generator exposes `iaot::generated::Original_<PatchName>()`.

```yaml
patches:
  - name: AdjustValue
    enabled: true
    description: Authorized example value adjustment
    replacement: IAOT_AdjustValue
    signature: "float(void*, float)"
    preserve_original: true
    module: main
    failure_behavior: disable_hook
    locator:
      type: pattern
      pattern: "FD 7B BF A9 F4 4F 01 A9"
      expected_matches: 1
      segment: __TEXT
      section: __text
    patch:
      type: branch_to_cave
    expect: auto
```

```cpp
extern "C" float IAOT_AdjustValue(void* object, float value) {
    auto original = iaot::generated::Original_AdjustValue();
    if (!Features::Enabled()) return original.Call(object, value);
    value *= 2.0f;
    float result = original.Call(object, value);
    return result + 1.0f;
}
```

Returning a constant, returning from `void`, skipping the original, modifying arguments, inspecting/modifying a result, and conditionally blocking are ordinary C++ variations of this form. See [Generic Examples](../examples/HookPatterns.md).

`callback_index` and `original_slot` remain available as advanced overrides,
and the legacy top-level `config.callbacks` plus explicit
`action.callback` form remains supported for existing manifests. New hooks
should normally omit all three indices.

## Other fixed actions

- `ret_if_flag`: return immediately when a flag is true.
- `zero_register_if_flag`: set a selected general register to zero.
- `callback_if_flag`: call an observational callback and then continue.
- `callback_return_zero_if_flag`: observational callback followed by zero return.
- `callback_after`: relocate the displaced instruction, observe, then continue.
- `branch_to_replacement`: tail-branch to a symbol already in the target image.

These are lower-level fixed templates. Typed user hooks should normally use `branch_to_callback`.

## Objective-C

Objective-C method replacement happens at runtime through `method_setImplementation`, which is safer than patching a dynamic Objective-C IMP entry offline. Declare `kind: objc`, `class`, `selector`, `class_method`, `signature`, `replacement`, and `preserve_original` under `functions`. The generator registers the replacement and exposes `iaot::generated::Original_<Name>()`. The replacement signature includes `self` and `_cmd` and must match the real method ABI exactly.

```yaml
functions:
  - name: ExampleTitle
    kind: objc
    class: ExampleController
    selector: title
    class_method: false
    signature: "id(id, SEL)"
    replacement: IAOT_ExampleTitle
    preserve_original: true
```

Objective-C replacement differs from native/IL2CPP static patching: the class/selector is resolved by the Objective-C runtime after image load, while a native or IL2CPP method implementation address is redirected offline. IAOT installs available hooks from its image constructor and retries only failed entries as later dyld images arrive. Ready hooks are never reinstalled, which prevents a replacement from being captured as its own original IMP.

Projects that must establish process-local state before callback slots are
published may optionally export a synchronous preflight:

```cpp
extern "C" void IAOT_RunEarlyPreflight() {
    // Keep this bounded and independent of UIApplication/UIScene readiness.
}
```

The symbol is weak and is ignored when absent. UI setup belongs in the overlay
lifecycle, which waits for a foreground scene rather than delaying all hooks.

## Safety

Never call a null original pointer. `AppFunction::Call` returns a default value and logs when unresolved, but that is a last safety net, not ABI validation. Do not dereference application objects without an authorized, version-matched layout.
