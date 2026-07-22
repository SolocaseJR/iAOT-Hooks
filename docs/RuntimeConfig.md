# Runtime Config ABI

Version 2 uses magic `IAOTCFG1`, a 72-byte header, 256 32-bit flag cells, 64 callback pointers, 32 fixed 256-byte strings, and 64 original-trampoline pointers. The 64-bit total is 10,312 bytes; static assertions and Rust verification lock every offset.

Scalar flag/callback access uses atomic compiler builtins. Compound 64-bit/string operations use a small runtime lock; string reads return a thread-local copy. When multiple modules are patched, setters mirror data to every valid config. Original lookup searches configs for the unique nonzero slot.

Only fixed-layout values belong in Mach-O data. `std::string`, containers, and allocator-owned objects are prohibited.
