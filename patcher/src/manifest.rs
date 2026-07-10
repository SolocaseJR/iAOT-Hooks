use crate::errors::IaotError;
use serde::Deserialize;
use std::{collections::HashSet, fs, path::Path};

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub project: Project,
    pub runtime: Runtime,
    pub app: App,
    #[serde(default)]
    pub config: Config,
    #[serde(default)]
    pub functions: Vec<Function>,
    #[serde(default)]
    pub generated_hooks: Vec<GeneratedHook>,
    #[serde(default)]
    pub patches: Vec<Patch>,
}
#[derive(Debug, Deserialize)]
pub struct Project {
    pub name: String,
    pub bundle_id: String,
    pub platform: String,
    pub arch: String,
    pub min_ios: String,
}
#[derive(Debug, Deserialize)]
pub struct Runtime {
    pub config_segment: String,
    pub cave_segment: String,
    pub page_size: u64,
    pub dylib_name: String,
}
#[derive(Debug, Deserialize)]
pub struct App {
    pub executable_member: Option<String>,
    pub authorized_only: bool,
}
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub flags: Vec<Flag>,
    #[serde(default)]
    pub strings: Vec<StringValue>,
    #[serde(default)]
    pub callbacks: Vec<Callback>,
}
#[derive(Debug, Deserialize)]
pub struct Flag {
    pub name: String,
    pub index: u32,
    pub default: bool,
}
#[derive(Debug, Deserialize)]
pub struct StringValue {
    pub name: String,
    pub index: u32,
    pub default: String,
}
#[derive(Debug, Deserialize)]
pub struct Callback {
    pub name: String,
    pub index: u32,
    pub symbol: String,
}
#[derive(Debug, Deserialize)]
pub struct GeneratedHook {
    pub name: String,
    pub symbol: String,
}
#[derive(Debug, Deserialize)]
pub struct Function {
    pub name: String,
    pub signature: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    pub address: Option<FunctionAddress>,
    #[serde(rename = "class")]
    pub class_name: Option<String>,
    pub selector: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct FunctionAddress {
    #[serde(rename = "type")]
    pub kind: String,
    pub value: Option<u64>,
    pub symbol: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Patch {
    pub name: String,
    pub enabled: bool,
    pub description: String,
    pub locator: Locator,
    pub patch: PatchOperation,
    #[serde(default)]
    pub expect: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Locator {
    Pattern {
        pattern: String,
        match_index: Option<usize>,
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
    Va {
        value: u64,
    },
    Vmaddr {
        value: u64,
    },
    Rva {
        value: u64,
    },
}

#[derive(Debug, Deserialize)]
pub struct PatchOperation {
    #[serde(rename = "type")]
    pub kind: String,
    pub action: PatchAction,
}

#[derive(Debug, Deserialize)]
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

pub fn load(path: &Path) -> anyhow::Result<Manifest> {
    let text = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&text)?)
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
    if m.runtime.dylib_name.contains(['/', '\\']) {
        return Err(IaotError::Manifest(
            "runtime.dylib_name must be a filename, not a path".into(),
        ));
    }
    if m.project.platform != "ios" || m.project.arch != "arm64" {
        return Err(IaotError::Manifest(
            "only platform ios / arch arm64 is supported".into(),
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
    if m.runtime.page_size == 0 || !m.runtime.page_size.is_power_of_two() {
        return Err(IaotError::Manifest(
            "runtime.page_size must be a power of two".into(),
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
        } else {
            let sig = f.signature.as_deref().ok_or_else(|| {
                IaotError::Manifest(format!("function {} needs signature", f.name))
            })?;
            if !sig.contains('(')
                || !sig.ends_with(')')
                || sig
                    .chars()
                    .any(|c| matches!(c, ';' | '{' | '}' | '#' | '\n' | '\r' | '"'))
            {
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
            Locator::Symbol { symbol } if symbol.is_empty() => {
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
            },
            Callback {
                name: "b".into(),
                index: 0,
                symbol: "B".into(),
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
    fn invalid_member() {
        let mut m = parse("").unwrap();
        m.app.executable_member = Some("../escape".into());
        assert!(validate(&m, false).is_err());
    }
}
