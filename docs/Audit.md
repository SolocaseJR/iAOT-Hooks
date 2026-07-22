# Initial Architecture Audit

The pre-generalization project already had a useful conservative core: a Rust manifest parser, thin ARM64 Mach-O parsing, executable-region pattern scanning, fixed config/cave segments, several fixed cave actions, callback registration, typed address calls, Objective-C replacement, IPA member repacking, and a standalone Metal overlay. Its Rust tests passed.

Important gaps and risks found during the audit:

- IPA selection was a configured path plus a special UnityFramework shortcut; `Info.plist`, embedded modules, architectures, application type, scripting backend, and renderer were not analyzed.
- `patch-ipa` patched one module and injected the load command into that same module. It was not a multi-module pipeline and produced no final validation manifest.
- The displaced instruction was replayed only when it was not PC-relative. There was no user-facing callable-original slot.
- Callback slots were populated manually. A missing runtime constructor safely passed through, but the bridge relationship was not generated or documented.
- The config store used unsynchronized ordinary loads/stores. Multiple patched modules could contain configs, while the runtime selected only one.
- Hook registration reported only a combined Boolean, with no per-hook status.
- Runtime initialization used a plain Boolean and was not guarded by `call_once`.
- Objective-C helpers were useful but depended on the user supplying the exact ABI and did not generate manifest-driven replacements.
- The optional overlay was independent Metal, but the template default was empty and did not demonstrate a persisted hook toggle.
- There was no IL2CPP static adapter, runtime resolver, Unity compatibility report, Windows drag/drop workflow, dependency doctor, or complete setup/troubleshooting guide.
- The old documentation contained stale claims, encoding corruption, and a workflow that stopped before a validated final IPA.
- Fat Mach-O, arm64e pointer authentication, load-command expansion, and broad instruction relocation were unsupported but not organized into one capabilities report.

Aurora-iOS contributed useful workflow ideas: staged copies instead of input mutation, exact output paths, WSL source synchronization, preflight dependency checks, development/release modes, settings persistence, explicit stage logging, hash/structure verification, and single-entry Windows automation. Application-specific offsets, features, authentication, assets, names, protection systems, and menu styling were intentionally not ported.

The completed architecture keeps the conservative Rust writer and source-distributed runtime, adds target analysis and orchestration around them, generates callback/original glue, replicates runtime data across patched modules, and records unsupported cases rather than guessing.
