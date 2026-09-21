# Importing Existing Authorized Work

Existing C++ logic and Dear ImGui drawing code can usually be reused. Jailbreak-specific loading and runtime patching cannot.

## Keep

- Plain C++ algorithms and state.
- Dear ImGui widget/drawing functions.
- Exact function typedefs for an owned app.
- Objective-C replacement methods that use the public Objective-C runtime.
- Configuration names and UI layout.

## Replace

- Substrate/Substitute/ElleKit hooks with generated IAOT registration, Objective-C method replacement, or offline manifest caves.
- Runtime memory writes to executable pages with `hooks.yaml` offline redirects.
- Static initializer-only registration with `RegisterGeneratedHooks` output.
- Jailbreak preference paths with IAOT’s fixed-layout runtime config.
- Arbitrary process attachment with embedding into the authorized IPA before signing.

## Suggested migration order

1. Copy UI drawing into a function with signature `void()` and register it through `SetOverlayDrawCallback`.
   For a callback that creates its own main window, pop-out consoles, or modal
   windows, set `OverlayOptions::clientDrawsWindows = true`. IAOT then derives
   touch regions from every visible top-level ImGui window while leaving gaps
   touch-through. Use `SetOverlayMinimized(true)` to return to the framework's
   floating launcher and `IsOverlayMinimized()` to query that state.
2. Move each hook body into a normal `.cpp` or `.mm` precompiled function.
3. Describe callback slots, flags, strings, and locators in `hooks.yaml`.
4. Remove all jailbreak loader and executable-page mutation code.
5. Generate, build, patch the owned IPA, sign, and test one hook at a time.

Wrong ABIs remain dangerous. Start with logging-only callbacks and do not dereference unknown objects until their owned source layout is verified.
