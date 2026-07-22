use crate::errors::IaotError;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
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
}
#[derive(Clone, Debug, Deserialize)]
pub struct App {
    pub executable_member: Option<String>,
    pub authorized_only: bool,
    #[serde(default)]
    pub prefer_unity_framework: bool,
    #[serde(default)]
    pub target_module: Option<String>,
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
    pub symbol: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Function {
    pub name: String,
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
    },
    Xref {
        pattern: String,
        xref_pattern: Option<String>,
        match_index: Option<usize>,
        xref_match_index: Option<usize>,
        xref_bl_index: Option<usize>,
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

#[derive(Clone, Debug, Deserialize)]
pub struct PatchOperation {
    #[serde(rename = "type")]
    pub kind: String,
    pub action: PatchAction,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatchAction {
    RetIfFlag { flag: String },
    ZeroRegisterIfFlag { flag: String, register: u8 },
    CallbackIfFlag { flag: String, callback: String },
    CallbackReturnZeroIfFlag { flag: String, callback: String },
    CallbackAfter { callback: String },
    BranchToCallback { callback: String },
    BranchToReplacement { symbol: String },
}

fn default_true() -> bool {
    true
}

pub fn load(path: &Path) -> anyhow::Result<Manifest> {
    let text = fs::read_to_string(path)?;
    let mut manifest: Manifest = serde_yaml::from_str(&text)?;
    manifest.source_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    Ok(manifest)
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
pub fn validate(m: &Manifest, force: bool) -> Result<(), IaotError> {
    if !m.app.authorized_only && !force {
        return Err(IaotError::Manifest(
            "app.authorized_only must be true (or explicitly pass --force-authorized)".into(),
        ));
    }
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
        if patch.name.is_empty()
            || patch.description.is_empty()
            || patch.patch.kind != "branch_to_cave"
        {
            return Err(IaotError::Manifest(
                "patch name/description are required and patch.type must be branch_to_cave".into(),
            ));
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
            && !matches!(patch.patch.action, PatchAction::BranchToCallback { .. })
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
        match &patch.patch.action {
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
                if !flags.contains(flag.as_str()) || !callbacks.contains(callback.as_str()) =>
            {
                return Err(IaotError::Manifest(format!(
                    "patch {} references an unknown flag or callback",
                    patch.name
                )))
            }
            PatchAction::CallbackAfter { callback }
            | PatchAction::BranchToCallback { callback }
                if !callbacks.contains(callback.as_str()) =>
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
        if let Some(expect) = &patch.expect {
            if expect == "auto" {
                continue;
            }
            let parsed = crate::pattern::parse(expect).map_err(|_| {
                IaotError::Manifest(format!("patch {} has invalid expected bytes", patch.name))
            })?;
            if parsed.len() != 1
                || parsed[0].0.iter().any(|x| x.is_none())
                || parsed[0].0.len() != 4
            {
                return Err(IaotError::Manifest(format!(
                    "patch {} expect must be exactly four concrete bytes",
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
        serde_yaml::from_str(&format!("project: {{ name: Demo, bundle_id: com.example.demo, platform: ios, arch: arm64, min_ios: '14.0' }}\nruntime: {{ config_segment: __IAOTCFG, cave_segment: __IAOTCAVE, page_size: 16384, dylib_name: Demo.dylib }}\napp: {{ executable_member: Payload/Demo.app/Demo, authorized_only: true }}\nconfig: {{ flags: [], strings: [], callbacks: [] }}\nfunctions: []\ngenerated_hooks: []\npatches: []\n{extra}"))
    }
    #[test]
    fn valid_example() {
        assert!(validate(&parse("").unwrap(), false).is_ok());
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
        assert!(validate(&m, false).is_err());
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
        assert!(validate(&m, false).is_err());
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
        assert!(validate(&m, false).is_err());
    }
    #[test]
    fn missing_hook_symbol() {
        let yaml="project: { name: Demo, bundle_id: com.example.demo, platform: ios, arch: arm64, min_ios: '14.0' }\nruntime: { config_segment: __IAOTCFG, cave_segment: __IAOTCAVE, page_size: 16384, dylib_name: Demo.dylib }\napp: { authorized_only: true }\ngenerated_hooks: [{ name: bad }]";
        assert!(serde_yaml::from_str::<Manifest>(yaml).is_err());
    }
    #[test]
    fn invalid_segment() {
        let mut m = parse("").unwrap();
        m.runtime.config_segment = "too_long_segment_name".into();
        assert!(validate(&m, false).is_err());
    }
    #[test]
    fn invalid_page_size() {
        let mut m = parse("").unwrap();
        m.runtime.page_size = 1;
        assert!(validate(&m, false).is_err());
        m.runtime.page_size = 131_072;
        assert!(validate(&m, false).is_err());
    }
    #[test]
    fn validates_objective_c_hook_definition() {
        let mut m = parse("").unwrap();
        m.functions = vec![serde_yaml::from_str(
            "{ name: ExampleTitle, kind: objc, class: ExampleController, selector: title, class_method: false, signature: 'id(id, SEL)', replacement: IAOT_ExampleTitle, preserve_original: true }",
        )
        .unwrap()];
        assert!(validate(&m, false).is_ok());
        m.functions[0].replacement = None;
        assert!(validate(&m, false).is_err());
    }
    #[test]
    fn invalid_member() {
        let mut m = parse("").unwrap();
        m.app.executable_member = Some("../escape".into());
        assert!(validate(&m, false).is_err());
    }
}
