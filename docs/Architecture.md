# Architecture

IAOT has three layers that deliberately do not share responsibilities.

1. The Rust offline tool reads the IPA, classifies it, resolves targets, emits fixed ARM64 bridge code, injects a dylib load command, repackages, and validates.
2. The precompiled runtime dylib populates callback slots, exposes toggles and diagnostics, initializes the optional UI and IL2CPP API resolver, and contains every user callback.
3. User code describes exact ABIs and behavior. Generated code connects manifest names/indices to typed C++ accessors.

## Static bridge data flow

```text
original entry VM address
  -> 4-byte B instruction written offline
  -> __IAOTCAVE bridge
       callback slot null: relocate displaced instruction -> entry+4
       callback slot set:  tail-branch to compiled user callback
                              -> optional generated Original_Name().Call(...)
                              -> separate original trampoline
                                   relocated displaced instruction -> entry+4
```

`__IAOTCFG` is read/write and contains fixed-layout flags, callback pointers, strings, and original-trampoline addresses. `__IAOTCAVE` is read/execute and contains only audited generated instructions. No page is writable and executable.

The offline writer stores original trampoline VM addresses. At startup and on later dyld image notifications, the runtime finds IAOT config segments, applies each image's slide once, and registers them. A late config inherits the active flags, strings, and callback pointers; subsequent writes are mirrored to every config. This lets a main executable load command and a UnityFramework or dynamically loaded framework hook coexist. Original lookup returns the nonzero trampoline for its globally unique slot.

## ABI scope

`branch_to_callback` is a tail branch. It leaves x0-x7 and q0-q7 unchanged and therefore supports ordinary scalar, pointer, floating/vector, and ABI-defined structure parameters/returns when the callback is compiled with the exact same non-variadic AAPCS64 signature. IAOT does not infer or prove that signature.

Observational `callback_if_flag` templates save caller-saved x0-x17, LR, q0-q7, and NZCV around a `void` callback. They are not replacement hooks and do not expose a callable original.

Variadic hooks, arm64e authenticated callback pointers, and unsupported relocation instructions are rejected or documented as unavailable.

## Initialization and failure isolation

Runtime initialization uses `std::call_once`. Generated callback registration runs before user registration. Each hook has a diagnostic state. A generated callback slot remains zero until initialization; a static bridge encountering zero safely replays the original path.

IL2CPP initialization uses dyld image callbacks plus a bounded background wait. It never blocks the main thread and never gates native hooks, Objective-C replacement, config, persistence, or rendering.

## Packaging

The high-level pipeline always places the runtime at `Payload/<App>.app/Frameworks/<name>.dylib` and adds the load command to the main executable. Static patches may target the main image or other analyzed modules. Repacking uses a same-directory temporary ZIP and replaces the requested output only after the archive is complete. The input IPA is never overwritten.
