# Mach-O Patcher

The writer accepts thin plain ARM64 `MH_EXECUTE` and `MH_DYLIB` files. It parses load commands/sections with bounds checks, scans executable ranges, validates aligned file-backed targets, removes stale `LC_CODE_SIGNATURE`, appends read/write `__IAOTCFG` and read/execute `__IAOTCAVE`, writes fixed audited bridges, reopens the result, and verifies segment ranges and redirects.

Supported one-instruction relocation families are B/BL, B.cond, CBZ/CBNZ, TBZ/TBNZ, ADR, ADRP, integer LDR literal, and LDRSW literal. Ordinary non-PC-relative instructions are copied. SIMD/FP literal and PRFM relocation fail closed. Direct/relocated branch-range failures are reported.

The writer does not accept arbitrary instruction bytes, shift all sections for header growth, mutate fat binaries, or claim arm64e authenticated function-pointer support. See [Architecture](Architecture.md) for the bridge and [Hooks](Hooks.md) for locators/actions.
