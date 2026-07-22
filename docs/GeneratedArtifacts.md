# Generated Artifacts

`generated/` is machine-owned. Every generated C++ file begins with a do-not-edit comment.

- `IAOTGeneratedConfig.hpp`: flag/string/callback enums and defaults.
- `IAOTGeneratedFunctions.hpp`: typed calls to existing application code.
- `IAOTGeneratedOriginals.hpp`: typed callable-original accessors.
- `IAOTGeneratedProject.hpp`: menu, persistence, renderer, and IL2CPP target settings.
- `IAOTGeneratedProject.mk`: authoritative build mode plus Theos overlay enablement/backend derived from centralized configuration.
- `IAOTGeneratedHooks.mm`: explicit callback/resolver/user registration.
- `hook-manifest.yaml`: machine-readable configured hook summary.

Report directories contain analysis and validation artifacts. `hook-preflight.yaml` proves that target resolution, bridge/trampoline construction, and load-command space checks passed before the runtime build. `il2cpp-methods.yaml` is created as an empty audited adapter skeleton and is not overwritten when it already exists.

Generated address/trampoline data in `final-validation.yaml` records the target module, VM address, displaced bytes, optional original trampoline, disabled hook reasons, runtime member, and load-command result. These reports are suitable build artifacts; they are not application source.
