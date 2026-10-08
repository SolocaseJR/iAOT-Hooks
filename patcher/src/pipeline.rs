use crate::{
    analyzer, branding, generate, ipa, macho,
    manifest::{self, Manifest},
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug)]
pub struct PipelineOptions<'a> {
    pub manifest_path: &'a Path,
    pub input_ipa: &'a Path,
    pub output_ipa: &'a Path,
    pub dylib: Option<&'a Path>,
    pub build_runtime: bool,
    pub reports_dir: Option<&'a Path>,
    pub work_dir: Option<&'a Path>,
}

#[derive(Debug, Serialize)]
struct HookPatchRecord {
    name: String,
    module: String,
    vmaddr: String,
    displaced_bytes: String,
    original_vmaddr: Option<String>,
    located_by: String,
    /// The generic instantiation that was hooked, when the target is generic.
    #[serde(skip_serializing_if = "Option::is_none")]
    instantiation: Option<String>,
    /// The entry address before an entry stub was followed.
    #[serde(skip_serializing_if = "Option::is_none")]
    stub_from: Option<String>,
    /// Cross-check results and anything that degraded, in resolution order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct HookFailureRecord {
    name: String,
    module: String,
    reason: String,
}

#[derive(Debug, Serialize)]
struct ValidationReport {
    schema_version: u32,
    input_ipa: String,
    output_ipa: String,
    output_exists: bool,
    output_size: u64,
    executable_member: String,
    runtime_dylib_member: String,
    display_name: Option<String>,
    custom_icon_members: Vec<String>,
    runtime_dylib_present: bool,
    load_command_present: bool,
    patched_modules: Vec<String>,
    applied_hooks: Vec<HookPatchRecord>,
    disabled_hooks: Vec<HookFailureRecord>,
    structural_validation: String,
    signing_required: bool,
}

#[derive(Debug, Serialize)]
struct PreflightReport {
    schema_version: u32,
    status: &'static str,
    applied_hooks: Vec<HookPatchRecord>,
    disabled_hooks: Vec<HookFailureRecord>,
}

struct PatchSelection {
    groups: BTreeMap<String, Vec<crate::manifest::Patch>>,
    disabled_hooks: Vec<HookFailureRecord>,
}

struct PatchedModules {
    replacement_files: Vec<(String, PathBuf)>,
    applied_hooks: Vec<HookPatchRecord>,
    disabled_hooks: Vec<HookFailureRecord>,
}

fn resolve_module(report: &analyzer::IpaAnalysis, value: &str) -> anyhow::Result<String> {
    if value == "main" {
        return Ok(report.executable_member.clone());
    }
    if matches!(value, "unity" | "il2cpp") {
        return report.unity.il2cpp_module.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "module alias '{value}' was requested but no IL2CPP module was detected"
            )
        });
    }
    if value.starts_with("Payload/") {
        if report.modules.iter().any(|module| module.member == value) {
            return Ok(value.into());
        }
        anyhow::bail!("configured module member does not exist: {value}")
    }
    let matches = report
        .modules
        .iter()
        .filter(|module| module.name == value || module.member.ends_with(&format!("/{value}")))
        .map(|module| module.member.clone())
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [member] => Ok(member.clone()),
        [] => anyhow::bail!("configured module '{value}' was not found"),
        _ => anyhow::bail!("configured module '{value}' is ambiguous"),
    }
}

/// Resolve a configured IL2CPP module (e.g. `main`) to its runtime image name -
/// the basename of the resolved member path - so the generated config carries
/// the actual executable image name rather than a manifest alias.
fn runtime_module_name(report: &analyzer::IpaAnalysis, value: &str) -> anyhow::Result<String> {
    let member = resolve_module(report, value)?;
    member
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            anyhow::anyhow!("resolved IL2CPP module has no runtime image name: {member}")
        })
}

fn default_patch_module(
    manifest: &Manifest,
    report: &analyzer::IpaAnalysis,
) -> anyhow::Result<String> {
    if let Some(value) = manifest.app.target_module.as_deref() {
        return resolve_module(report, value);
    }
    if let Some(value) = manifest.analysis.il2cpp_module.as_deref() {
        return resolve_module(report, value);
    }
    if manifest.app.prefer_unity_framework && report.unity.scripting_backend == "il2cpp" {
        if let Some(member) = &report.unity.il2cpp_module {
            return Ok(member.clone());
        }
    }
    Ok(report.executable_member.clone())
}

