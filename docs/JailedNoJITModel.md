# Jailed / No-JIT Execution Model

All callback and bridge behavior exists in signed executable pages before launch. Offline patching is allowed to create executable bytes; runtime code never maps writable+executable memory, compiles code, writes host executable pages, attaches to another process, or relies on a jailbreak loader.

Runtime flags choose among already compiled paths. The optional IL2CPP resolver performs data/API lookup and managed invocation only. Runtime hook installation is intentionally absent.
