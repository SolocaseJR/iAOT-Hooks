# Split project configuration

New IAOT projects use three focused YAML files.

- `hooks.yaml` contains hook-local flags/strings, typed functions, generated
  installers, and single-declaration patches. Callback slots are normally
  generated from each patch's `replacement`.
- `configs.yaml` contains project identity, paths and final IPA name, build
  settings, general analysis settings, runtime container settings, and app
  targeting.
- `il2cpp_config.yaml` contains only IL2CPP analysis, metadata imports, string
  factory, and offline classes/methods/fields/internal calls.

`hooks.yaml` selects the project settings:

```yaml
schema_version: 2
project_config: configs.yaml
config: { flags: [], strings: [] }
functions: []
generated_hooks: []
patches: []
```

`configs.yaml` selects IL2CPP independently:

```yaml
project: { name: Example, bundle_id: com.example.app, platform: ios, arch: arm64, min_ios: "14.0" }
paths: { input_ipa: input.ipa, output_ipa: build/Example-unsigned.ipa, build_dir: build }
build: { mode: release, log_level: info, menu: true, persistence: true, signing: external }
analysis: { application_type: auto, rendering_backend: auto }
runtime: { config_segment: "__IAOTCFG", cave_segment: "__IAOTCAVE", page_size: 16384, dylib_name: Example.dylib }
app: { executable_member: null, prefer_unity_framework: true, target_module: null, display_name: null, icon_path: null }
il2cpp: { enabled: true, config_file: il2cpp_config.yaml }
```

When `il2cpp.enabled` is false, IAOT does not open `il2cpp_config.yaml` and
forces both static and runtime IL2CPP analysis off. This allows the same
framework layout to serve native, Mono, and IL2CPP projects without dummy
metadata files.

Paths in the effective configuration are relative to the directory containing
`hooks.yaml`. The IL2CPP config filename itself is relative to `configs.yaml`.
The merge order is project config, optional IL2CPP config, then hook manifest;
the final hook layer can intentionally override a shared project default.

Legacy monolithic `hooks.yaml` manifests remain supported.

## Sections

- `paths`: optional input/output IPA, build, report, and temporary locations.
  Explicit CLI arguments override these values.
- `build`: development/release mode, log level, menu/persistence, runtime
  startup, minimal-runtime diagnostics, cleanup, and signing declaration.
- `analysis`: application/renderer classification in `configs.yaml`; IL2CPP
  backend/module/analyzer selection in `il2cpp_config.yaml`.
- `runtime`: cave/config segment names, page size, and dylib name in
  `configs.yaml`; metadata and offline resolver bindings in
  `il2cpp_config.yaml`.
- `app`: executable/target override, Unity-framework preference, and optional
  `display_name`/`icon_path` SpringBoard branding. The icon path is relative to
  the hook project and must reference a square PNG of at least 180x180 pixels
  (1024x1024 recommended).
- `config`: indexed flags (0-255) and strings (0-31). Manually indexed
  callbacks (0-63) remain supported for advanced legacy manifests.
- `functions`: typed native calls and Objective-C replacement declarations.
- `generated_hooks`: user installer declarations (`name`, `symbol`, optional
  `enabled`, default `true`). Disabled installers are neither declared nor
  registered in generated runtime code. This does not disable independently
  registered callbacks, static patches, Objective-C hooks, or dylib constructors.
- `patches`: replacement symbol, ABI, automatic original preservation, module,
  failure behavior, constraints, locator, action, and expected bytes.

Run `iaot validate hooks.yaml`; referenced files are merged before validation.
Run `iaot resolve-config --manifest hooks.yaml` to print the fully resolved
project paths and build selection as JSON for scripts and CI.

See the [hook template](../templates/theos-dylib/hooks.yaml),
[project template](../templates/theos-dylib/configs.yaml), and
[IL2CPP template](../templates/theos-dylib/il2cpp_config.yaml).

Application-type overrides use `unity_il2cpp`, `unity_mono`,
`unity_unknown_backend`, `native_objective_c`, `native_objective_cpp`,
`native_cpp`, `swift`, `mixed_native`, or `unknown_or_native_c`. Scripting
overrides are `il2cpp`, `mono`, `unknown`, or `native`. `external_analyzer` is
`auto`, `method_manifest`, or `none`; IAOT never executes an unpinned
third-party analyzer from YAML.
