# Packaging and Signing

The high-level pipeline embeds the compiled runtime under the primary app's `Frameworks` directory, adds `@executable_path/Frameworks/<dylib>` to the main executable, replaces every patched module, and writes a new IPA through a temporary archive. It never overwrites the input.

When `app.display_name` or `app.icon_path` is configured, the same staged pass
rewrites the primary `Info.plist`, updates localized `InfoPlist.strings`, and
generates opaque iPhone/iPad PNG icon variants. This deliberately uses
standalone icon declarations so a compiled `Assets.car` cannot override the
configured icon. The final validation report records the resolved display name
and every generated icon member.

Patched code signatures are stale and removed where IAOT mutates a Mach-O. The output is explicitly unsigned. Sign the main executable, embedded frameworks, and injected dylib with a compatible provisioning/entitlement setup before installation. IAOT does not manage certificates, entitlements, device registration, or installation.
