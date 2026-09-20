mod analyzer;
mod arm64;
mod branding;
mod caves;
mod errors;
mod generate;
mod il2cpp_metadata;
mod ipa;
mod macho;
mod manifest;
mod pattern;
mod pipeline;
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
    },
    Generate {
        manifest: PathBuf,
    },
    ResolveConfig {
        #[arg(long)]
        manifest: PathBuf,
    },
    AnalyzeIpa {
        #[arg(long = "in")]
        input: PathBuf,
        #[arg(long)]
        reports: Option<PathBuf>,
    },
    Prepare {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long = "in")]
        input: PathBuf,
        #[arg(long)]
        reports: PathBuf,
    },
    Pipeline {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long = "in")]
        input: Option<PathBuf>,
        #[arg(long = "out")]
        output: Option<PathBuf>,
        #[arg(long)]
        dylib: Option<PathBuf>,
        #[arg(long)]
        build_runtime: bool,
        #[arg(long)]
        reports: Option<PathBuf>,
        #[arg(long)]
        work_dir: Option<PathBuf>,
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
        load_dylib: bool,
    },
    PatchIpa {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long = "in")]
        input: PathBuf,
        #[arg(long = "out")]
        output: PathBuf,
        #[arg(long)]
        member: Option<String>,
        #[arg(long)]
        dylib: Option<PathBuf>,
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
fn validated(path: &Path) -> anyhow::Result<manifest::Manifest> {
    let m = manifest::load(path)?;
    manifest::validate(&m)?;
    Ok(m)
}
fn print_patch_report(report: &macho::PatchReport) {
    for resolution in &report.resolutions {
        println!(
            "{}: vmaddr=0x{:x}, displaced={:02X} {:02X} {:02X} {:02X}{}",
            resolution.name,
            resolution.vmaddr,
            resolution.bytes[0],
            resolution.bytes[1],
            resolution.bytes[2],
            resolution.bytes[3],
            if resolution.automatic {
                " (resolved automatically)"
            } else {
                ""
            }
        );
        if let Some(original) = resolution.original_vmaddr {
            println!("  callable original trampoline: 0x{original:x}");
        }
    }
    for failure in &report.failures {
        println!("{}: disabled safely: {}", failure.name, failure.reason);
    }
    let near = report
        .near_cave_vmaddr
        .map(|address| format!(", near-cave=0x{address:x}"))
        .unwrap_or_default();
    println!(
        "patched {} redirect(s); config=0x{:x}, cave=0x{:x}{}, removed {} stale code signature command(s){}",
        report.patches,
        report.config_vmaddr,
        report.cave_vmaddr,
        near,
        report.removed_code_signatures,
        if report.added_dylib_load_command {
            "; added embedded-dylib load command"
        } else {
            ""
        }
    );
}

#[derive(serde::Serialize)]
struct ResolvedProjectConfig {
    project_name: String,
    source_dir: PathBuf,
    input_ipa: Option<PathBuf>,
    output_ipa: Option<PathBuf>,
    build_dir: Option<PathBuf>,
    generated_dir: Option<PathBuf>,
    temp_dir: Option<PathBuf>,
    build_mode: String,
    dylib_name: String,
    il2cpp_enabled: bool,
}

fn resolve_project_path(source_dir: &Path, value: Option<PathBuf>) -> Option<PathBuf> {
    value.map(|path| {
        if path.is_absolute() {
            path
        } else {
            source_dir.join(path)
        }
    })
}

