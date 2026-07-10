# Packaging and Signing

Jailed iOS executes code that is part of the signed app package. A Theos-built dylib is only an artifact: an authorized workflow must embed it, arrange for the owned app to load it, and sign/re-sign the complete package and nested code.

`iaot repack-ipa` only replaces one explicit ZIP member while preserving compression, timestamp, and Unix mode where the ZIP exposes them. It does not inject unrelated files, modify entitlements, or sign. The resulting IPA must be signed/re-signed externally before installation.

IAOT cannot attach to arbitrary already-installed apps and does not use jailbreak loaders, platform entitlements, or `task_for_pid`.
