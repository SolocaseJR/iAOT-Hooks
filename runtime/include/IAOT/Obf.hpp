// IAOT/Obf.hpp -- the framework's single entry point for string/constant
// obfuscation. Backs the OBF()/OBFNS() convention already threaded through
// every runtime and user hook file with ac3ss0r/obfusheader.h (vendored at
// IAOT/vendor/obfusheader.h; same header the Aurora-iOS port uses).
//
// When enabled, string/int literals wrapped in OBF() are XOR-encrypted at
// compile time and only reconstructed in memory at first use, so `strings`
// or a Mach-O dump of the shipped dylib doesn't reveal log text, IL2CPP
// class/method/field names used for lookups, hook status strings, etc.
//
// IAOT_ENABLE_OBF is normally set by the Makefile from IAOT_BUILD_MODE
// (release => 1, development => 0; see templates/theos-dylib/Makefile) so
// development builds stay readable and release builds ship obfuscated.
// It can still be overridden with -DIAOT_ENABLE_OBF=0/1 directly.
#pragma once

#ifndef IAOT_ENABLE_OBF
#define IAOT_ENABLE_OBF 0
#endif

// clangd / editor indexers parse this tree without the real build's defines
// and don't benefit from (and are only slowed by) the template machinery.
// Fall back to identity macros there; the real Theos build path is unaffected.
#if defined(IAOT_CLANGD) || defined(__clangd__)
#undef IAOT_ENABLE_OBF
#define IAOT_ENABLE_OBF 0
#endif

#if IAOT_ENABLE_OBF

// --- clang/arm64-safe configuration, set BEFORE including obfusheader.h ---
// CFLOW_BRANCHING is the important one: upstream leaves it ON, which
// #defines the `if`/`for`/`while`/`switch`/`return`/`else` keywords as
// macros - catastrophic once this header reaches ObjC++ / STL / ImGui code.
// Keep it OFF. INDIRECT_BRANCHING is x86-only inline asm (a no-op on arm64)
// and FAKE_SIGNATURES is Windows/PE-only; disable both to avoid dead
// machinery. CONST_ENCRYPTION stays ON - that is the actual compile-time
// encryption: the obfuscator ctor XORs each byte at compile time, so only
// ciphertext lands in __cstring. CFLOW_CONST_DECRYPTION is OFF: it only
// control-flow-obfuscates the decrypt routine itself, which is always_inline
// and would otherwise bloat every OBF() call site for no additional secrecy.
#define CFLOW_CONST_DECRYPTION 0
#define CFLOW_BRANCHING 0
#define INDIRECT_BRANCHING 0
#define FAKE_SIGNATURES 0
// (OBF_ENCRYPT_BOOLS intentionally left undefined - never shadow true/false.)

#if defined(__clang__)
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wunused-function"
#pragma clang diagnostic ignored "-Wunused-variable"
#pragma clang diagnostic ignored "-Wunused-const-variable"
#pragma clang diagnostic ignored "-Wkeyword-macro"
#pragma clang diagnostic ignored "-Wgnu-zero-variadic-macro-arguments"
#endif
#include "IAOT/vendor/obfusheader.h"
#if defined(__clang__)
#pragma clang diagnostic pop
#endif

// obfusheader.h defines OBF(x) for strings and scalars alike.

#else // !IAOT_ENABLE_OBF - identity fallback (dev builds + editor)

#ifndef OBF
#define OBF(x) (x)
#endif

#endif // IAOT_ENABLE_OBF

// Objective-C convenience: obfuscate an @"..."-style literal into an
// autoreleased NSString, so the plaintext never lands in __cstring.
// Usage: OBFNS("com.example.endpoint").
#if defined(__OBJC__)
#define OBFNS(cstr) ([NSString stringWithUTF8String:(OBF(cstr))])
#endif
