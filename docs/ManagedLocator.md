# Managed-name locator

A hook can name its target the way the game's own source does - class and
method - instead of (or alongside) byte signatures. IAOT turns that name into a
native address **at build time**, reading the decrypted IPA it is already
patching. Static hooks must be written into the binary before signing, so the
resolution has to happen offline; nothing about this locator runs on device.

Method names survive app updates. Byte signatures do not: an IL2CPP toolchain
change can re-codegen a function and invalidate its pattern while the managed
method is untouched. A named hook keeps working across such an update; a named
hook that *also* carries signatures reports which signatures went stale.

## Inputs

Only the decrypted IPA from `paths.input_ipa`. The build reads
`global-metadata.dat` and the IL2CPP module from inside it, so there is no
dump, no Cpp2IL output, and nothing to regenerate per version.

`iaot patch-macho` works on a loose binary, so there it takes `--metadata
<global-metadata.dat>`. `patch-ipa` and the full build pipeline find the
metadata member themselves.

## Configuration

Name only:

```yaml
    locator:
      type: managed
      method: "Example.Game.Player::ConsumePower(System.String, System.Int32)"
```

Name plus signatures, with the toggle:

```yaml
    locator:
      type: signatures
      managed: "Example.Game.Player::ConsumePower(System.String, System.Int32)"
      use_managed: true
      signatures:
        - { kind: xref, pattern: "? ? ? 94 ? ? ? 11 ? ? ? B9 ? ? ? 17" }
        - { kind: direct, pattern: "? ? ? 71 ? ? ? 52 ? ? ? 52 28 01 88 9A" }
```

Both forms accept:

| key | meaning |
| --- | --- |
| `assembly` | which assembly the class lives in (`Assembly-CSharp.dll`), when the class name is ambiguous |
| `offset` | hook N bytes into the function; a multiple of 4 |
| `instantiation` | which native copy of a generic method to take (see below) |
| `follow_entry_stub` | follow a single jump from the entry to the real body; `true` by default |
| `on_conflict` | `fail` (default) or `prefer_managed`, for `signatures` + `use_managed` |

## Method string grammar

```
[Namespace.]Class[/Nested][`Arity]::MethodName[(ParamType, ...)]
```

- The class is the full managed class name. Nested types use `Outer/Inner` or
  `Outer+Inner`; both are accepted.
- A generic class keeps its backtick arity, as the metadata spells it:
  ``System.Collections.Generic.List`1``.
- The method name follows `::`. Constructors are `.ctor` (static ones
  `.cctor`); managed accessors keep their generated names, `get_IsFlagged`.
- Parameters are optional. Omitted, the method name must be unique in that
  class, and an ambiguous name is an error that lists the overloads. Given,
  they can be full names (`System.Int32`), simple names (`Int32`), or `*` as a
  per-position wildcard. `T[]`, `T*`, and `T&` are written as in C#.

```yaml
      method: "Ns.Outer/Inner::.ctor()"
      method: "System.Collections.Generic.Dictionary`2::TryGetValue(*, *)"
      method: "Ns.Thing::Write(System.Byte[], System.Int32&)"
```

## Generic methods

A generic method has no single native body. All reference-type arguments share
one copy compiled against `System.Object`; each value-type instantiation gets
its own. `instantiation` picks which copy the hook lands on:

- `auto` (the default): the copy that reference-type arguments run, falling
  back to the fully-shared copy.
- `object` / `fully_shared`: the shared copy explicitly.
- an explicit list of type arguments, for one value-type copy:

```yaml
    locator:
      type: managed
      method: "System.Collections.Generic.Dictionary`2::TryGetValue"
      instantiation: [System.Byte, System.Int32]
```

The report records which instantiation was taken. If the requested one has no
body, the error lists the instantiations that do.

## Toggle behavior

`use_managed: true` makes the name authoritative:

- The build uses the named address.
- Signatures become a cross-check. A signature that lands somewhere else is
  reported as stale and the build continues at the named address.
- If the name does not resolve, the build falls back to the signatures and
  notes that it did.
- With no signatures, a failed lookup fails the patch by name, per
  `failure_behavior`.
- When two or more signatures agree with *each other* against the name, the
  default `on_conflict: fail` stops the build with both addresses rather than
  guessing which side is right. `on_conflict: prefer_managed` takes the name.

`use_managed: false` behaves exactly like today's signatures locator. The name
stays in the manifest as documentation and is noted as recorded but ignored.

## Entry stubs

When the named entry is a single unconditional jump to the real body, IAOT
follows it and notes the hop. It refuses to follow a branch whose target is
another managed method's own entry (a tail call) or one that leaves the
section, so a shared thunk never silently redirects a hook.

## Errors

Every failure says what to change:

- class not found - with the near-miss names in that namespace;
- class in several assemblies - lists them and asks for `assembly:`;
- several overloads match - lists every overload with its parameter types and
  address;
- method has no native body - abstract, extern, or stripped;
- generic with no usable instantiation - lists the instantiations that exist;
- unsupported or encrypted metadata - points back to signatures.

## Reporting

`generated/reports/hook-preflight.yaml` records, per hook, how it was found
(`managed name`, `signature N of M`, `managed name (signature 2 is stale)`),
the instantiation taken, the stub hop if any, and the cross-check notes. The
build log prints the same notes.

## Update workflow

Change `paths.input_ipa` and build. Named hooks resolve against the new
binary's metadata. Signature-only hooks behave as before. Named hooks that also
carry signatures tell you which signatures went stale, so the patterns can be
refreshed deliberately instead of being discovered broken later.

## Supported metadata

`global-metadata.dat` version 39 (Unity 2022/2023-era IL2CPP). The reader
validates the layout it derives - the method table must partition exactly by
assembly, declaring-type back-pointers must agree, and the module's method
count must match the metadata's - and refuses to resolve anything if a check
fails. Other versions, and encrypted metadata, fail with a message pointing to
signatures rather than resolving to a wrong address.
