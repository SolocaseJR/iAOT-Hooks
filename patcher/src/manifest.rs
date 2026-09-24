use crate::errors::IaotError;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    pub project: Project,
    pub runtime: Runtime,
    pub app: App,
    #[serde(default)]
    pub paths: Paths,
    #[serde(default)]
    pub build: Build,
    #[serde(default)]
    pub analysis: Analysis,
    #[serde(default)]
    pub config: Config,
    #[serde(default)]
    pub functions: Vec<Function>,
    #[serde(default)]
    pub generated_hooks: Vec<GeneratedHook>,
    #[serde(default)]
    pub patches: Vec<Patch>,
    #[serde(skip)]
    pub source_dir: PathBuf,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Project {
    pub name: String,
    pub bundle_id: String,
    pub platform: String,
    pub arch: String,
    pub min_ios: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Runtime {
    pub config_segment: String,
    pub cave_segment: String,
    pub page_size: u64,
    pub dylib_name: String,
    /// Optional path to an IL2CPP metadata dump. This can be a directory or a
    /// single file, in any of the auto-detected formats (see il2cpp_metadata.rs):
    ///   * a Cpp2IL analysis package (cpp2il_methods.jsonl [+ cpp2il_types.jsonl])
    ///   * an Il2CppDumper `script.json` (methods) and/or `dump.cs` (+ fields)
    /// When set, any offline_* entry and the string factory may be specified by
    /// name only and their address/offset is resolved from the dump at generate
    /// time instead of being hand-typed. Resolved relative to source_dir.
    #[serde(default)]
    pub metadata_package: Option<PathBuf>,
    /// Optional override for the metadata dump format: "auto" (default),
    /// "cpp2il", or "il2cpp_dumper". Leave unset to detect from the files present.
    #[serde(default)]
    pub metadata_format: Option<String>,
    /// Optional RVA of the target's own UTF-16 managed-string constructor.
    /// This is used when a stripped Unity IL2CPP binary does not export
    /// il2cpp_string_new but still contains its generated String runtime.
    /// When `string_factory` (name-based) and `metadata_package` are set this
    /// is resolved automatically; an explicit value here still wins.
    #[serde(default)]
    pub string_factory_rva: Option<u64>,
    /// Optional name-based spec for the managed-string constructor, resolved
    /// against metadata_package into string_factory_rva.
    #[serde(default)]
    pub string_factory: Option<OfflineMethod>,
    #[serde(default)]
    pub offline_classes: Vec<OfflineClass>,
    #[serde(default)]
    pub offline_methods: Vec<OfflineMethod>,
    #[serde(default)]
    pub offline_fields: Vec<OfflineField>,
    #[serde(default)]
    pub offline_internal_calls: Vec<OfflineInternalCall>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct OfflineClass {
    pub module: String,
    pub namespace: String,
    pub name: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct OfflineMethod {
    pub module: String,
    pub namespace: String,
    #[serde(rename = "class")]
    pub class_name: String,
    pub name: String,
    #[serde(default = "default_parameter_count")]
    pub parameter_count: i32,
    /// Optional exact managed parameter-type list (e.g. ["System.Char*"]) used
    /// to disambiguate overloads that share an arity when resolving from the
    /// metadata package. Takes precedence over parameter_count for matching.
    #[serde(default)]
    pub parameter_types: Option<Vec<String>>,
    /// Explicit native RVA. Optional when metadata_package is set (resolved by
    /// name); an explicit value always wins over dump resolution.
    #[serde(default)]
    pub rva: Option<u64>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct OfflineField {
    pub module: String,
    pub namespace: String,
    #[serde(rename = "class")]
    pub class_name: String,
    pub name: String,
    /// Explicit field offset. Optional when metadata_package is set.
    #[serde(default)]
    pub offset: Option<u64>,
    /// Field size in bytes. Optional; filled from the dump, else defaults to a
    /// pointer width when neither the manifest nor the dump provides it.
    #[serde(default)]
    pub size: Option<u64>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct OfflineInternalCall {
    pub name: String,
    pub module: String,
    /// Explicit native RVA. Optional when metadata_package is set.
    #[serde(default)]
    pub rva: Option<u64>,
}
fn default_parameter_count() -> i32 {
    -1
}
/// Fallback field width (pointer size) when neither the manifest nor the
/// metadata package supplies a size for an offline field.
pub const DEFAULT_FIELD_SIZE: u64 = 8;
#[derive(Clone, Debug, Deserialize)]
pub struct App {
    /// Remove the device-model allowlist from the main app's Info.plist.
    #[serde(default)]
    pub remove_supported_devices: bool,
    pub executable_member: Option<String>,
    #[serde(default)]
    pub prefer_unity_framework: bool,
    #[serde(default)]
    pub target_module: Option<String>,
    /// Optional SpringBoard/home-screen name override.
    #[serde(default, alias = "app_name")]
    pub display_name: Option<String>,
    /// Optional square PNG used to generate the complete iPhone/iPad icon set.
    /// Relative paths are resolved from the hook-project directory.
    #[serde(default, alias = "icon")]
    pub icon_path: Option<PathBuf>,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Paths {
    pub input_ipa: Option<PathBuf>,
    pub output_ipa: Option<PathBuf>,
    pub build_dir: Option<PathBuf>,
    pub generated_dir: Option<PathBuf>,
    pub temp_dir: Option<PathBuf>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Build {
    pub mode: String,
    pub log_level: String,
    pub menu: bool,
    pub persistence: bool,
    #[serde(default = "default_true")]
    pub runtime_autostart: bool,
    /// Build only a no-op dylib.  This is a launch-diagnostics control and
    /// deliberately omits all IAOT runtime, hook, overlay, and ObjC++ code.
    #[serde(default)]
    pub minimal_runtime: bool,
    pub clean_temporary: bool,
    pub signing: String,
}
impl Default for Build {
    fn default() -> Self {
        Self {
            mode: "development".into(),
            log_level: "info".into(),
            menu: true,
            persistence: true,
            runtime_autostart: true,
            minimal_runtime: false,
            clean_temporary: true,
            signing: "external".into(),
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Analysis {
    pub application_type: String,
    pub scripting_backend: String,
    pub rendering_backend: String,
    pub il2cpp_module: Option<String>,
    pub static_il2cpp: bool,
    pub runtime_il2cpp: bool,
    pub il2cpp_method_manifest: Option<PathBuf>,
    pub external_analyzer: String,
}
impl Default for Analysis {
    fn default() -> Self {
        Self {
            application_type: "auto".into(),
            scripting_backend: "auto".into(),
            rendering_backend: "auto".into(),
            il2cpp_module: None,
            static_il2cpp: true,
            runtime_il2cpp: true,
            il2cpp_method_manifest: None,
            external_analyzer: "auto".into(),
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub flags: Vec<Flag>,
    #[serde(default)]
    pub strings: Vec<StringValue>,
    #[serde(default)]
    pub callbacks: Vec<Callback>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Flag {
    pub name: String,
    pub index: u32,
    pub default: bool,
}
#[derive(Clone, Debug, Deserialize)]
pub struct StringValue {
    pub name: String,
    pub index: u32,
    pub default: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Callback {
    pub name: String,
    pub index: u32,
    pub symbol: String,
    pub signature: Option<String>,
    #[serde(default)]
    pub auto_register: bool,
}
#[derive(Clone, Debug, Deserialize)]
pub struct GeneratedHook {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub symbol: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Function {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub signature: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    pub address: Option<FunctionAddress>,
    #[serde(rename = "class")]
    pub class_name: Option<String>,
    pub selector: Option<String>,
    #[serde(default)]
    pub class_method: bool,
    pub replacement: Option<String>,
    #[serde(default = "default_true")]
    pub preserve_original: bool,
}
#[derive(Clone, Debug, Deserialize)]
pub struct FunctionAddress {
    #[serde(rename = "type")]
    pub kind: String,
    pub value: Option<u64>,
    pub symbol: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Patch {
    pub name: String,
    pub enabled: bool,
    pub description: String,
    pub locator: Locator,
    pub patch: PatchOperation,
    /// Single-declaration shorthand for a branch-to-callback patch. When set,
    /// the loader creates and auto-registers the callback entry, using
    /// `signature` and an automatically allocated callback index.
    #[serde(default)]
    pub replacement: Option<String>,
    /// Optional explicit callback slot for `replacement`. Most projects should
    /// omit this and let IAOT allocate the first available slot.
    #[serde(default)]
    pub callback_index: Option<u32>,
    /// Allocate an original trampoline automatically. The generated accessor
    /// remains `iaot::generated::Original_<patch name>()`.
    #[serde(default)]
    pub preserve_original: bool,
    #[serde(default)]
    pub expect: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub original_slot: Option<u32>,
    #[serde(default)]
    pub failure_behavior: FailureBehavior,
    #[serde(default)]
    pub architectures: Vec<String>,
    #[serde(default)]
    pub versions: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureBehavior {
    #[default]
    Abort,
    DisableHook,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Locator {
    Pattern {
        pattern: String,
        match_index: Option<usize>,
        expected_matches: Option<usize>,
        segment: Option<String>,
        section: Option<String>,
        fallback_rva: Option<u64>,
    },
    Xref {
        pattern: String,
        xref_pattern: Option<String>,
        match_index: Option<usize>,
        xref_match_index: Option<usize>,
        xref_bl_index: Option<usize>,
    },
    /// Several independent signatures for one target. Every signature that
    /// resolves must agree; `fallback` is used only when none resolves and
    /// the detected app/Unity version equals `fallback.version`.
    Signatures {
        signatures: Vec<Signature>,
        fallback: Option<VersionedRva>,
    },
    Symbol {
        symbol: String,
    },
    Export {
        symbol: String,
    },
    Va {
        value: u64,
    },
    Vmaddr {
        value: u64,
    },
    Rva {
        value: u64,
    },
    Il2cpp {
        assembly: String,
        namespace: String,
        class: String,
        method: String,
        parameter_count: Option<u32>,
        method_id: Option<String>,
        rva: Option<u64>,
        manifest: Option<PathBuf>,
    },
    Custom {
        resolver: String,
        value: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SignatureKind {
    /// The match (plus `offset`) is the target.
    Direct,
    /// The match contains a B or BL at `branch_index` whose target is the target.
    Xref,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Signature {
    pub kind: SignatureKind,
    pub pattern: String,
    /// Instruction index of the B/BL inside an xref pattern (default 0).
    #[serde(default)]
    pub branch_index: Option<usize>,
    /// Byte offset from a direct match to the target (default 0).
    #[serde(default)]
    pub offset: Option<i64>,
    /// Pick one of several matches instead of requiring exactly one.
    #[serde(default)]
    pub match_index: Option<usize>,
}

/// A build-specific address, trusted only for the version it was taken from.
#[derive(Clone, Debug, Deserialize)]
pub struct VersionedRva {
    pub rva: u64,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PatchOperation {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub action: Option<PatchAction>,
    #[serde(default)]
    pub instruction_offset: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatchAction {
    RetIfFlag {
        flag: String,
    },
    ZeroRegisterIfFlag {
        flag: String,
        register: u8,
    },
    CallbackIfFlag {
        flag: String,
        #[serde(default)]
        callback: Option<String>,
    },
    CallbackReturnZeroIfFlag {
        flag: String,
        #[serde(default)]
        callback: Option<String>,
    },
    CallbackAfter {
        #[serde(default)]
        callback: Option<String>,
    },
    BranchToCallback {
        #[serde(default)]
        callback: Option<String>,
    },
    BranchToReplacement {
        symbol: String,
    },
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Default, Deserialize)]
struct ManifestReferences {
    #[serde(default)]
    project_config: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
struct Il2CppConfigReference {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    config_file: Option<PathBuf>,
}

fn read_yaml(path: &Path, label: &str) -> anyhow::Result<serde_yaml::Value> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("cannot read {label} {}", path.display()))?;
    serde_yaml::from_str(&text).with_context(|| format!("cannot parse {label} {}", path.display()))
}

fn yaml_field<'a>(value: &'a serde_yaml::Value, name: &str) -> Option<&'a serde_yaml::Value> {
    value
        .as_mapping()?
        .get(serde_yaml::Value::String(name.to_owned()))
}

fn merge_yaml(base: &mut serde_yaml::Value, overlay: serde_yaml::Value) {
    match (base, overlay) {
        (serde_yaml::Value::Mapping(base_map), serde_yaml::Value::Mapping(overlay_map)) => {
            for (key, value) in overlay_map {
                if let Some(existing) = base_map.get_mut(&key) {
                    merge_yaml(existing, value);
                } else {
                    base_map.insert(key, value);
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

fn force_il2cpp_disabled(value: &mut serde_yaml::Value) -> anyhow::Result<()> {
    let root = value
        .as_mapping_mut()
        .ok_or_else(|| anyhow::anyhow!("configs.yaml root must be a mapping"))?;
    let analysis_key = serde_yaml::Value::String("analysis".into());
    let analysis = root
        .entry(analysis_key)
        .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()));
    let analysis = analysis
        .as_mapping_mut()
        .ok_or_else(|| anyhow::anyhow!("configs.yaml analysis must be a mapping"))?;
    analysis.insert(
        serde_yaml::Value::String("static_il2cpp".into()),
        serde_yaml::Value::Bool(false),
    );
    analysis.insert(
        serde_yaml::Value::String("runtime_il2cpp".into()),
        serde_yaml::Value::Bool(false),
    );
    Ok(())
}

pub fn load(path: &Path) -> anyhow::Result<Manifest> {
    let hooks = read_yaml(path, "hook manifest")?;
    let references: ManifestReferences = serde_yaml::from_value(hooks.clone())
        .with_context(|| format!("cannot read references from {}", path.display()))?;
    let source_dir = path.parent().unwrap_or(Path::new("."));

    let effective = if let Some(configured) = references.project_config {
        let config_path = if configured.is_absolute() {
            configured
        } else {
            source_dir.join(configured)
        };
        let mut merged = read_yaml(&config_path, "project config")?;
        let il2cpp: Il2CppConfigReference = yaml_field(&merged, "il2cpp")
            .cloned()
            .map(serde_yaml::from_value)
            .transpose()
            .with_context(|| {
                format!(
                    "cannot parse il2cpp selection in project config {}",
                    config_path.display()
                )
            })?
            .unwrap_or_default();

        if il2cpp.enabled {
            let configured = il2cpp.config_file.ok_or_else(|| {
                anyhow::anyhow!(
                    "il2cpp.enabled is true but il2cpp.config_file is missing in {}",
                    config_path.display()
                )
            })?;
            let config_dir = config_path.parent().unwrap_or(source_dir);
            let il2cpp_path = if configured.is_absolute() {
                configured
            } else {
                config_dir.join(configured)
            };
            merge_yaml(&mut merged, read_yaml(&il2cpp_path, "IL2CPP config")?);
        } else {
            // Analysis defaults historically enable both IL2CPP paths. An
            // explicit disabled selector must override those defaults even
            // when no IL2CPP file is loaded.
            force_il2cpp_disabled(&mut merged)?;
        }

        // hooks.yaml is the final layer so a hook project may intentionally
        // override a shared base config without mutating the shared file.
        merge_yaml(&mut merged, hooks);
        merged
    } else {
        // Backward compatibility for existing single-file manifests.
        hooks
    };

    let mut manifest: Manifest = serde_yaml::from_value(effective)
        .with_context(|| format!("cannot construct manifest from {}", path.display()))?;
    manifest.source_dir = source_dir.to_path_buf();
    expand_replacement_shorthand(&mut manifest)?;
    Ok(manifest)
}

fn first_free_slot(used: &HashSet<u32>, capacity: u32, label: &str) -> anyhow::Result<u32> {
    (0..capacity)
        .find(|index| !used.contains(index))
        .ok_or_else(|| anyhow::anyhow!("no free {label} slots remain (capacity {capacity})"))
}

/// Convert the concise, user-facing replacement form into the same callback
/// and patch representation used by legacy manifests. Keeping one normalized
/// internal representation means the cave writer and runtime do not need a
/// second implementation path.
fn expand_replacement_shorthand(manifest: &mut Manifest) -> anyhow::Result<()> {
    let mut callback_names: HashSet<String> = manifest
        .config
        .callbacks
        .iter()
        .map(|callback| callback.name.clone())
        .collect();
    let mut callback_slots: HashSet<u32> = manifest
        .config
        .callbacks
        .iter()
        .map(|callback| callback.index)
        .collect();
    let mut original_slots: HashSet<u32> = manifest
        .patches
        .iter()
        .filter(|patch| patch.enabled)
        .filter_map(|patch| patch.original_slot)
        .collect();

    for patch in &mut manifest.patches {
        // Disabled patches are not emitted or applied, so they must not consume
        // finite runtime callback/original slots during manifest resolution.
        if !patch.enabled {
            continue;
        }

        let Some(symbol) = patch.replacement.clone() else {
            if patch.callback_index.is_some() {
                anyhow::bail!(
                    "patch {} sets callback_index without replacement",
                    patch.name
                );
            }
            if patch.preserve_original && patch.original_slot.is_none() {
                patch.original_slot =
                    Some(first_free_slot(&original_slots, 64, "original trampoline")?);
                original_slots.insert(patch.original_slot.unwrap());
            }
            continue;
        };

        if patch.patch.kind != "branch_to_cave" {
            anyhow::bail!(
                "patch {} replacement shorthand requires patch.type branch_to_cave",
                patch.name
            );
        }
        if !c_symbol(&patch.name) {
            anyhow::bail!(
                "patch {} needs a C-identifier name when replacement shorthand is used",
                patch.name
            );
        }
        if !c_symbol(&symbol) {
            anyhow::bail!(
                "patch {} has invalid replacement symbol {}",
                patch.name,
                symbol
            );
        }
        let signature = patch.signature.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "patch {} replacement shorthand requires an exact signature",
                patch.name
            )
        })?;
        if !valid_signature(&signature) {
            anyhow::bail!(
                "patch {} replacement shorthand has an invalid signature",
                patch.name
            );
        }
        if !callback_names.insert(patch.name.clone()) {
            anyhow::bail!(
                "patch {} replacement shorthand collides with an existing callback name",
                patch.name
            );
        }

        let callback_index = match patch.callback_index {
            Some(index) => {
                if index >= 64 {
                    anyhow::bail!(
                        "patch {} callback_index exceeds runtime capacity",
                        patch.name
                    );
                }
                if !callback_slots.insert(index) {
                    anyhow::bail!(
                        "patch {} callback_index {} is already in use",
                        patch.name,
                        index
                    );
                }
                index
            }
            None => {
                let index = first_free_slot(&callback_slots, 64, "callback")?;
                callback_slots.insert(index);
                index
            }
        };

        match &mut patch.patch.action {
            None => {
                patch.patch.action = Some(PatchAction::BranchToCallback {
                    callback: Some(patch.name.clone()),
                });
            }
            Some(
                PatchAction::CallbackIfFlag { callback, .. }
                | PatchAction::CallbackReturnZeroIfFlag { callback, .. }
                | PatchAction::CallbackAfter { callback }
                | PatchAction::BranchToCallback { callback },
            ) if callback.as_deref().is_none_or(|name| name == patch.name) => {
                *callback = Some(patch.name.clone());
            }
            Some(_) => {
                anyhow::bail!(
                    "patch {} replacement shorthand requires a callback action and may not reference another callback",
                    patch.name
                )
            }
        }

        manifest.config.callbacks.push(Callback {
            name: patch.name.clone(),
            index: callback_index,
            symbol,
            signature: Some(signature),
            // A disabled shorthand patch should not force an otherwise unused
            // replacement symbol to be implemented and linked.
            auto_register: patch.enabled,
        });

        if patch.preserve_original && patch.original_slot.is_none() {
            let slot = first_free_slot(&original_slots, 64, "original trampoline")?;
            original_slots.insert(slot);
            patch.original_slot = Some(slot);
        }
    }
    Ok(())
}
fn c_symbol(s: &str) -> bool {
    let mut it = s.chars();
    matches!(it.next(),Some(c) if c=='_'||c.is_ascii_alphabetic())
        && it.all(|c| c == '_' || c.is_ascii_alphanumeric())
}
fn unique<T: std::hash::Hash + Eq + Copy>(
    values: impl Iterator<Item = T>,
    what: &str,
) -> Result<(), IaotError> {
    let mut set = HashSet::new();
    for v in values {
        if !set.insert(v) {
            return Err(IaotError::Manifest(format!("duplicate {what} index")));
        }
    }
    Ok(())
}
fn validate_signatures(
    name: &str,
    signatures: &[Signature],
    fallback: Option<&VersionedRva>,
) -> Result<(), IaotError> {
    let invalid = |detail: String| Err(IaotError::Manifest(format!("patch {name} {detail}")));
    if signatures.is_empty() {
        return invalid("needs at least one signature".into());
    }
    for (index, signature) in signatures.iter().enumerate() {
        let number = index + 1;
        let Ok(parsed) = crate::pattern::parse(&signature.pattern) else {
            return invalid(format!("signature {number} has an invalid pattern"));
        };
        if parsed.len() != 1 || parsed[0].0.len() % 4 != 0 {
            return invalid(format!(
                "signature {number} must be one alternative of whole instructions"
            ));
        }
        match signature.kind {
            SignatureKind::Direct => {
                if signature.branch_index.is_some() {
                    return invalid(format!("signature {number} is direct but sets branch_index"));
                }
                if signature.offset.is_some_and(|offset| offset % 4 != 0) {
                    return invalid(format!("signature {number} offset is unaligned"));
                }
            }
            SignatureKind::Xref => {
                if signature.offset.is_some() {
                    return invalid(format!("signature {number} is an xref but sets offset"));
                }
                let branch = signature.branch_index.unwrap_or(0);
                // The top byte of a B (0x14-0x17) or BL (0x94-0x97), or a wildcard.
                let top = parsed[0].0.get(branch * 4 + 3).copied();
                let is_branch = matches!(top, Some(None))
                    || matches!(top, Some(Some(byte)) if byte & 0x7c == 0x14);
                if !is_branch {
                    return invalid(format!(
                        "signature {number} has no B/BL at instruction {branch}"
                    ));
                }
            }
        }
    }
    if let Some(fallback) = fallback {
        if fallback.rva % 4 != 0 {
            return invalid("fallback rva is unaligned".into());
        }
        if fallback.version.trim().is_empty() {
            return invalid("fallback needs the version its rva was taken from".into());
        }
    }
    Ok(())
}
pub fn validate(m: &Manifest) -> Result<(), IaotError> {
    if m.project.name.is_empty()
        || m.project.bundle_id.is_empty()
        || m.runtime.dylib_name.is_empty()
        || m.project.min_ios.is_empty()
    {
        return Err(IaotError::Manifest(
            "project identity, minimum iOS, and dylib name are required".into(),
        ));
    }
    if m.runtime.dylib_name.contains(['/', '\\'])
        || !m.runtime.dylib_name.is_ascii()
        || m.runtime.dylib_name.len() > 255
    {
        return Err(IaotError::Manifest(
            "runtime.dylib_name must be an ASCII filename of at most 255 bytes, not a path".into(),
        ));
    }
    if let Some(display_name) = &m.app.display_name {
        let trimmed = display_name.trim();
        if trimmed.is_empty()
            || trimmed.chars().count() > 255
            || trimmed.chars().any(char::is_control)
        {
            return Err(IaotError::Manifest(
                "app.display_name must contain 1-255 non-control characters".into(),
            ));
        }
    }
    if let Some(icon_path) = &m.app.icon_path {
        if icon_path
            .extension()
            .and_then(|value| value.to_str())
            .is_none_or(|value| !value.eq_ignore_ascii_case("png"))
        {
            return Err(IaotError::Manifest(
                "app.icon_path must point to a PNG file".into(),
            ));
        }
        let resolved = if icon_path.is_absolute() {
            icon_path.clone()
        } else {
            m.source_dir.join(icon_path)
        };
        if !resolved.is_file() {
            return Err(IaotError::Manifest(format!(
                "app.icon_path does not exist: {}",
                resolved.display()
            )));
        }
    }
    if m.project.platform != "ios" || m.project.arch != "arm64" {
        return Err(IaotError::Manifest(
            "only platform ios / arch arm64 is supported".into(),
        ));
    }
    for value in [&m.build.mode, &m.build.log_level, &m.build.signing] {
        if value.trim().is_empty() {
            return Err(IaotError::Manifest(
                "build settings may not be empty".into(),
            ));
        }
    }
    if !matches!(m.build.mode.as_str(), "development" | "release") {
        return Err(IaotError::Manifest(
            "build.mode must be development or release".into(),
        ));
    }
    if !matches!(
        m.build.log_level.as_str(),
        "debug" | "info" | "warning" | "error" | "off"
    ) {
        return Err(IaotError::Manifest(
            "build.log_level must be debug, info, warning, error, or off".into(),
        ));
    }
    if !matches!(
        m.analysis.application_type.as_str(),
        "auto"
            | "unity_il2cpp"
            | "unity_mono"
            | "unity_unknown_backend"
            | "native_objective_c"
            | "native_objective_cpp"
            | "native_cpp"
            | "swift"
            | "mixed_native"
            | "unknown_or_native_c"
    ) {
        return Err(IaotError::Manifest(
            "analysis.application_type contains an unsupported override".into(),
        ));
    }
    if !matches!(
        m.analysis.scripting_backend.as_str(),
        "auto" | "il2cpp" | "mono" | "unknown" | "native"
    ) {
        return Err(IaotError::Manifest(
            "analysis.scripting_backend must be auto, il2cpp, mono, unknown, or native".into(),
        ));
    }
    if !matches!(
        m.analysis.external_analyzer.as_str(),
        "auto" | "none" | "method_manifest"
    ) {
        return Err(IaotError::Manifest(
            "analysis.external_analyzer must be auto, none, or method_manifest".into(),
        ));
    }
    if !matches!(
        m.analysis.rendering_backend.as_str(),
        "auto" | "metal" | "disabled"
    ) {
        return Err(IaotError::Manifest(
            "analysis.rendering_backend must be auto, metal, or disabled".into(),
        ));
    }
    for s in [&m.runtime.config_segment, &m.runtime.cave_segment] {
        if !s.starts_with("__") || s.len() > 16 || !s.is_ascii() {
            return Err(IaotError::Manifest(format!(
                "invalid Mach-O segment name {s}"
            )));
        }
    }
    if m.runtime.config_segment == m.runtime.cave_segment {
        return Err(IaotError::Manifest(
            "config and cave segments must differ".into(),
        ));
    }
    if !(4096..=65536).contains(&m.runtime.page_size) || !m.runtime.page_size.is_power_of_two() {
        return Err(IaotError::Manifest(
            "runtime.page_size must be a power of two from 4096 through 65536".into(),
        ));
    }
    if let Some(member) = &m.app.executable_member {
        if !member.starts_with("Payload/") || member.contains("..") || member.ends_with('/') {
            return Err(IaotError::Manifest(
                "app.executable_member must be an explicit safe Payload/... member".into(),
            ));
        }
    }
    unique(m.config.flags.iter().map(|x| x.index), "flag")?;
    unique(m.config.strings.iter().map(|x| x.index), "string")?;
    unique(m.config.callbacks.iter().map(|x| x.index), "callback")?;
    unique(
        m.patches.iter().filter_map(|x| x.original_slot),
        "original trampoline",
    )?;
    if m.config.flags.iter().any(|x| x.index >= 256)
        || m.config.strings.iter().any(|x| x.index >= 32)
        || m.config.callbacks.iter().any(|x| x.index >= 64)
    {
        return Err(IaotError::Manifest(
            "config index exceeds runtime capacity".into(),
        ));
    }
    for name in m
        .config
        .flags
        .iter()
        .map(|x| &x.name)
        .chain(m.config.strings.iter().map(|x| &x.name))
        .chain(m.config.callbacks.iter().map(|x| &x.name))
    {
        if !c_symbol(name) {
            return Err(IaotError::Manifest(format!(
                "config name is not a C identifier: {name}"
            )));
        }
    }
    let mut callback_signatures = HashMap::new();
    for callback in &m.config.callbacks {
        if !c_symbol(&callback.symbol) {
            return Err(IaotError::Manifest(format!(
                "invalid callback symbol {}",
                callback.symbol
            )));
        }
        if callback.auto_register
            && callback
                .signature
                .as_deref()
                .is_none_or(|value| !valid_signature(value))
        {
            return Err(IaotError::Manifest(format!(
                "auto-registered callback {} needs a valid signature",
                callback.name
            )));
        }
        if callback.auto_register {
            let signature = callback.signature.as_deref().unwrap();
            if let Some(previous) = callback_signatures.insert(callback.symbol.as_str(), signature)
            {
                if previous != signature {
                    return Err(IaotError::Manifest(format!(
                        "callback symbol {} is registered with conflicting signatures",
                        callback.symbol
                    )));
                }
            }
        }
    }
    if m.config.strings.iter().any(|x| x.default.len() >= 256) {
        return Err(IaotError::Manifest(
            "string default exceeds the 255-byte fixed-string capacity".into(),
        ));
    }
    for h in &m.generated_hooks {
        if !c_symbol(&h.symbol) {
            return Err(IaotError::Manifest(format!(
                "invalid generated hook symbol {}",
                h.symbol
            )));
        }
    }
    let mut function_names = HashSet::new();
    for f in &m.functions {
        if !c_symbol(&f.name) || !function_names.insert(&f.name) {
            return Err(IaotError::Manifest(format!(
                "invalid or duplicate function name {}",
                f.name
            )));
        }
        if f.kind.as_deref() == Some("objc") {
            if f.class_name.as_deref().unwrap_or("").is_empty()
                || f.selector.as_deref().unwrap_or("").is_empty()
            {
                return Err(IaotError::Manifest(format!(
                    "Objective-C function {} needs class and selector",
                    f.name
                )));
            }
            let signature = f.signature.as_deref().ok_or_else(|| {
                IaotError::Manifest(format!(
                    "Objective-C hook {} needs an exact replacement signature",
                    f.name
                ))
            })?;
            if !valid_signature(signature) {
                return Err(IaotError::Manifest(format!(
                    "Objective-C hook {} has an invalid signature",
                    f.name
                )));
            }
            let replacement = f.replacement.as_deref().unwrap_or("");
            if !c_symbol(replacement) {
                return Err(IaotError::Manifest(format!(
                    "Objective-C hook {} needs a valid replacement symbol",
                    f.name
                )));
            }
            if f.address.is_some() {
                return Err(IaotError::Manifest(format!(
                    "Objective-C hook {} must not specify a native address",
                    f.name
                )));
            }
        } else {
            let sig = f.signature.as_deref().ok_or_else(|| {
                IaotError::Manifest(format!("function {} needs signature", f.name))
            })?;
            if !valid_signature(sig) {
                return Err(IaotError::Manifest(format!(
                    "obviously invalid signature for {}",
                    f.name
                )));
            }
            let a = f
                .address
                .as_ref()
                .ok_or_else(|| IaotError::Manifest(format!("function {} needs address", f.name)))?;
            match a.kind.as_str() {
                "vmaddr" | "rva" | "runtime" if a.value.unwrap_or(0) != 0 => {}
                "symbol" if !a.symbol.as_deref().unwrap_or("").is_empty() => {}
                _ => {
                    return Err(IaotError::Manifest(format!(
                        "function {} has an incomplete or unsupported address",
                        f.name
                    )))
                }
            }
        }
    }
    let mut generated_api_names = HashSet::new();
    for function in &m.functions {
        let names = if function.kind.as_deref() == Some("objc") {
            vec![
                format!("Original_{}", function.name),
                format!("OriginalIMP_{}", function.name),
                format!("Install_{}", function.name),
            ]
        } else {
            vec![function.name.clone()]
        };
        for name in names {
            if !generated_api_names.insert(name.clone()) {
                return Err(IaotError::Manifest(format!(
                    "generated API name collides: {name}"
                )));
            }
        }
    }
    for patch in m
        .patches
        .iter()
        .filter(|patch| patch.original_slot.is_some())
    {
        let name = format!("Original_{}", patch.name);
        if !generated_api_names.insert(name.clone()) {
            return Err(IaotError::Manifest(format!(
                "generated API name collides: {name}"
            )));
        }
    }
    let flags: HashSet<_> = m.config.flags.iter().map(|x| x.name.as_str()).collect();
    let callbacks: HashSet<_> = m.config.callbacks.iter().map(|x| x.name.as_str()).collect();
    for patch in &m.patches {
        if patch.name.is_empty() || patch.description.is_empty() {
            return Err(IaotError::Manifest(
                "patch name and description are required".into(),
            ));
        }
        // Disabled patches are retained as project documentation/configuration,
        // but they do not participate in callback wiring or binary mutation.
        if !patch.enabled {
            continue;
        }
        match patch.patch.kind.as_str() {
            "branch_to_cave"
                if patch.patch.action.is_some() && patch.patch.instruction_offset.is_none() => {}
            "branch_to_instruction_target"
                if patch.patch.action.is_none()
                    && patch
                        .patch
                        .instruction_offset
                        .is_none_or(|offset| offset % 4 == 0) => {}
            "branch_to_cave" => {
                return Err(IaotError::Manifest(format!(
                    "patch {} branch_to_cave requires action and forbids instruction_offset",
                    patch.name
                )))
            }
            "branch_to_instruction_target" => {
                return Err(IaotError::Manifest(format!(
                    "patch {} branch_to_instruction_target requires an aligned instruction_offset and forbids action",
                    patch.name
                )))
            }
            _ => {
                return Err(IaotError::Manifest(format!(
                    "patch {} has unsupported patch type {}",
                    patch.name, patch.patch.kind
                )))
            }
        }
        match &patch.locator {
            Locator::Pattern { pattern, .. } | Locator::Xref { pattern, .. }
                if crate::pattern::parse(pattern).is_err() =>
            {
                return Err(IaotError::Manifest(format!(
                    "patch {} has an invalid pattern",
                    patch.name
                )))
            }
            Locator::Signatures {
                signatures,
                fallback,
            } => validate_signatures(&patch.name, signatures, fallback.as_ref())?,
            Locator::Symbol { symbol } | Locator::Export { symbol } if symbol.is_empty() => {
                return Err(IaotError::Manifest(format!(
                    "patch {} has an empty symbol",
                    patch.name
                )))
            }
            Locator::Va { value } | Locator::Vmaddr { value } | Locator::Rva { value }
                if value % 4 != 0 =>
            {
                return Err(IaotError::Manifest(format!(
                    "patch {} address is unaligned",
                    patch.name
                )))
            }
            _ => {}
        }
        if patch.original_slot.is_some_and(|slot| slot >= 64) {
            return Err(IaotError::Manifest(format!(
                "patch {} original_slot exceeds runtime capacity",
                patch.name
            )));
        }
        if patch.original_slot.is_some() && !c_symbol(&patch.name) {
            return Err(IaotError::Manifest(format!(
                "patch {} needs a C-identifier name when original_slot is used",
                patch.name
            )));
        }
        if patch.original_slot.is_some()
            && !matches!(
                patch.patch.action.as_ref(),
                Some(PatchAction::BranchToCallback { .. })
            )
        {
            return Err(IaotError::Manifest(format!(
                "patch {} can preserve a typed original only with branch_to_callback",
                patch.name
            )));
        }
        if patch.original_slot.is_some()
            && patch
                .signature
                .as_deref()
                .is_none_or(|value| !valid_signature(value))
        {
            return Err(IaotError::Manifest(format!(
                "patch {} needs a valid signature when original_slot is used",
                patch.name
            )));
        }
        if patch.architectures.iter().any(|arch| arch != "arm64") {
            return Err(IaotError::Manifest(format!(
                "patch {} requests an unsupported architecture",
                patch.name
            )));
        }
        if let Some(action) = &patch.patch.action {
            match action {
                PatchAction::RetIfFlag { flag } | PatchAction::ZeroRegisterIfFlag { flag, .. }
                    if !flags.contains(flag.as_str()) =>
                {
                    return Err(IaotError::Manifest(format!(
                        "patch {} references an unknown flag",
                        patch.name
                    )))
                }
                PatchAction::CallbackIfFlag { flag, callback }
                | PatchAction::CallbackReturnZeroIfFlag { flag, callback }
                    if !flags.contains(flag.as_str())
                        || callback
                            .as_deref()
                            .is_none_or(|name| !callbacks.contains(name)) =>
                {
                    return Err(IaotError::Manifest(format!(
                        "patch {} references an unknown flag or callback",
                        patch.name
                    )))
                }
                PatchAction::CallbackAfter { callback }
                | PatchAction::BranchToCallback { callback }
                    if callback
                        .as_deref()
                        .is_none_or(|name| !callbacks.contains(name)) =>
                {
                    return Err(IaotError::Manifest(format!(
                        "patch {} references an unknown callback",
                        patch.name
                    )))
                }
                PatchAction::ZeroRegisterIfFlag { register, .. } if *register > 30 => {
                    return Err(IaotError::Manifest(format!(
                        "patch {} has an invalid register",
                        patch.name
                    )))
                }
                PatchAction::BranchToReplacement { symbol } if symbol.is_empty() => {
                    return Err(IaotError::Manifest(format!(
                        "patch {} has an empty replacement symbol",
                        patch.name
                    )))
                }
                _ => {}
            }
        }
        if let Some(expect) = &patch.expect {
            if expect == "auto" {
                continue;
            }
            let parsed = crate::pattern::parse(expect).map_err(|_| {
                IaotError::Manifest(format!("patch {} has invalid expected bytes", patch.name))
            })?;
            // Wildcards let one expectation hold across game versions, e.g.
            // "FF ?? ?? D1" for any `sub sp, sp, #imm`.
            if parsed.len() != 1 || parsed[0].0.len() != 4 {
                return Err(IaotError::Manifest(format!(
                    "patch {} expect must be exactly four bytes or wildcards",
                    patch.name
                )));
            }
        }
    }
    Ok(())
}

fn valid_signature(sig: &str) -> bool {
    sig.contains('(')
        && sig.ends_with(')')
        && !sig
            .chars()
            .any(|c| matches!(c, ';' | '{' | '}' | '#' | '\n' | '\r' | '"'))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(extra: &str) -> Result<Manifest, serde_yaml::Error> {
        serde_yaml::from_str(&format!("project: {{ name: Demo, bundle_id: com.example.demo, platform: ios, arch: arm64, min_ios: '14.0' }}\nruntime: {{ config_segment: __IAOTCFG, cave_segment: __IAOTCAVE, page_size: 16384, dylib_name: Demo.dylib }}\napp: {{ executable_member: Payload/Demo.app/Demo }}\nconfig: {{ flags: [], strings: [], callbacks: [] }}\nfunctions: []\ngenerated_hooks: []\npatches: []\n{extra}"))
    }
    #[test]
    fn valid_example() {
        assert!(validate(&parse("").unwrap()).is_ok());
    }
    #[test]
    fn generated_hook_enabled_defaults_to_true_and_honors_false() {
        let default_hook: GeneratedHook =
            serde_yaml::from_str("name: Default\nsymbol: InstallDefault\n").unwrap();
        let disabled_hook: GeneratedHook =
            serde_yaml::from_str("name: Disabled\nenabled: false\nsymbol: InstallDisabled\n")
                .unwrap();
        assert!(default_hook.enabled);
        assert!(!disabled_hook.enabled);
        assert!(serde_yaml::from_str::<GeneratedHook>(
            "name: Invalid\nenabled: definitely\nsymbol: InstallInvalid\n"
        )
        .is_err());
    }
    #[test]
    fn duplicate_flag() {
        let mut m = parse("").unwrap();
        m.config.flags = vec![
            Flag {
                name: "a".into(),
                index: 0,
                default: false,
            },
            Flag {
                name: "b".into(),
                index: 0,
                default: true,
            },
        ];
        assert!(validate(&m).is_err());
    }
    #[test]
    fn duplicate_string() {
        let mut m = parse("").unwrap();
        m.config.strings = vec![
            StringValue {
                name: "a".into(),
                index: 0,
                default: "x".into(),
            },
            StringValue {
                name: "b".into(),
                index: 0,
                default: "y".into(),
            },
        ];
        assert!(validate(&m).is_err());
    }
    #[test]
    fn duplicate_callback() {
        let mut m = parse("").unwrap();
        m.config.callbacks = vec![
            Callback {
                name: "a".into(),
                index: 0,
                symbol: "A".into(),
                signature: None,
                auto_register: false,
            },
            Callback {
                name: "b".into(),
                index: 0,
                symbol: "B".into(),
                signature: None,
                auto_register: false,
            },
        ];
        assert!(validate(&m).is_err());
    }
    #[test]
    fn conflicting_callback_symbol_signatures_are_rejected() {
        let mut m = parse("").unwrap();
        m.config.callbacks = vec![
            Callback {
                name: "first".into(),
                index: 0,
                symbol: "SharedReplacement".into(),
                signature: Some("void()".into()),
                auto_register: true,
            },
            Callback {
                name: "second".into(),
                index: 1,
                symbol: "SharedReplacement".into(),
                signature: Some("int(void*)".into()),
                auto_register: true,
            },
        ];
        assert!(validate(&m).is_err());
    }
    fn signatures_patch(locator: &str) -> Manifest {
        let mut m = parse("").unwrap();
        m.patches.push(
            serde_yaml::from_str(&format!(
                "name: SignedHook\n\
                 enabled: true\n\
                 description: owned signatures fixture\n\
                 replacement: IAOT_SignedHook\n\
                 signature: 'int(void*)'\n\
                 preserve_original: true\n\
                 locator: {locator}\n\
                 patch: {{ type: branch_to_cave }}\n\
                 expect: 'FF ?? ?? D1'\n"
            ))
            .unwrap(),
        );
        expand_replacement_shorthand(&mut m).unwrap();
        m
    }
    #[test]
    fn signatures_locator_parses_and_validates() {
        let m = signatures_patch(
            "{ type: signatures, signatures: [\
               { kind: xref, pattern: '?? ?? ?? 94 E0 03 13 AA' },\
               { kind: xref, pattern: '00 00 80 D2 ?? ?? ?? 17', branch_index: 1 },\
               { kind: direct, pattern: 'FF ?? ?? D1 ?? ?? ?? A9', offset: 8 }],\
             fallback: { rva: 0x265CE40, version: '56.3' } }",
        );
        validate(&m).unwrap();
        let Locator::Signatures {
            signatures,
            fallback,
        } = &m.patches[0].locator
        else {
            panic!("expected a signatures locator")
        };
        assert_eq!(signatures.len(), 3);
        assert_eq!(signatures[1].branch_index, Some(1));
        assert_eq!(signatures[2].offset, Some(8));
        assert_eq!(fallback.as_ref().unwrap().rva, 0x265CE40);
    }
    #[test]
    fn signatures_locator_rejects_bad_shapes() {
        for locator in [
            "{ type: signatures, signatures: [] }",
            // No B/BL at the named instruction.
            "{ type: signatures, signatures: [{ kind: xref, pattern: 'FF 43 00 D1' }] }",
            "{ type: signatures, signatures: [{ kind: xref, pattern: '?? ?? ?? 94', branch_index: 1 }] }",
            // Partial instruction, unaligned offset, direct with a branch index.
            "{ type: signatures, signatures: [{ kind: direct, pattern: 'FF 43 00' }] }",
            "{ type: signatures, signatures: [{ kind: direct, pattern: 'FF 43 00 D1', offset: 2 }] }",
            "{ type: signatures, signatures: [{ kind: direct, pattern: 'FF 43 00 D1', branch_index: 0 }] }",
            // A fallback must say which version its rva belongs to.
            "{ type: signatures, signatures: [{ kind: direct, pattern: 'FF 43 00 D1' }], fallback: { rva: 0x1000, version: '' } }",
        ] {
            assert!(validate(&signatures_patch(locator)).is_err(), "{locator}");
        }
    }
    #[test]
    fn replacement_shorthand_allocates_and_wires_runtime_slots() {
        let mut m = parse("").unwrap();
        m.patches.push(
            serde_yaml::from_str(
                "name: ExampleHook\n\
                 enabled: true\n\
                 description: owned shorthand fixture\n\
                 replacement: IAOT_ExampleHook\n\
                 signature: 'int(void*, int)'\n\
                 preserve_original: true\n\
                 locator: { type: rva, value: 4096 }\n\
                 patch: { type: branch_to_cave }\n",
            )
            .unwrap(),
        );

        expand_replacement_shorthand(&mut m).unwrap();
        validate(&m).unwrap();

        assert_eq!(m.config.callbacks.len(), 1);
        let callback = &m.config.callbacks[0];
        assert_eq!(callback.name, "ExampleHook");
        assert_eq!(callback.index, 0);
        assert_eq!(callback.symbol, "IAOT_ExampleHook");
        assert_eq!(callback.signature.as_deref(), Some("int(void*, int)"));
        assert!(callback.auto_register);
        assert_eq!(m.patches[0].original_slot, Some(0));
        assert!(matches!(
            m.patches[0].patch.action,
            Some(PatchAction::BranchToCallback {
                callback: Some(ref name)
            }) if name == "ExampleHook"
        ));
    }
    #[test]
    fn disabled_replacement_shorthand_does_not_consume_runtime_slots() {
        let mut m = parse("").unwrap();
        for index in 0..70 {
            m.patches.push(
                serde_yaml::from_str(&format!(
                    "name: DisabledHook{index}\n\
                     enabled: false\n\
                     description: disabled shorthand fixture\n\
                     replacement: IAOT_DisabledHook\n\
                     signature: 'void()'\n\
                     preserve_original: true\n\
                     locator: {{ type: rva, value: 4096 }}\n\
                     patch: {{ type: branch_to_cave, action: {{ type: branch_to_callback }} }}\n"
                ))
                .unwrap(),
            );
        }

        expand_replacement_shorthand(&mut m).unwrap();
        validate(&m).unwrap();
        assert!(m.config.callbacks.is_empty());
        assert!(m.patches.iter().all(|patch| patch.original_slot.is_none()));
    }

    #[test]
    fn missing_hook_symbol() {
        let yaml="project: { name: Demo, bundle_id: com.example.demo, platform: ios, arch: arm64, min_ios: '14.0' }\nruntime: { config_segment: __IAOTCFG, cave_segment: __IAOTCAVE, page_size: 16384, dylib_name: Demo.dylib }\napp: {}\ngenerated_hooks: [{ name: bad }]";
        assert!(serde_yaml::from_str::<Manifest>(yaml).is_err());
    }
    #[test]
    fn invalid_segment() {
        let mut m = parse("").unwrap();
        m.runtime.config_segment = "too_long_segment_name".into();
        assert!(validate(&m).is_err());
    }
    #[test]
    fn invalid_page_size() {
        let mut m = parse("").unwrap();
        m.runtime.page_size = 1;
        assert!(validate(&m).is_err());
        m.runtime.page_size = 131_072;
        assert!(validate(&m).is_err());
    }
    #[test]
    fn validates_objective_c_hook_definition() {
        let mut m = parse("").unwrap();
        m.functions = vec![serde_yaml::from_str(
            "{ name: ExampleTitle, kind: objc, class: ExampleController, selector: title, class_method: false, signature: 'id(id, SEL)', replacement: IAOT_ExampleTitle, preserve_original: true }",
        )
        .unwrap()];
        assert!(validate(&m).is_ok());
        m.functions[0].replacement = None;
        assert!(validate(&m).is_err());
    }
    #[test]
    fn invalid_member() {
        let mut m = parse("").unwrap();
        m.app.executable_member = Some("../escape".into());
        assert!(validate(&m).is_err());
    }

    #[test]
    fn split_configs_are_merged_and_il2cpp_is_optional() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("hooks.yaml"),
            "project_config: configs.yaml\nconfig: { flags: [], strings: [], callbacks: [] }\nfunctions: []\ngenerated_hooks: []\npatches: []\n",
        )
        .unwrap();
        fs::write(
            directory.path().join("configs.yaml"),
            "project: { name: Split, bundle_id: com.example.split, platform: ios, arch: arm64, min_ios: '14.0' }\npaths: { output_ipa: build/Split.ipa }\nbuild: { mode: release }\nanalysis: { application_type: auto, rendering_backend: auto }\nruntime: { config_segment: __IAOTCFG, cave_segment: __IAOTCAVE, page_size: 16384, dylib_name: Split.dylib }\napp: { prefer_unity_framework: false }\nil2cpp: { enabled: false, config_file: missing-file-is-not-read.yaml }\n",
        )
        .unwrap();

        let loaded = load(&directory.path().join("hooks.yaml")).unwrap();
        assert_eq!(loaded.project.name, "Split");
        assert_eq!(loaded.build.mode, "release");
        assert!(!loaded.analysis.static_il2cpp);
        assert!(!loaded.analysis.runtime_il2cpp);
        validate(&loaded).unwrap();
    }

    #[test]
    fn enabled_il2cpp_config_is_merged() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("hooks.yaml"),
            "project_config: configs.yaml\nconfig: { flags: [], strings: [], callbacks: [] }\nfunctions: []\ngenerated_hooks: []\npatches: []\n",
        )
        .unwrap();
        fs::write(
            directory.path().join("configs.yaml"),
            "project: { name: Split, bundle_id: com.example.split, platform: ios, arch: arm64, min_ios: '14.0' }\nanalysis: { application_type: auto, rendering_backend: auto }\nruntime: { config_segment: __IAOTCFG, cave_segment: __IAOTCAVE, page_size: 16384, dylib_name: Split.dylib }\napp: { prefer_unity_framework: true }\nil2cpp: { enabled: true, config_file: il2cpp_config.yaml }\n",
        )
        .unwrap();
        fs::write(
            directory.path().join("il2cpp_config.yaml"),
            "analysis: { scripting_backend: il2cpp, static_il2cpp: true, runtime_il2cpp: true, il2cpp_module: UnityFramework }\nruntime: { offline_classes: [{ module: Assembly-CSharp, namespace: Demo, name: Player }] }\n",
        )
        .unwrap();

        let loaded = load(&directory.path().join("hooks.yaml")).unwrap();
        assert_eq!(loaded.analysis.scripting_backend, "il2cpp");
        assert_eq!(
            loaded.analysis.il2cpp_module.as_deref(),
            Some("UnityFramework")
        );
        assert_eq!(loaded.runtime.offline_classes.len(), 1);
        validate(&loaded).unwrap();
    }
}
