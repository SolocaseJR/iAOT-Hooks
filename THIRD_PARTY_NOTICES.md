# Third-Party Notices

IAOT source is MIT licensed. Dependencies are not relicensed by this project.

- [Dear ImGui](https://github.com/ocornut/imgui), MIT. It is fetched at a pinned tag by `scripts/fetch-imgui.*`; its `LICENSE.txt` is retained in `vendor/imgui`.
- Rust crates are declared in `patcher/Cargo.toml`; Cargo preserves their package metadata and licenses. Notable direct dependencies are `anyhow`, `clap`, `object`, `serde`, `serde_yaml`, `thiserror`, `tempfile`, and `zip`.
- [Cpp2IL/LibCpp2IL](https://github.com/SamboyCoding/Cpp2IL), MIT, is an optional external static analyzer and is not vendored.
- [Il2CppDumper](https://github.com/Perfare/Il2CppDumper), MIT, is an optional compatibility adapter source and is not vendored.
- [IOS-Il2cppResolver](https://github.com/Batchhh/IOS-Il2cppResolver), MIT, was evaluated as a compatibility baseline but no source is copied into IAOT.
- [Il2CppInspector](https://github.com/djkaty/Il2CppInspector), AGPL-3.0, was evaluated but is not included or linked.

If another dependency is vendored, add its unmodified license text under `vendor/<name>/` and update this file.