fn apply_analysis_overrides(
    report: &mut analyzer::IpaAnalysis,
    manifest: &Manifest,
) -> anyhow::Result<()> {
    if let Some(configured) = manifest.app.executable_member.as_deref() {
        let module = report
            .modules
            .iter()
            .find(|module| module.member == configured)
            .ok_or_else(|| {
                anyhow::anyhow!("app.executable_member override was not found: {configured}")
            })?;
        if module.role != "main_executable" {
            anyhow::bail!("app.executable_member must identify the primary MH_EXECUTE member; use app.target_module for a framework")
        }
        report.executable_member = configured.to_owned();
        report.executable_name = configured
            .rsplit('/')
            .next()
            .unwrap_or(configured)
            .to_owned();
    }
    if manifest.analysis.application_type != "auto" {
        report.application_type = manifest.analysis.application_type.clone();
        report.classification_confidence = "configured_override".into();
        report.classification_evidence.push(format!(
            "analysis.application_type override selected {}",
            manifest.analysis.application_type
        ));
    }
    if manifest.analysis.scripting_backend != "auto" {
        report.unity.scripting_backend = manifest.analysis.scripting_backend.clone();
        if matches!(
            manifest.analysis.scripting_backend.as_str(),
            "il2cpp" | "mono"
        ) {
            report.unity.detected = true;
        }
        report.unity.evidence.push(format!(
            "analysis.scripting_backend override selected {}",
            manifest.analysis.scripting_backend
        ));
    }
    report.unity.static_analyzer = if report.unity.scripting_backend != "il2cpp" {
        "not_applicable".into()
    } else if !manifest.analysis.static_il2cpp || manifest.analysis.external_analyzer == "none" {
        "disabled".into()
    } else {
        "iaot_method_manifest_adapter".into()
    };
    report.unity.runtime_resolver = if report.unity.scripting_backend != "il2cpp" {
        "not_applicable".into()
    } else if manifest.analysis.runtime_il2cpp {
        "iaot_exported_api_resolver".into()
    } else {
        "disabled".into()
    };
    if let Some(configured) = manifest.analysis.il2cpp_module.as_deref() {
        report.unity.il2cpp_module = Some(resolve_module(report, configured)?);
        report.unity.evidence.push(format!(
            "analysis.il2cpp_module override selected {}",
            report.unity.il2cpp_module.as_deref().unwrap_or(configured)
        ));
    }
    if manifest.analysis.rendering_backend != "auto" {
        report.rendering.selected = manifest.analysis.rendering_backend.clone();
        report.rendering.evidence.push(format!(
            "analysis.rendering_backend override selected {}",
            manifest.analysis.rendering_backend
        ));
    }
    Ok(())
}

