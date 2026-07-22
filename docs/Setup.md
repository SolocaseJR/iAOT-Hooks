# Windows, WSL, Theos, and Dependency Setup

## Windows host

1. Enable virtualization in firmware if required.
2. Open an administrator PowerShell and run `wsl --install -d Ubuntu`.
3. Restart, launch Ubuntu, and create its Linux user.
4. Confirm `wsl --list --verbose` shows Ubuntu at version 2.
5. Install Git and Rust from their official installers. Confirm `git --version` and `cargo --version` in a new PowerShell.
6. Clone IAOT and run `cargo install --path patcher`.

If WSL reports insufficient resources, close memory-heavy applications, run `wsl --shutdown`, check free disk/RAM and `%UserProfile%\.wslconfig`, then retry. If no distribution is listed, run `wsl --list --online` and install Ubuntu explicitly.

## Ubuntu packages

```bash
sudo apt update
sudo apt install -y bash curl git build-essential clang make python3 rsync zip unzip
```

IAOT's core CLI has no Python package dependency; Python is checked because optional analyzer adapters commonly use it.

## Theos and SDK

Install Theos as the normal Linux user following the current official guide:

```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/theos/theos/master/bin/install-theos)"
```

Set `THEOS` in `~/.profile` if the installer did not do so:

```bash
export THEOS="$HOME/theos"
```

Install an iPhoneOS SDK supported by Theos under `$THEOS/sdks`. Verify:

```bash
test -d "$THEOS"
find "$THEOS/sdks" -maxdepth 1 -type d -name 'iPhoneOS*.sdk'
test -x "$THEOS/toolchain/linux/iphone/bin/clang" || command -v clang
```

IAOT does not distribute Apple SDKs. Obtain and use SDKs under their applicable license.

## Dear ImGui

The default template includes a menu, so fetch the pinned upstream source once:

```powershell
.\scripts\fetch-imgui.ps1
```

or:

```bash
bash scripts/fetch-imgui.sh
```

## First test

```powershell
cargo test --manifest-path patcher/Cargo.toml --locked
iaot new FirstHooks
iaot validate .\FirstHooks\hooks.yaml
```

Analyze an authorized decrypted IPA before enabling any hook. Then use `build_iaot_ipa.bat` or `scripts/build-iaot.ps1`. The script copies source into a fresh `mktemp` WSL staging directory, builds, copies only the dylib back, and removes only that staging directory.

## Verifying artifacts

`iaot pipeline` rejects a runtime artifact that is not a plain ARM64 `MH_DYLIB`. It reopens the output IPA, checks that the runtime member exists, checks the main executable load command, verifies IAOT segments and patched module structure, and writes `final-validation.yaml`. Signing validation is external.
