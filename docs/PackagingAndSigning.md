# Packaging and Signing

The high-level pipeline embeds the compiled runtime under the primary app's `Frameworks` directory, adds `@executable_path/Frameworks/<dylib>` to the main executable, replaces every patched module, and writes a new IPA through a temporary archive. It never overwrites the input.

Patched code signatures are stale and removed where IAOT mutates a Mach-O. The output is explicitly unsigned. Sign the main executable, embedded frameworks, and injected dylib with a compatible provisioning/entitlement setup before installation. IAOT does not manage certificates, entitlements, device registration, or installation.
