use std::process::Command;
use tempfile::tempdir;

#[test]
fn new_project_contains_unity_and_overlay_defaults() {
    let directory = tempdir().unwrap();
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_iaot"))
        .canonicalize()
        .unwrap();
    let result = Command::new(executable)
        .current_dir(directory.path())
        .args(["new", "OwnedHooks"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let manifest = std::fs::read_to_string(directory.path().join("OwnedHooks/hooks.yaml")).unwrap();
    let config = std::fs::read_to_string(directory.path().join("OwnedHooks/configs.yaml")).unwrap();
    let makefile = std::fs::read_to_string(directory.path().join("OwnedHooks/Makefile")).unwrap();
    assert!(manifest.contains("project_config: configs.yaml"));
    assert!(config.contains("prefer_unity_framework: true"));
    assert!(directory
        .path()
        .join("OwnedHooks/il2cpp_config.yaml")
        .is_file());
    assert!(makefile.contains("IAOT_ENABLE_OVERLAY ?= 1"));
    assert!(makefile.contains("IAOT_OVERLAY_BACKEND ?= auto"));
    assert!(makefile.contains("runtime/overlay/IAOTOverlay.mk"));
    assert!(directory
        .path()
        .join("OwnedHooks/user/UserHooks.mm")
        .is_file());
    assert!(directory
        .path()
        .join("OwnedHooks/user/UserMenu.mm")
        .is_file());
    assert!(directory
        .path()
        .join("OwnedHooks/generated/IAOTGeneratedProject.mk")
        .is_file());
    let settings = std::fs::read_to_string(
        directory
            .path()
            .join("OwnedHooks/generated/IAOTGeneratedProject.mk"),
    )
    .unwrap();
    assert!(settings.contains("IAOT_BUILD_MODE := development"));
    assert!(settings.contains("IAOT_OVERLAY_AUTOSTART := 0"));
    let hook_manifest = std::fs::read_to_string(
        directory
            .path()
            .join("OwnedHooks/generated/hook-manifest.yaml"),
    )
    .unwrap();
    assert!(hook_manifest.contains("failure_behavior: disable_hook"));
}
