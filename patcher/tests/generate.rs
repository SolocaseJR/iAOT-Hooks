use std::process::Command;
use tempfile::tempdir;

#[test]
fn disabled_objective_c_hooks_keep_original_accessors_but_are_not_installed() {
    let directory = tempdir().unwrap();
    let manifest = directory.path().join("hooks.yaml");
    std::fs::write(
        &manifest,
        "project: { name: Demo, bundle_id: com.example.demo, platform: ios, arch: arm64, min_ios: '14.0' }\n\
runtime: { config_segment: __IAOTCFG, cave_segment: __IAOTCAVE, page_size: 16384, dylib_name: Demo.dylib }\n\
app: {}\n\
analysis: { runtime_il2cpp: false }\n\
config: { flags: [], strings: [], callbacks: [] }\n\
functions:\n\
  - { name: DisabledTitle, enabled: false, kind: objc, class: DisabledController, selector: title, class_method: false, signature: 'id(id, SEL)', replacement: IAOT_DisabledTitle, preserve_original: true }\n\
  - { name: EnabledTitle, kind: objc, class: EnabledController, selector: title, class_method: false, signature: 'id(id, SEL)', replacement: IAOT_EnabledTitle, preserve_original: true }\n\
generated_hooks: []\n\
patches: []\n",
    )
    .unwrap();

    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_iaot"))
        .canonicalize()
        .unwrap();
    let result = Command::new(executable)
        .args(["generate", manifest.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );

    let functions = std::fs::read_to_string(
        directory
            .path()
            .join("generated/IAOTGeneratedFunctions.hpp"),
    )
    .unwrap();
    let hooks =
        std::fs::read_to_string(directory.path().join("generated/IAOTGeneratedHooks.mm")).unwrap();
    let generated_manifest =
        std::fs::read_to_string(directory.path().join("generated/hook-manifest.yaml")).unwrap();

    assert!(functions.contains("Original_DisabledTitle()"));
    assert!(functions.contains("OriginalIMP_DisabledTitle()"));
    assert!(!functions.contains("Install_DisabledTitle()"));
    assert!(!functions.contains("IAOT_DisabledTitle"));
    assert!(!hooks.contains("DisabledTitle"));
    assert!(!generated_manifest.contains("DisabledTitle"));
    assert!(functions.contains("EnabledTitle"));
    assert!(hooks.contains("EnabledTitle"));
    assert!(generated_manifest.contains("EnabledTitle"));
    let runtime =
        std::fs::read_to_string(directory.path().join("generated/IAOTGeneratedRuntime.hpp"))
            .unwrap();
    assert!(runtime.contains("#define IAOT_CONFIG_SEGMENT_NAME \"__IAOTCFG\""));
}

#[test]
fn generated_installers_honor_enabled_and_report_the_configured_state() {
    let directory = tempdir().unwrap();
    let manifest = directory.path().join("hooks.yaml");
    std::fs::write(
        &manifest,
        "project: { name: Demo, bundle_id: com.example.demo, platform: ios, arch: arm64, min_ios: '14.0' }\n\
runtime: { config_segment: __IAOTCFG, cave_segment: __IAOTCAVE, page_size: 16384, dylib_name: Demo.dylib }\n\
app: {}\n\
analysis: { runtime_il2cpp: false }\n\
generated_hooks:\n\
  - { name: DisabledInstaller, enabled: false, symbol: IAOT_DisabledInstaller }\n\
  - { name: EnabledInstaller, enabled: true, symbol: IAOT_EnabledInstaller }\n\
  - { name: DefaultInstaller, symbol: IAOT_DefaultInstaller }\n",
    )
    .unwrap();

    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_iaot"))
        .canonicalize()
        .unwrap();
    let result = Command::new(executable)
        .args(["generate", manifest.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );

    let hooks =
        std::fs::read_to_string(directory.path().join("generated/IAOTGeneratedHooks.mm")).unwrap();
    assert!(!hooks.contains("IAOT_DisabledInstaller"));
    for symbol in ["IAOT_EnabledInstaller", "IAOT_DefaultInstaller"] {
        assert!(hooks.contains(&format!("extern \"C\" bool {symbol}(")));
        assert!(hooks.contains(&format!(", {symbol} }});")));
    }
    assert!(hooks.contains("staticPatches=0 objectiveC=0 installers=2 callbackSlots=0"));

    let summary: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(directory.path().join("generated/hook-manifest.yaml")).unwrap(),
    )
    .unwrap();
    let installers = summary["generated_hooks"].as_sequence().unwrap();
    assert_eq!(installers.len(), 3);
    assert_eq!(installers[0]["enabled"].as_bool(), Some(false));
    assert_eq!(installers[1]["enabled"].as_bool(), Some(true));
    assert_eq!(installers[2]["enabled"].as_bool(), Some(true));
}
