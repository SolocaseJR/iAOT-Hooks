# Troubleshooting

## IPA and analysis

- **No primary app / multiple primary apps:** the ZIP must contain exactly one top-level `Payload/*.app/Info.plist`. App extensions under PlugIns are not selected.
- **Missing executable:** `CFBundleExecutable` must name an existing file in the primary app.
- **Encrypted code:** obtain a legitimately decrypted IPA. IAOT has no decryptor.
- **Unsupported architecture:** use a thin plain ARM64 target. arm64e and fat mutation are intentionally unavailable.
- **Unity backend uncertain:** read the evidence and set an explicit analysis override only when you know the correct backend/module.

## Resolution and bridges

- **Zero/multiple pattern matches:** select the correct module/section and strengthen the pattern. Never add a blind index across versions.
- **Expected bytes mismatch:** the app build changed or the locator is wrong. Re-analyze; do not update bytes until the new target is audited.
- **Relocation unsupported:** move to a safe entry instruction or add/test a relocation implementation. SIMD/FP literals and PRFM currently fail closed.
- **Branch out of range:** the appended cave is outside the ARM64 immediate range. This layout needs a reviewed near-island strategy not currently implemented.
- **Not enough load-command padding:** IAOT will not shift all existing sections. Use a compatible build/layout or extend the writer with full offset rewriting and tests.
- **Managed name is ambiguous:** the error lists the overloads or the
  assemblies. Add the parameter types to the `method:` string, or add
  `assembly:`. See [Managed-name locator](ManagedLocator.md).
- **Managed name has no native body:** the method is abstract, extern, or was
  stripped, or the generic instantiation you asked for was never compiled (the
  error lists the ones that were). Hook a concrete implementation, or fall back
  to signatures.
- **Unsupported metadata version:** only `global-metadata.dat` v39 is read, and
  encrypted metadata cannot be read at all. Use signatures for that build.
- **Managed name and signatures disagree:** the patch stops rather than guess.
  Confirm which address is right, then either refresh the stale pattern or set
  `on_conflict: prefer_managed`.
- **Hook disabled safely:** inspect `hook-preflight.yaml` or `final-validation.yaml`; `failure_behavior: disable_hook` preserves the rest of the build.

## WSL and Theos

- **No distribution:** `wsl --install -d Ubuntu`, reboot, and initialize the user.
- **WSL will not start / resource error:** run `wsl --shutdown`, check RAM/disk and `.wslconfig`, then retry.
- **Missing SDK/compiler:** inspect `$THEOS/sdks` and the Theos toolchain; follow the official installation guide.
- **Path with spaces:** use the PowerShell/batch entry points; they pass paths as separate quoted arguments and map them with `wslpath`.
- **Sync problems:** the WSL helper uses a fresh `mktemp` stage and rsync. It never deletes the source tree. Confirm `rsync` is installed.
- **Missing Python module:** the core needs no Python modules. Install requirements documented by the optional external analyzer you chose.
- **Missing ImGui:** run `scripts/fetch-imgui.ps1` or `.sh` before building the default menu template.

## Packaging and launch

- **Dylib injection failure:** the artifact must be a plain ARM64 `MH_DYLIB`, and the main executable needs load-command padding.
- **Invalid load commands:** keep the failed report/work directory and run `iaot inspect-macho`/`verify`; do not sign the IPA.
- **App crashes before logs:** common causes are invalid signing, wrong callback ABI, arm64e/PAC assumptions, or an incorrect target. Start with all patches disabled.
- **Original call returns a default:** the original slot is zero/unavailable. Confirm `original_vmaddr` in the report and that the callback signature/name matches generated code.
- **IL2CPP unavailable:** the runtime resolver needs the game's `il2cpp_*` exports. If the status stays `runtime_functions_unavailable`, the exports are stripped; static hooks still work, and managed lookups return null.
- **A managed member returns null after an update:** the class, method or field was renamed or its overload changed. The log names ambiguous overloads; pass parameter types to `FindMethod`.
- **Install rejected:** sign the main executable and every nested framework/dylib with compatible entitlements/provisioning. IAOT does not perform this step.
