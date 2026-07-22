# Hook Development

## Locators

Supported static locators are `pattern`, `xref`, `va`/`vmaddr`, `rva`, `symbol`, `export`, and `il2cpp`. A `custom` locator is a named extension point but is rejected until an offline adapter implements it; arbitrary code from YAML is never executed.

Patterns use two-digit bytes, `?`/`??` wildcards, and `||` alternatives. They search executable segments by default. `segment`, `section`, `expected_matches`, and `match_index` make intent explicit. Multiple matches without an index fail.

Hardcoded VM addresses and RVAs are version-dependent. Use `versions` and pinned `expect` bytes for build-specific definitions.

## Replacement with callable original

Use `branch_to_callback`, an exact `signature`, and a unique `original_slot`. The generator creates `iaot::generated::Original_<PatchName>()`.

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

Objective-C replacement differs from native/IL2CPP static patching: the class/selector is resolved by the Objective-C runtime after image load, while a native or IL2CPP method implementation address is redirected offline.

## Safety

Never call a null original pointer. `AppFunction::Call` returns a default value and logs when unresolved, but that is a last safety net, not ABI validation. Do not dereference application objects without an authorized, version-matched layout.
