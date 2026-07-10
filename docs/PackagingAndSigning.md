# Packaging and Signing

Jailed iOS executes code that is part of the signed app package. A Theos-built dylib is only an artifact: an authorized workflow must embed it, arrange for the owned app to load it, and sign/re-sign the complete package and nested code.

`iaot patch-ipa` selects the executable from `--member` or `app.executable_member`, extracts and patches it, and repacks it while preserving compression, timestamps, and Unix modes where exposed. With `--dylib`, it stages the compiled runtime under the app's `Frameworks` directory and adds the matching Mach-O load command. `iaot repack-ipa` remains available for replacing one already-patched member.

Neither command modifies entitlements or signs. The resulting IPA and all nested code must be signed/re-signed externally before installation.

IAOT cannot attach to arbitrary already-installed apps and does not use jailbreak loaders, platform entitlements, or `task_for_pid`.