fn detected_versions(report: &analyzer::IpaAnalysis) -> Vec<&str> {
    [
        report.unity.version.as_deref(),
        report.short_version.as_deref(),
        report.bundle_version.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn selected_patch_groups(
    manifest: &Manifest,
    analysis: &analyzer::IpaAnalysis,
    include_main: bool,
) -> anyhow::Result<PatchSelection> {
    let default_module = default_patch_module(manifest, analysis)?;
    let mut groups: BTreeMap<String, Vec<crate::manifest::Patch>> = BTreeMap::new();
    let mut disabled_hooks = Vec::new();
    for patch in manifest.patches.iter().filter(|patch| patch.enabled) {
        let member = match patch.module.as_deref() {
            Some(value) => resolve_module(analysis, value)?,
            None => default_module.clone(),
        };
        let versions = detected_versions(analysis);
        if !patch.versions.is_empty()
            && !versions
                .iter()
                .any(|version| patch.versions.iter().any(|allowed| allowed == version))
        {
            let detected = if versions.is_empty() {
                "unknown".into()
            } else {
                versions.join(", ")
            };
            if patch.failure_behavior == crate::manifest::FailureBehavior::DisableHook {
                disabled_hooks.push(HookFailureRecord {
                    name: patch.name.clone(),
                    module: member,
                    reason: format!(
                        "detected Unity/app version {detected} is not in the hook allow-list"
                    ),
                });
                continue;
            }
            anyhow::bail!(
                "patch {} does not allow detected Unity/app version {}",
                patch.name,
                detected
            );
        }
        groups.entry(member).or_default().push(patch.clone());
    }
    if include_main {
        groups
            .entry(analysis.executable_member.clone())
            .or_default();
    }
    Ok(PatchSelection {
        groups,
        disabled_hooks,
    })
}

fn patch_extracted_modules(
    manifest: &Manifest,
    analysis: &analyzer::IpaAnalysis,
    extracted: &Path,
    patched_dir: &Path,
    load_runtime: bool,
) -> anyhow::Result<PatchedModules> {
    let selection = selected_patch_groups(manifest, analysis, load_runtime)?;
    let mut disabled_hooks = selection.disabled_hooks;
    std::fs::create_dir_all(patched_dir)?;
    let mut replacement_files = Vec::new();
    let mut applied_hooks = Vec::new();
    for (index, (member, patches)) in selection.groups.into_iter().enumerate() {
        let source = extracted.join(Path::new(&member));
        if !source.is_file() {
            anyhow::bail!("selected Mach-O is missing from workspace: {member}")
        }
        let metadata = analysis
            .unity
            .metadata_member
            .as_ref()
            .map(|member| extracted.join(Path::new(member)))
            .filter(|path| path.is_file());
        let mut module_manifest = manifest.clone();
        module_manifest.patches = patches;
        let output = patched_dir.join(format!("module-{index}.macho"));
        let patch_report = macho::patch_with_options(
            &module_manifest,
            &source,
            &output,
            macho::PatchOptions {
                load_dylib: load_runtime && member == analysis.executable_member,
                metadata,
            },
        )
        .map_err(|error| error.context(format!("failed to patch {member}")))?;
        for resolution in patch_report.resolutions {
            println!(
                "  hook {} -> {} @ 0x{:x}; original={}; located by {}",
                resolution.name,
                member,
                resolution.vmaddr,
                resolution
                    .original_vmaddr
                    .map(|address| format!("0x{address:x}"))
                    .unwrap_or_else(|| "not requested".into()),
                resolution.located_by
            );
            applied_hooks.push(HookPatchRecord {
                name: resolution.name,
                module: member.clone(),
                vmaddr: format!("0x{:x}", resolution.vmaddr),
                displaced_bytes: resolution
                    .bytes
                    .iter()
                    .map(|byte| format!("{byte:02X}"))
                    .collect::<Vec<_>>()
                    .join(" "),
                original_vmaddr: resolution
                    .original_vmaddr
                    .map(|address| format!("0x{address:x}")),
                located_by: resolution.located_by,
                instantiation: resolution.instantiation,
                stub_from: resolution.stub_from.map(|address| format!("0x{address:x}")),
                notes: resolution.notes,
            });
        }
        for record in &applied_hooks {
            for note in &record.notes {
                println!("    note ({}): {note}", record.name);
            }
        }
        for failure in patch_report.failures {
            println!(
                "  hook {} disabled safely: {}",
                failure.name, failure.reason
            );
            disabled_hooks.push(HookFailureRecord {
                name: failure.name,
                module: member.clone(),
                reason: failure.reason,
            });
        }
        replacement_files.push((member, output));
    }
    Ok(PatchedModules {
        replacement_files,
        applied_hooks,
        disabled_hooks,
    })
}

fn find_files_with_extension(
    directory: &Path,
    extension: &str,
    output: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            find_files_with_extension(&path, extension, output)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            output.push(path);
        }
    }
    Ok(())
}

