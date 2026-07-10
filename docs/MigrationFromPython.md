# Migration from Python

Generic concepts map as follows:

| Earlier script concept | IAOT equivalent |
|---|---|
| `CFG_FIELDS` | generated flag/string/callback enums |
| `PATCHES` | manifest `patches` |
| `build_config_bytes` | stable `RuntimeConfig` initializer/layout |
| signature parsing/search | Rust `pattern` module |
| `build_cave` | audited Rust cave builder architecture |
| IPA executable replacement script | `iaot repack-ipa` with explicit `--member` |
| one-off Mach-O patch script | `iaot patch-macho` architecture |

The migration removes hardcoded bundle/member paths and app-specific constants. The IPA operation and conservative thin-ARM64 writer are implemented generically. Inputs that need load-command expansion by shifting existing sections remain unsupported and are refused explicitly.
