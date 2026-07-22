# Theos Runtime Build

The template compiles all `runtime/src/*.cpp`/`.mm`, generated registration, and `user/*.mm` into one ARM64 dylib. Objective-C++ uses ARC; C++ uses C++17. `runtime/overlay/IAOTOverlay.mk` conditionally adds Dear ImGui and Metal frameworks.

Run `iaot prepare` before Make so detected IL2CPP/module/renderer data reaches generated code. On Linux/WSL, `iaot pipeline --build-runtime` runs Make. On Windows, use `scripts/build-iaot.ps1`; it stages source into WSL, supplies the correct `IAOT_ROOT`, and copies the dylib back.

Theos only compiles. IPA injection, validation, and signing are separate pipeline/external stages.