fn build_runtime(manifest: &Manifest) -> anyhow::Result<PathBuf> {
    if cfg!(windows) {
        anyhow::bail!(
            "runtime build requested on Windows; use build_iaot_ipa.bat so Theos runs inside WSL"
        )
    }
    let makefile = manifest.source_dir.join("Makefile");
    if !makefile.is_file() {
        anyhow::bail!("runtime Makefile is missing: {}", makefile.display())
    }
    let status = Command::new("make")
        .arg("clean")
        .current_dir(&manifest.source_dir)
        .status()?;
    if !status.success() {
        anyhow::bail!("Theos clean stage failed")
    }
    let status = Command::new("make")
        .arg(if manifest.build.mode == "release" {
            "DEBUG=0"
        } else {
            "DEBUG=1"
        })
        .current_dir(&manifest.source_dir)
        .status()?;
    if !status.success() {
        anyhow::bail!("Theos runtime build failed")
    }
    let mut dylibs = Vec::new();
    find_files_with_extension(&manifest.source_dir.join(".theos"), "dylib", &mut dylibs)?;
    dylibs.sort_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    dylibs
        .pop()
        .ok_or_else(|| anyhow::anyhow!("Theos reported success but no .dylib was produced"))
}

fn report_directory(options: &PipelineOptions<'_>, manifest: &Manifest) -> PathBuf {
    options
        .reports_dir
        .map(Path::to_path_buf)
        .or_else(|| {
            manifest.paths.generated_dir.as_ref().map(|path| {
                if path.is_absolute() {
                    path.clone()
                } else {
                    manifest.source_dir.join(path)
                }
            })
        })
        .or_else(|| {
            manifest.paths.build_dir.as_ref().map(|path| {
                if path.is_absolute() {
                    path.join("reports")
                } else {
                    manifest.source_dir.join(path).join("reports")
                }
            })
        })
        .unwrap_or_else(|| {
            options
                .output_ipa
                .parent()
                .unwrap_or(Path::new("."))
                .join("iaot-reports")
        })
}

pub fn prepare(manifest_path: &Path, input_ipa: &Path, reports: &Path) -> anyhow::Result<()> {
    let manifest = manifest::load(manifest_path)?;
    manifest::report_warnings(&manifest);
    manifest::validate(&manifest)?;
    let mut analysis = analyzer::analyze_ipa(input_ipa)?;
    apply_analysis_overrides(&mut analysis, &manifest)?;
    print!("{}", analyzer::human_report(&analysis));
    analyzer::write_reports(&analysis, reports)?;
    println!("Preflighting hook resolution and static bridge generation");
    let preflight_workspace = tempfile::Builder::new()
        .prefix("iaot-preflight-")
        .tempdir()?;
    let extracted = preflight_workspace.path().join("extracted");
    ipa::extract_all(input_ipa, &extracted)?;
    let preflight = patch_extracted_modules(
        &manifest,
        &analysis,
        &extracted,
        &preflight_workspace.path().join("patched"),
        true,
    )?;
    std::fs::write(
        reports.join("hook-preflight.yaml"),
        serde_yaml::to_string(&PreflightReport {
            schema_version: 1,
            status: "passed",
            applied_hooks: preflight.applied_hooks,
            disabled_hooks: preflight.disabled_hooks,
        })?,
    )?;
    let mut effective = manifest.clone();
    effective.analysis.runtime_il2cpp &= analysis.unity.scripting_backend == "il2cpp";
    if let Some(configured) = effective.analysis.il2cpp_module.clone() {
        effective.analysis.il2cpp_module = Some(runtime_module_name(&analysis, &configured)?);
    } else {
        effective.analysis.il2cpp_module = analysis
            .unity
            .il2cpp_module
            .as_ref()
            .and_then(|member| member.rsplit('/').next())
            .map(str::to_owned);
    }
    if effective.analysis.rendering_backend == "auto" {
        effective.analysis.rendering_backend = if effective.build.menu {
            "metal"
        } else {
            "disabled"
        }
        .into();
    }
    generate::generate(&effective, manifest_path)?;
    println!(
        "prepared generated sources and analysis reports for {}",
        input_ipa.display()
    );
    Ok(())
}

