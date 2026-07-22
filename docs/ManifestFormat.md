# Centralized Manifest

`hooks.yaml` contains `project`, `paths`, `build`, `analysis`, `runtime`, `app`, `config`, `functions`, `generated_hooks`, and `patches`.

- `paths`: optional input/output, build, generated-report, and temp locations. CLI paths override them.
- `build`: development/release mode, log level, menu/persistence, cleanup policy, and external signing declaration.
- `analysis`: auto/override classification, renderer, IL2CPP module and method map, and static/runtime IL2CPP enablement.
- `app`: optional executable/target override, Unity preference, and required authorization assertion.
- `config`: indexed flags (0-255), strings (0-31), and callbacks (0-63). Auto callbacks need a symbol and exact signature.
- `functions`: typed calls by runtime address, VM address, RVA, or symbol; Objective-C hook entries name a class/selector, class/instance kind, replacement symbol, exact ABI, and original-preservation policy.
- `patches`: module, exact signature, optional original slot (0-63), failure behavior, architecture/version constraints, locator, action, and expected bytes.

Run `iaot validate hooks.yaml`. Unknown identifiers, duplicate indices/names, invalid segments/paths, unsupported architecture, bad signatures, missing flag/callback references, and invalid expected bytes are rejected before mutation. The current example is [the template manifest](../templates/theos-dylib/hooks.yaml).

Application-type overrides use the analyzer's reported names (`unity_il2cpp`, `unity_mono`, `unity_unknown_backend`, `native_objective_c`, `native_objective_cpp`, `native_cpp`, `swift`, `mixed_native`, or `unknown_or_native_c`). Scripting overrides are `il2cpp`, `mono`, `unknown`, or `native`. `external_analyzer` is `auto`, `method_manifest`, or `none`; IAOT never executes an unpinned third-party analyzer from YAML.
