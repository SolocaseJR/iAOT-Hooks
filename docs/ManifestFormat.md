# Manifest Format

`hooks.yaml` is authorization-bearing build input.

- `project`: `name`, reverse-DNS `bundle_id`, `platform: ios`, `arch: arm64`, and quoted `min_ios`.
- `runtime`: distinct ASCII Mach-O `config_segment`/`cave_segment` names (maximum 16 bytes), power-of-two `page_size`, and `dylib_name`.
- `app`: `authorized_only: true` and optional explicit `executable_member`, such as `Payload/OwnedDemo.app/OwnedDemo`.
- `config.flags`: unique `name`, `index` (0–255), and Boolean `default`.
- `config.strings`: unique `name`, `index` (0–31), and fixed-string `default`.
- `config.callbacks`: unique `name`, `index` (0–63), and precompiled C `symbol`.
- `generated_hooks`: display `name` and valid C `symbol` with signature `bool(const HookContext&)`.
- `functions`: C++ `name`, exact `signature`, and address `{type, value|symbol}`. Types are `runtime`, `vmaddr`, `rva`, and `symbol`. Objective-C entries instead use `kind: objc`, `class`, and `selector`.
- `patches`: named, optionally enabled offline operations with `description`, `locator`, `patch`, and `expect`.

Locator examples:

```yaml
locator: { type: pattern, pattern: "AA BB ?? CC || 11 22 33 44", match_index: 0 }
locator:
  type: xref
  pattern: "?? ?? ?? 94"
  xref_pattern: "AA BB ?? CC"
  match_index: 0
  xref_match_index: 0
  xref_bl_index: 0
locator: { type: symbol, symbol: _OwnedDemoFunction }
expect: "FD 7B BF A9"
```

Ambiguous pattern/xref matches require the relevant index. With both patterns present, `pattern` selects the target instruction and `xref_pattern` must contain the indexed BL resolving to that target. Without `xref_pattern`, the indexed BL inside `pattern` resolves the patch target. Enabled patches require four concrete expected bytes unless `--force-authorized` is explicitly supplied. `executable_member` is never inferred from a private/hardcoded app. The force flag does not create authorization.