pub fn run(options: PipelineOptions<'_>) -> anyhow::Result<()> {
    if options.input_ipa == options.output_ipa {
        anyhow::bail!("input and output IPA paths must differ")
    }
    let manifest = manifest::load(options.manifest_path)?;
    manifest::report_warnings(&manifest);
    manifest::validate(&manifest)?;
    println!(
        "[1/8] Analyzing decrypted IPA: {}",
        options.input_ipa.display()
    );
    let mut analysis = analyzer::analyze_ipa(options.input_ipa)?;
    apply_analysis_overrides(&mut analysis, &manifest)?;
    print!("{}", analyzer::human_report(&analysis));
    if analysis
        .modules
        .iter()
        .any(|module| module.encrypted == Some(true))
    {
        anyhow::bail!("the IPA contains an encrypted Mach-O; IAOT does not decrypt applications")
    }
    let reports = report_directory(&options, &manifest);
    analyzer::write_reports(&analysis, &reports)?;

    println!("[2/8] Validating configuration and generating C++ glue");
    let mut generated_manifest = manifest.clone();
    generated_manifest.analysis.runtime_il2cpp &= analysis.unity.scripting_backend == "il2cpp";
    if let Some(configured) = generated_manifest.analysis.il2cpp_module.clone() {
        generated_manifest.analysis.il2cpp_module =
            Some(runtime_module_name(&analysis, &configured)?);
    } else {
        generated_manifest.analysis.il2cpp_module = analysis
            .unity
            .il2cpp_module
            .as_ref()
            .and_then(|member| member.rsplit('/').next())
            .map(str::to_owned);
    }
    if generated_manifest.analysis.rendering_backend == "auto" {
        generated_manifest.analysis.rendering_backend = if generated_manifest.build.menu {
            "metal"
        } else {
            "disabled"
        }
        .into();
    }
    generate::generate(&generated_manifest, options.manifest_path)?;

    println!("[3/8] Extracting IPA into an isolated workspace");
    let mut temporary = None;
    let workspace = if let Some(path) = options.work_dir {
        if path.exists() && path.read_dir()?.next().is_some() {
            anyhow::bail!("--work-dir must be empty")
        }
        std::fs::create_dir_all(path)?;
        path.to_path_buf()
    } else if let Some(base) = manifest.paths.temp_dir.as_ref() {
        let base = if base.is_absolute() {
            base.clone()
        } else {
            manifest.source_dir.join(base)
        };
        std::fs::create_dir_all(&base)?;
        let directory = tempfile::Builder::new().prefix("iaot-").tempdir_in(base)?;
        let path = directory.path().to_path_buf();
        temporary = Some(directory);
        path
    } else {
        let directory = tempfile::Builder::new().prefix("iaot-").tempdir()?;
        let path = directory.path().to_path_buf();
        temporary = Some(directory);
        path
    };
    let extracted = workspace.join("extracted");
    ipa::extract_all(options.input_ipa, &extracted)?;
    let branding_artifacts = branding::prepare(
        &manifest,
        &analysis.app_bundle,
        &analysis.info_plist_member,
        &extracted,
        &workspace.join("branding"),
    )?;
    if branding_artifacts.display_name.is_some() || !branding_artifacts.icon_members.is_empty() {
        println!(
            "  branding: display_name={} custom_icons={}",
            branding_artifacts
                .display_name
                .as_deref()
                .unwrap_or("unchanged"),
            branding_artifacts.icon_members.len()
        );
    }

    println!("[4/8] Resolving hook targets and applying static ARM64 bridges");
    let patched_dir = workspace.join("patched");
    let patched = patch_extracted_modules(&manifest, &analysis, &extracted, &patched_dir, true)?;
    let patched_module_members = patched
        .replacement_files
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let mut replacement_files = patched.replacement_files;
    replacement_files.extend(branding_artifacts.replacements.iter().cloned());
    let applied_hooks = patched.applied_hooks;
    let disabled_hooks = patched.disabled_hooks;

    println!("[5/8] Building or validating the precompiled runtime dylib");
    let built_dylib;
    let dylib = if let Some(path) = options.dylib {
        path
    } else if options.build_runtime {
        built_dylib = build_runtime(&manifest)?;
        &built_dylib
    } else {
        anyhow::bail!("the complete pipeline needs --dylib or --build-runtime")
    };
    if !dylib.is_file() {
        anyhow::bail!("runtime dylib does not exist: {}", dylib.display())
    }
    let dylib_arches = analyzer::macho_architectures(&std::fs::read(dylib)?);
    if !dylib_arches.iter().any(|arch| arch == "arm64") {
        anyhow::bail!("runtime dylib does not contain a supported plain arm64 slice")
    }
    macho::verify_dylib(dylib)?;

    println!("[6/8] Injecting the precompiled runtime dylib and repackaging");
    let dylib_member = format!(
        "{}/Frameworks/{}",
        analysis.app_bundle, manifest.runtime.dylib_name
    );
    let replacement_refs = replacement_files
        .iter()
        .map(|(name, path)| (name.as_str(), path.as_path()))
        .collect::<Vec<_>>();
    let mut addition_refs = branding_artifacts
        .additions
        .iter()
        .map(|(name, path)| (name.as_str(), path.as_path()))
        .collect::<Vec<_>>();
    addition_refs.push((dylib_member.as_str(), dylib));
    let staged_ipa = workspace.join("validated-output.ipa");
    ipa::repack_many(
        options.input_ipa,
        &staged_ipa,
        &replacement_refs,
        &addition_refs,
    )?;

    println!("[7/8] Reopening and structurally validating the final IPA");
    let final_analysis = analyzer::analyze_ipa(&staged_ipa)?;
    let final_extract = workspace.join("final-validation");
    ipa::extract_all(&staged_ipa, &final_extract)?;
    for member in &patched_module_members {
        macho::verify(&final_extract.join(Path::new(member)))?;
    }
    let dylib_output = final_extract.join(Path::new(&dylib_member));
    let dylib_present = dylib_output.is_file();
    if !dylib_present {
        anyhow::bail!("final IPA is missing the injected runtime dylib")
    }
    macho::verify_dylib(&dylib_output)?;
    let executable = final_extract.join(Path::new(&analysis.executable_member));
    let inspect = macho::inspect(&executable)?;
    let load_command_present = final_analysis
        .modules
        .iter()
        .find(|module| module.member == analysis.executable_member)
        .is_some_and(|module| {
            module
                .imports
                .iter()
                .any(|name| name.ends_with(&format!("/{}", manifest.runtime.dylib_name)))
        });
    if !load_command_present {
        anyhow::bail!("final main executable does not load the injected runtime dylib")
    }
    if let Some(expected) = &branding_artifacts.display_name {
        if final_analysis.display_name.as_deref() != Some(expected.as_str()) {
            anyhow::bail!(
                "final IPA display name mismatch: expected {expected:?}, got {:?}",
                final_analysis.display_name
            )
        }
    }
    for member in &branding_artifacts.icon_members {
        if !final_extract.join(Path::new(member)).is_file() {
            anyhow::bail!("final IPA is missing generated app icon {member}")
        }
    }

    println!("[8/8] Writing patch and validation manifests");
    std::fs::create_dir_all(&reports)?;
    ipa::publish_file(&staged_ipa, options.output_ipa)?;
    let validation = ValidationReport {
        schema_version: 1,
        input_ipa: options.input_ipa.display().to_string(),
        output_ipa: options.output_ipa.display().to_string(),
        output_exists: options.output_ipa.is_file(),
        output_size: std::fs::metadata(options.output_ipa)?.len(),
        executable_member: analysis.executable_member.clone(),
        runtime_dylib_member: dylib_member,
        display_name: final_analysis.display_name.clone(),
        custom_icon_members: branding_artifacts.icon_members,
        runtime_dylib_present: dylib_present,
        load_command_present,
        patched_modules: patched_module_members,
        applied_hooks,
        disabled_hooks,
        structural_validation: "passed".into(),
        signing_required: true,
    };
    std::fs::write(
        reports.join("final-validation.yaml"),
        serde_yaml::to_string(&validation)?,
    )?;
    std::fs::write(reports.join("final-validation.txt"), format!(
        "IAOT final validation: PASSED\nOutput: {}\nExecutable: {}\nRuntime: {}\nLoad command: present\nSigning: required externally\n\n{}",
        options.output_ipa.display(), validation.executable_member, validation.runtime_dylib_member, inspect))?;
    if !manifest.build.clean_temporary {
        if let Some(directory) = temporary.take() {
            println!(
                "Retained temporary workspace: {}",
                directory.keep().display()
            );
        }
    }
    println!("Complete: {}", options.output_ipa.display());
    println!(
        "Validation report: {}",
        reports.join("final-validation.yaml").display()
    );
    println!("The output is structurally valid but unsigned; sign the app and every nested binary before installation.");
    Ok(())
}
