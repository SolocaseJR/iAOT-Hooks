# Testing and Verification

Rust unit/integration tests cover manifest constraints, wildcard/ambiguous scanning, BL xrefs, ARM64 encoding and relocation, config/cave generation, original trampolines, load-command padding, signature removal, IPA replacement/addition, executable and Unity/metadata detection, new-project generation, low-level IPA patching, and the complete synthetic IPA pipeline.

Portable C++ tests cover config layout/access, multi-module original lookup, typed application calls, hook status/ordering, generated APIs, overlay stubs, persistence, and IL2CPP graceful degradation.

Run the commands in the README. A release additionally needs manual verification with an authorized test app:

1. Keep the original IPA hash and verify it is unchanged.
2. Run analysis with all hooks disabled.
3. Build, inject, repackage, and inspect `final-validation.yaml`.
4. Sign every nested binary and launch on a test device.
5. Confirm runtime/config/menu diagnostics.
6. Enable one hook, verify pass-through, then verify enabled behavior and original calls.
7. For IL2CPP, verify resolver status and each used capability independently.
8. Preserve the exact IPA version, manifest, generated reports, dylib, and crash symbols.

No real third-party IPA, proprietary offset, certificate, or decrypted binary belongs in the repository.
