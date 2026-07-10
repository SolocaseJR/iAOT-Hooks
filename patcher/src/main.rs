mod arm64;
mod caves;
mod errors;
mod generate;
mod ipa;
mod macho;
mod manifest;
mod pattern;
mod verify;
use clap::{Parser, Subcommand};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
#[derive(Parser)]
#[command(
    name = "iaot",
    version,
    about = "Authorized jailed-iOS ahead-of-time instrumentation tooling"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}
#[derive(Subcommand)]
enum Cmd {
    New {
        name: String,
    },
    Validate {
        manifest: PathBuf,
        #[arg(long)]
        force_authorized: bool,
    },
    Generate {
        manifest: PathBuf,
        #[arg(long)]
        force_authorized: bool,
    },
    Build,
    PatchMacho {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long = "in")]
        input: PathBuf,
        #[arg(long = "out")]
        output: PathBuf,
        #[arg(long)]
        force_authorized: bool,
    },
    RepackIpa {
        #[arg(long = "in")]
        input: PathBuf,
        #[arg(long = "out")]
        output: PathBuf,
        #[arg(long)]
        member: String,
        #[arg(long)]
        patched: PathBuf,
    },
    InspectMacho {
        binary: PathBuf,
    },
    Verify {
        binary: PathBuf,
    },
}
fn validated(path: &Path, force: bool) -> anyhow::Result<manifest::Manifest> {
    let m = manifest::load(path)?;
    manifest::validate(&m, force)?;
    if force && !m.app.authorized_only {
        eprintln!("WARNING: authorization metadata was overridden; only proceed with an app you are authorized to test.");
    }
    Ok(m)
}
fn new_project(name: &str) -> anyhow::Result<()> {
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        anyhow::bail!("project name must use letters, numbers, '-' or '_'")
    }
    let d = Path::new(name);
    if d.exists() {
        anyhow::bail!("destination already exists")
    }
    fs::create_dir_all(d.join("hooks"))?;
    let manifest_path = d.join("hooks.yaml");
    fs::write(&manifest_path,format!("project:\n  name: {name}\n  bundle_id: com.example.owned.demo\n  platform: ios\n  arch: arm64\n  min_ios: \"14.0\"\nruntime:\n  config_segment: \"__IAOTCFG\"\n  cave_segment: \"__IAOTCAVE\"\n  page_size: 16384\n  dylib_name: \"{name}.dylib\"\napp:\n  executable_member: \"Payload/OwnedDemo.app/OwnedDemo\"\n  authorized_only: true\nconfig: {{ flags: [], strings: [], callbacks: [] }}\nfunctions: []\ngenerated_hooks:\n  - {{ name: UserHooks, symbol: IAOT_InstallUserHooks }}\npatches: []\n"))?;
    fs::write(
        d.join("README.md"),
        "Authorized IAOT project. Compile all native behavior before signing.\n",
    )?;
    fs::write(d.join("hooks/UserHooks.mm"),"#include <IAOT/IAOT.hpp>\n// Put authorized app-specific hook installs here.\nextern \"C\" bool IAOT_InstallUserHooks(const iaot::HookContext& ctx){IAOT_UNUSED(ctx);return true;}\n")?;
    fs::write(d.join("Makefile"),format!("TARGET := iphone:clang:latest:14.0\nARCHS := arm64\nIAOT_ROOT ?= ..\ninclude $(THEOS)/makefiles/common.mk\nLIBRARY_NAME := {name}\n{name}_FILES += $(wildcard $(IAOT_ROOT)/runtime/src/*.cpp) $(wildcard $(IAOT_ROOT)/runtime/src/*.mm) generated/IAOTGeneratedHooks.mm hooks/UserHooks.mm\n{name}_CCFLAGS += -std=c++17 -I$(IAOT_ROOT)/runtime/include -Igenerated\n{name}_OBJCCFLAGS += -std=c++17 -fobjc-arc -I$(IAOT_ROOT)/runtime/include -Igenerated\n{name}_LDFLAGS += -ObjC\ninclude $(THEOS_MAKE_PATH)/library.mk\n"))?;
    let m = manifest::load(&manifest_path)?;
    manifest::validate(&m, false)?;
    generate::generate(&m, &manifest_path)?;
    println!("created {}", d.display());
    Ok(())
}
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::New { name } => new_project(&name)?,
        Cmd::Validate {
            manifest,
            force_authorized,
        } => {
            validated(&manifest, force_authorized)?;
            println!("manifest is valid for authorized iOS instrumentation")
        }
        Cmd::Generate {
            manifest,
            force_authorized,
        } => {
            let m = validated(&manifest, force_authorized)?;
            generate::generate(&m, &manifest)?;
            println!("generated files next to {}", manifest.display())
        }
        Cmd::Build => {
            let status = Command::new("make").status()?;
            if !status.success() {
                anyhow::bail!("Theos build failed")
            }
        }
        Cmd::PatchMacho {
            manifest,
            input,
            output,
            force_authorized,
        } => {
            let m = validated(&manifest, force_authorized)?;
            let report = macho::patch(&m, &input, &output)?;
            println!("patched {} authorized redirect(s); config=0x{:x}, caves=0x{:x}, removed {} stale code signature command(s). The output must be signed/re-signed.", report.patches, report.config_vmaddr, report.cave_vmaddr, report.removed_code_signatures);
        }
        Cmd::RepackIpa {
            input,
            output,
            member,
            patched,
        } => {
            ipa::repack(&input, &output, &member, &patched)?;
            println!("repacked one member; the IPA is not signed and must be signed/re-signed before jailed-iOS use")
        }
        Cmd::InspectMacho { binary } => print!("{}", macho::inspect(&binary)?),
        Cmd::Verify { binary } => {
            verify::verify(&binary)?;
            println!("ARM64 Mach-O structural verification passed")
        }
    }
    Ok(())
}