fn resolved_project_config(path: &Path) -> anyhow::Result<ResolvedProjectConfig> {
    let configured = validated(path)?;
    let source_dir = if configured.source_dir.is_absolute() {
        configured.source_dir.clone()
    } else {
        std::env::current_dir()?.join(&configured.source_dir)
    };
    Ok(ResolvedProjectConfig {
        project_name: configured.project.name,
        input_ipa: resolve_project_path(&source_dir, configured.paths.input_ipa),
        output_ipa: resolve_project_path(&source_dir, configured.paths.output_ipa),
        build_dir: resolve_project_path(&source_dir, configured.paths.build_dir),
        generated_dir: resolve_project_path(&source_dir, configured.paths.generated_dir),
        temp_dir: resolve_project_path(&source_dir, configured.paths.temp_dir),
        build_mode: configured.build.mode,
        dylib_name: configured.runtime.dylib_name,
        il2cpp_enabled: configured.analysis.runtime_il2cpp
            || configured.analysis.static_il2cpp
            || configured.analysis.scripting_backend == "il2cpp",
        source_dir,
    })
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
    fs::create_dir_all(d.join("user"))?;
    let manifest_path = d.join("hooks.yaml");
    let template_manifest = include_str!("../../templates/theos-dylib/hooks.yaml")
        .replace("ExampleHooks", name)
        .replace(
            "com.example.authorized.target",
            &format!("com.example.{}.target", name.to_ascii_lowercase()),
        );
    fs::write(&manifest_path, template_manifest)?;
    let template_config = include_str!("../../templates/theos-dylib/configs.yaml")
        .replace("ExampleHooks", name)
        .replace(
            "com.example.authorized.target",
            &format!("com.example.{}.target", name.to_ascii_lowercase()),
        );
    fs::write(d.join("configs.yaml"), template_config)?;
    fs::write(
        d.join("il2cpp_config.yaml"),
        include_str!("../../templates/theos-dylib/il2cpp_config.yaml"),
    )?;
    fs::write(
        d.join("README.md"),
        "Authorized IAOT project. Compile all native behavior before signing.\n",
    )?;
    fs::write(
        d.join("user/UserHooks.mm"),
        include_str!("../../templates/theos-dylib/user/UserHooks.mm"),
    )?;
    fs::write(
        d.join("user/UserMenu.mm"),
        include_str!("../../templates/theos-dylib/user/UserMenu.mm"),
    )?;
    fs::write(
        d.join("user/UserFeatures.hpp"),
        include_str!("../../templates/theos-dylib/user/UserFeatures.hpp"),
    )?;
    let makefile = include_str!("../../templates/theos-dylib/Makefile")
        .replace("IAOT_ROOT := ../..", "IAOT_ROOT ?= ..")
        .replace("ExampleHooks", name);
    fs::write(d.join("Makefile"), makefile)?;
    let m = manifest::load(&manifest_path)?;
    manifest::validate(&m)?;
    generate::generate(&m, &manifest_path)?;
    println!("created {}", d.display());
    Ok(())
}
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::New { name } => new_project(&name)?,
        Cmd::Validate { manifest } => {
            validated(&manifest)?;
            println!("manifest is valid for authorized iOS instrumentation")
        }
        Cmd::Generate { manifest } => {
            let m = validated(&manifest)?;
            generate::generate(&m, &manifest)?;
            println!("generated files next to {}", manifest.display())
        }
        Cmd::ResolveConfig { manifest } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&resolved_project_config(&manifest)?)?
            );
        }
        Cmd::AnalyzeIpa { input, reports } => {
            let report = analyzer::analyze_ipa(&input)?;
            print!("{}", analyzer::human_report(&report));
            if let Some(directory) = reports {
                analyzer::write_reports(&report, &directory)?;
                println!("reports written to {}", directory.display());
            }
        }
        Cmd::Prepare {
            manifest,
            input,
            reports,
        } => pipeline::prepare(&manifest, &input, &reports)?,
        Cmd::Pipeline {
            manifest,
            input,
            output,
            dylib,
            build_runtime,
            reports,
            work_dir,
        } => {
            let configured = manifest::load(&manifest)?;
            let resolve = |path: PathBuf| {
                if path.is_absolute() {
                    path
                } else {
                    configured.source_dir.join(path)
                }
            };
            let input = input
                .or_else(|| configured.paths.input_ipa.clone())
                .map(resolve)
                .ok_or_else(|| anyhow::anyhow!("pipeline needs --in or paths.input_ipa"))?;
            let output = output
                .or_else(|| configured.paths.output_ipa.clone())
                .map(resolve)
                .ok_or_else(|| anyhow::anyhow!("pipeline needs --out or paths.output_ipa"))?;
            pipeline::run(pipeline::PipelineOptions {
                manifest_path: &manifest,
                input_ipa: &input,
                output_ipa: &output,
                dylib: dylib.as_deref(),
                build_runtime,
                reports_dir: reports.as_deref(),
                work_dir: work_dir.as_deref(),
            })?;
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
            load_dylib,
        } => {
            let m = validated(&manifest)?;
            let report = if load_dylib {
                macho::patch_with_options(
                    &m,
                    &input,
                    &output,
                    macho::PatchOptions { load_dylib: true },
                )?
            } else {
                macho::patch(&m, &input, &output)?
            };
            print_patch_report(&report);
            println!("The output must be signed/re-signed.");
        }
        Cmd::PatchIpa {
            manifest,
            input,
            output,
            member,
            dylib,
        } => {
            if input == output {
                anyhow::bail!("input and output IPA paths must differ")
            }
            let m = validated(&manifest)?;
            let unity_member = if member.is_none() && m.app.prefer_unity_framework {
                ipa::find_unity_framework_member(&input)?
            } else {
                None
            };
            let executable_member = member
                .or(unity_member)
                .or_else(|| m.app.executable_member.clone())
                .ok_or_else(|| anyhow::anyhow!("supply --member or app.executable_member"))?;
            let temp = tempfile::tempdir()?;
            let original = temp.path().join("original-macho");
            let patched = temp.path().join("patched-macho");
            ipa::extract_member(&input, &executable_member, &original)?;
            println!("target member: {executable_member}");
            let report = macho::patch_with_options(
                &m,
                &original,
                &patched,
                macho::PatchOptions {
                    load_dylib: dylib.is_some(),
                },
            )?;
            let addition = if let Some(path) = dylib.as_deref() {
                if !path.is_file() {
                    anyhow::bail!("dylib file does not exist: {}", path.display())
                }
                let app_dir = ipa::app_bundle_root(&executable_member)?;
                Some((
                    format!("{app_dir}/Frameworks/{}", m.runtime.dylib_name),
                    path,
                ))
            } else {
                None
            };
            ipa::repack_with_addition(
                &input,
                &output,
                &executable_member,
                &patched,
                addition.as_ref().map(|x| (x.0.as_str(), x.1)),
            )?;
            print_patch_report(&report);
            println!("repacked patched executable{}; the IPA is not signed and must be signed/re-signed before jailed-iOS use",if addition.is_some(){" and staged the compiled dylib"}else{""});
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
