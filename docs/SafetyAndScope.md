# Safety and Scope

Use IAOT only with software you own or are explicitly authorized to test. It expects decrypted input, contains no decryptor, and grants no rights over third-party software. See [Responsible Use](../RESPONSIBLE_USE.md).

The framework fails closed on ambiguous targets, unsupported architecture/relocation, invalid addresses, branch range, insufficient patch space, malformed ZIP/plist/Mach-O data, invalid runtime dylibs, missing load commands, and structural validation errors. Per-hook disable mode is explicit; it never converts an unsafe patch into a guess.
