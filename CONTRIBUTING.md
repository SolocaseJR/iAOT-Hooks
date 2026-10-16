# Contributing to IAOT

Thanks for your interest in improving IAOT. Contributions of all sizes are
welcome — bug reports, documentation fixes, new tests, and features.

## Ground rules

IAOT exists for **authorized** instrumentation (see
[Responsible Use](RESPONSIBLE_USE.md)). Please keep the project that way:

- **Never commit real application data.** No decrypted binaries, no proprietary
  offsets or addresses, no `global-metadata.dat` from a shipping app, no
  certificates, provisioning profiles, or entitlements. Examples and tests must
  use **synthetic** symbols and impossible addresses, as the existing ones do.
- **Do not add decryption, DRM bypass, runtime injection, process attachment, or
  jailbreak-loader functionality.** These are out of scope by design, and PRs
  adding them will be declined.
- Prefer features that **fail closed**. IAOT refuses ambiguous or unsupported
  input rather than guessing; new resolvers, relocations, and patch actions
  should do the same, with a clear error that says what to change.

## Development setup

You need Rust/Cargo and a C++17 toolchain with CMake. Building the *device*
runtime additionally needs WSL2 + Theos + an iPhoneOS SDK; see
[docs/Setup.md](docs/Setup.md). The portable C++ tests build and run without any
Apple SDK.

## Before you open a pull request

Run the same checks the project is developed against:

```bash
cargo fmt --manifest-path patcher/Cargo.toml -- --check
cargo clippy --manifest-path patcher/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path patcher/Cargo.toml --locked

cmake -S . -B build-portable
cmake --build build-portable
ctest --test-dir build-portable -C Debug --output-on-failure
```

- Keep formatting clean (`cargo fmt`) and clippy warning-free.
- Add or update tests for behavior you change. The Rust suite covers manifest
  parsing, ARM64 encoding/relocation, code generation, and the synthetic
  IPA-to-IPA pipeline; the C++ suite covers the runtime config layout, hook
  registry, overlay, persistence, and IL2CPP degradation.
- Update the relevant `docs/` guide when you change user-facing behavior. The
  docs are treated as part of the feature.

## Reporting issues

When filing a bug, please include the `iaot` version, your host OS, the command
you ran, and the relevant portion of the generated reports
(`hook-preflight.yaml`, `final-validation.yaml`, or `ipa-analysis.yaml`) — with
any application-identifying details removed.

## License

By contributing, you agree that your contributions are licensed under the
[MIT License](LICENSE) that covers this project.
