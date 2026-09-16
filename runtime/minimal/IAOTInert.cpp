// Deliberately minimal arm64 dylib used only to isolate loader/signing and
// target-integrity failures from the IAOT runtime itself.  No constructors,
// Objective-C runtime, C++ runtime, hooks, or framework dependencies.
extern "C" int iaot_inert_marker(void) { return 0; }
