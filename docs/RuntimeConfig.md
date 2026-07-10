# Runtime Config

`RuntimeConfigHeader` stores `IAOTCFG1`, version, total size, and offset/count pairs for flags, callbacks, and strings, plus reserved fields. Static assertions lock the 64-bit header, array offsets, and total size.

Flags are 32-bit cells. `I32`, `U32`, `Bool`, and `F32` use one cell; `U64` uses two adjacent cells. Strings are 32 fixed 256-byte buffers and are always NUL-terminated. Callbacks are 64 pointer-sized slots and must point only to compatible precompiled signed functions.

Raw Mach-O storage cannot contain `std::string`, `std::vector`, or `std::map`: those contain process-specific pointers/allocator state and are not a stable binary layout. Accessors validate the header and bounds, return false/zero/empty string for unavailable data, and ignore invalid writes.
