# Owned Demo Project

This project targets only the demo symbols compiled in `DemoTarget/`. It demonstrates explicit generated registration, a precompiled C++ callback that selects a fixed config string at runtime, a demo Objective-C method replacement, and `AppFunction<std::string()>` symbol calls.

```sh
cargo run --manifest-path ../../patcher/Cargo.toml -- validate hooks.yaml
cargo run --manifest-path ../../patcher/Cargo.toml -- generate hooks.yaml
make
```

The Objective-C demo hook expects an owned host app class named `IAOTDemoGreeter`. A missing class returns a safe installation failure. The patch entry is disabled so the checked-in manifest is safe by default; enable it only against a matching owned demo executable and provide its exact four expected bytes.

The Theos output still must be embedded into an authorized package and the complete package signed/re-signed before jailed-device use.

For an owned IPA, enable a matching patch and run:

```sh
iaot pipeline --manifest hooks.yaml --in OwnedDemo.ipa --out PatchedOwnedDemo-unsigned.ipa --dylib path/to/OwnedDemoHooks.dylib --reports build/reports
```

`expect: auto` resolves and prints the displaced bytes from that IPA. Pin those bytes later when the manifest should target one exact build.
