# End-to-End Workflow

`iaot pipeline` performs the supported workflow in one invocation:

1. Validate authorization metadata and centralized configuration.
2. Open the IPA and parse the primary app's `Info.plist`.
3. Select `CFBundleExecutable` and inventory embedded Mach-O modules.
4. Detect architectures, encryption state, application type, Unity backend/version/metadata, IL2CPP module, and rendering evidence.
5. Generate target-aware project/callback/original headers and Theos settings.
6. Securely extract every IPA member to an isolated workspace.
7. Resolve each enabled patch, generate original trampolines/static bridges, and verify load-command space.
8. Build the runtime with Theos when `--build-runtime` is used, or validate the supplied `--dylib`.
9. Patch every affected module and add the runtime load command to the main executable.
10. Repackage all replacements and the runtime dylib into a staged IPA.
11. Reopen the staged IPA, re-analyze it, verify every patched Mach-O, runtime member, and load command.
12. Atomically publish the requested output only after validation and write the final reports.

The input and output paths must differ. Temporary extraction is under a system temp directory or `paths.temp_dir`. `--work-dir` must be empty and is retained for manual inspection.

Configuration values in `paths` can replace `--in`/`--out` for the pipeline. CLI values take precedence.

## Module selection

- `module: main` selects `CFBundleExecutable`.
- `module: il2cpp` or `unity` selects the detected IL2CPP image.
- A full `Payload/...` member is exact.
- A unique embedded binary basename is accepted.
- An omitted module uses `app.target_module`, then `analysis.il2cpp_module`, then detected IL2CPP when `prefer_unity_framework` is true, otherwise main.

Ambiguity or a missing override is an error; IAOT never silently patches the first similar framework.

## Development and release

`iaot prepare` performs the same resolution/bridge/load-command preflight and writes `hook-preflight.yaml` before Windows invokes Theos. Development permits `expect: auto` and verbose reporting. For release, pin the four displaced bytes, set `build.mode: release`, reduce `build.log_level`, and keep the matching analysis/validation reports with the build. IAOT never overwrites the source IPA.
