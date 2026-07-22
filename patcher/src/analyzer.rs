use object::{Object, ObjectSymbol};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::Path,
};
use zip::ZipArchive;

const MH_MAGIC_64: u32 = 0xfeed_facf;
const CPU_TYPE_ARM64: u32 = 0x0100_000c;
const CPU_SUBTYPE_ARM64E: u32 = 2;
const FAT_MAGIC: u32 = 0xcafe_babe;
const FAT_MAGIC_64: u32 = 0xcafe_babf;

#[derive(Clone, Debug, Serialize)]
pub struct ModuleAnalysis {
    pub member: String,
    pub name: String,
    pub role: String,
    pub size: u64,
    pub architectures: Vec<String>,
    pub file_kind: String,
    pub imports: Vec<String>,
    pub has_objc_metadata: bool,
    pub has_swift_metadata: bool,
    pub has_cpp_symbols: bool,
    pub has_il2cpp_exports: bool,
    pub encrypted: Option<bool>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UnityAnalysis {
    pub detected: bool,
    pub scripting_backend: String,
    pub version: Option<String>,
    pub metadata_version: Option<u32>,
    pub metadata_member: Option<String>,
    pub il2cpp_module: Option<String>,
    pub static_analyzer: String,
    pub runtime_resolver: String,
    pub evidence: Vec<String>,
    pub capabilities: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RenderingAnalysis {
    pub selected: String,
    pub candidates: Vec<String>,
    pub evidence: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IpaAnalysis {
    pub schema_version: u32,
    pub input: String,
    pub app_bundle: String,
    pub info_plist_member: String,
    pub bundle_id: Option<String>,
    pub display_name: Option<String>,
    pub short_version: Option<String>,
    pub bundle_version: Option<String>,
    pub executable_name: String,
    pub executable_member: String,
    pub application_type: String,
    pub classification_confidence: String,
    pub classification_evidence: Vec<String>,
    pub modules: Vec<ModuleAnalysis>,
    pub unity: UnityAnalysis,
    pub rendering: RenderingAnalysis,
    pub warnings: Vec<String>,
}

fn be_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn be_u64(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        data.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

fn le_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn arch_name(cpu: u32, subtype: u32) -> String {
    match (cpu, subtype & 0x00ff_ffff) {
        (CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64E) => "arm64e".into(),
        (CPU_TYPE_ARM64, _) => "arm64".into(),
        (0x0200_000c, _) => "arm64_32".into(),
        (0x0100_0007, _) => "x86_64".into(),
        (0x0000_000c, _) => "arm".into(),
        (0x0000_0007, _) => "i386".into(),
        _ => format!("cpu-0x{cpu:08x}-subtype-0x{subtype:08x}"),
    }
}

pub fn macho_architectures(data: &[u8]) -> Vec<String> {
    if le_u32(data, 0) == Some(MH_MAGIC_64) {
        return match (le_u32(data, 4), le_u32(data, 8)) {
            (Some(cpu), Some(subtype)) => vec![arch_name(cpu, subtype)],
            _ => vec![],
        };
    }
    let magic = be_u32(data, 0);
    if !matches!(magic, Some(FAT_MAGIC | FAT_MAGIC_64)) {
        return vec![];
    }
    let is64 = magic == Some(FAT_MAGIC_64);
    let count = be_u32(data, 4).unwrap_or(0).min(128) as usize;
    let stride = if is64 { 32 } else { 20 };
    let mut result = Vec::new();
    for index in 0..count {
        let offset = 8 + index * stride;
        if let (Some(cpu), Some(subtype)) = (be_u32(data, offset), be_u32(data, offset + 4)) {
            result.push(arch_name(cpu, subtype));
        }
    }
    result.sort();
    result.dedup();
    result
}

fn arm64_slice(data: &[u8]) -> Option<&[u8]> {
    if le_u32(data, 0) == Some(MH_MAGIC_64) && le_u32(data, 4) == Some(CPU_TYPE_ARM64) {
        return Some(data);
    }
    let magic = be_u32(data, 0)?;
    if !matches!(magic, FAT_MAGIC | FAT_MAGIC_64) {
        return None;
    }
    let is64 = magic == FAT_MAGIC_64;
    let count = be_u32(data, 4)?.min(128) as usize;
    let stride = if is64 { 32 } else { 20 };
    for index in 0..count {
        let entry = 8 + index * stride;
        if be_u32(data, entry)? != CPU_TYPE_ARM64 {
            continue;
        }
        let (offset, size) = if is64 {
            (
                be_u64(data, entry + 8)? as usize,
                be_u64(data, entry + 16)? as usize,
            )
        } else {
            (
                be_u32(data, entry + 8)? as usize,
                be_u32(data, entry + 12)? as usize,
            )
        };
        return data.get(offset..offset.checked_add(size)?);
    }
    None
}

fn contains_ascii(data: &[u8], value: &str) -> bool {
    !value.is_empty()
        && data
            .windows(value.len())
            .any(|window| window == value.as_bytes())
}

fn macho_imports(data: &[u8]) -> Vec<String> {
    let Some(data) = arm64_slice(data) else {
        return vec![];
    };
    if le_u32(data, 0) != Some(MH_MAGIC_64) {
        return vec![];
    }
    let ncmds = le_u32(data, 16).unwrap_or(0).min(65_536);
    let command_bytes = le_u32(data, 20).unwrap_or(0) as usize;
    let end = 32usize.saturating_add(command_bytes).min(data.len());
    let mut cursor = 32usize;
    let mut imports = Vec::new();
    for _ in 0..ncmds {
        let Some(cmd) = le_u32(data, cursor) else {
            break;
        };
        let Some(size) = le_u32(data, cursor + 4).map(|v| v as usize) else {
            break;
        };
        if size < 8 || cursor.saturating_add(size) > end {
            break;
        }
        let base_cmd = cmd & 0x7fff_ffff;
        if matches!(base_cmd, 0x0c | 0x18 | 0x1f | 0x23 | 0x20) && size >= 24 {
            if let Some(name_offset) = le_u32(data, cursor + 8).map(|v| v as usize) {
                if name_offset < size {
                    let bytes = &data[cursor + name_offset..cursor + size];
                    let terminator = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
                    if let Ok(name) = std::str::from_utf8(&bytes[..terminator]) {
                        imports.push(name.to_owned());
                    }
                }
            }
        }
        cursor += size;
    }
    imports.sort();
    imports.dedup();
    imports
}

fn encryption_state(data: &[u8]) -> Option<bool> {
    let data = arm64_slice(data)?;
    let ncmds = le_u32(data, 16)?.min(65_536);
    let end = 32usize
        .checked_add(le_u32(data, 20)? as usize)?
        .min(data.len());
    let mut cursor = 32usize;
    for _ in 0..ncmds {
        let cmd = le_u32(data, cursor)?;
        let size = le_u32(data, cursor + 4)? as usize;
        if size < 8 || cursor.checked_add(size)? > end {
            return None;
        }
        if cmd == 0x2c && size >= 24 {
            return Some(le_u32(data, cursor + 16)? != 0);
        }
        cursor += size;
    }
    Some(false)
}

fn module_name(member: &str) -> String {
    member.rsplit('/').next().unwrap_or(member).to_owned()
}

fn module_analysis(member: &str, role: &str, data: &[u8]) -> ModuleAnalysis {
    let architectures = macho_architectures(data);
    let imports = macho_imports(data);
    let mut warnings = Vec::new();
    if architectures.is_empty() {
        warnings.push("member does not parse as a supported Mach-O".into());
    }
    if architectures.iter().any(|arch| arch == "arm64e") {
        warnings.push("arm64e was detected; IAOT does not claim pointer-authenticated callback-pointer support".into());
    }
    if encryption_state(data) == Some(true) {
        warnings
            .push("LC_ENCRYPTION_INFO_64 reports encrypted code; provide a decrypted IPA".into());
    }
    let parsed = object::File::parse(data).ok();
    let has_cpp_symbols = contains_ascii(data, "__Z")
        || parsed.as_ref().is_some_and(|file| {
            file.symbols()
                .filter_map(|symbol| symbol.name().ok())
                .any(|name| name.starts_with("__Z") || name.starts_with("_Z"))
        });
    ModuleAnalysis {
        member: member.into(),
        name: module_name(member),
        role: role.into(),
        size: data.len() as u64,
        architectures,
        file_kind: parsed
            .as_ref()
            .map(|file| format!("{:?}/{:?}", file.format(), file.kind()))
            .unwrap_or_else(|| "unknown".into()),
        has_objc_metadata: contains_ascii(data, "__objc_classlist")
            || contains_ascii(data, "_objc_msgSend"),
        has_swift_metadata: imports.iter().any(|name| name.contains("libswift"))
            || contains_ascii(data, "__swift5_types")
            || contains_ascii(data, "_$s"),
        has_cpp_symbols,
        has_il2cpp_exports: contains_ascii(data, "il2cpp_domain_get")
            || contains_ascii(data, "il2cpp_codegen_register")
            || contains_ascii(data, "g_CodeRegistration"),
        encrypted: encryption_state(data),
        imports,
        warnings,
    }
}

fn is_primary_info_plist(name: &str) -> bool {
    if !name.starts_with("Payload/") || !name.ends_with(".app/Info.plist") {
        return false;
    }
    let tail = &name["Payload/".len()..];
    tail.matches(".app/").count() == 1 && !tail.contains("/PlugIns/")
}

fn read_member(zip: &mut ZipArchive<File>, name: &str) -> anyhow::Result<Vec<u8>> {
    let mut file = zip
        .by_name(name)
        .map_err(|_| anyhow::anyhow!("IPA member not found: {name}"))?;
    let mut data = Vec::with_capacity(file.size().min(64 * 1024 * 1024) as usize);
    file.read_to_end(&mut data)?;
    Ok(data)
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn xml_plist_strings(data: &[u8]) -> anyhow::Result<BTreeMap<String, String>> {
    let text = std::str::from_utf8(data)?;
    let mut values = BTreeMap::new();
    let mut cursor = 0usize;
    while let Some(key_start) = text[cursor..]
        .find("<key>")
        .map(|offset| cursor + offset + 5)
    {
        let Some(key_end) = text[key_start..]
            .find("</key>")
            .map(|offset| key_start + offset)
        else {
            break;
        };
        let after_key = key_end + 6;
        let value = &text[after_key..];
        let whitespace = value.len() - value.trim_start().len();
        let tag_start = after_key + whitespace;
        if !text[tag_start..].starts_with("<string>") {
            cursor = after_key;
            continue;
        }
        let string_start = tag_start + 8;
        let Some(string_end) = text[string_start..]
            .find("</string>")
            .map(|offset| string_start + offset)
        else {
            break;
        };
        values.insert(
            xml_unescape(&text[key_start..key_end]),
            xml_unescape(&text[string_start..string_end]),
        );
        cursor = string_end + 9;
    }
    Ok(values)
}

fn read_be_integer(data: &[u8]) -> Option<u64> {
    if data.is_empty() || data.len() > 8 {
        return None;
    }
    Some(
        data.iter()
            .fold(0u64, |value, byte| (value << 8) | *byte as u64),
    )
}

struct BinaryPlist<'a> {
    data: &'a [u8],
    offsets: Vec<usize>,
    ref_size: usize,
    top: usize,
}
impl<'a> BinaryPlist<'a> {
    fn new(data: &'a [u8]) -> anyhow::Result<Self> {
        if data.len() < 40 || &data[..8] != b"bplist00" {
            anyhow::bail!("invalid binary plist header")
        }
        let trailer = &data[data.len() - 32..];
        let offset_size = trailer[6] as usize;
        let ref_size = trailer[7] as usize;
        let count = read_be_integer(&trailer[8..16]).unwrap_or(0) as usize;
        let top = read_be_integer(&trailer[16..24]).unwrap_or(u64::MAX) as usize;
        let table = read_be_integer(&trailer[24..32]).unwrap_or(u64::MAX) as usize;
        if offset_size == 0
            || offset_size > 8
            || ref_size == 0
            || ref_size > 8
            || count > 1_000_000
            || top >= count
        {
            anyhow::bail!("unsupported binary plist trailer")
        }
        let table_bytes = count
            .checked_mul(offset_size)
            .ok_or_else(|| anyhow::anyhow!("binary plist offset overflow"))?;
        if table
            .checked_add(table_bytes)
            .is_none_or(|end| end > data.len() - 32)
        {
            anyhow::bail!("binary plist offset table is truncated")
        }
        let mut offsets = Vec::with_capacity(count);
        for index in 0..count {
            offsets.push(
                read_be_integer(
                    &data[table + index * offset_size..table + (index + 1) * offset_size],
                )
                .unwrap_or(u64::MAX) as usize,
            );
        }
        if offsets.iter().any(|offset| *offset >= table) {
            anyhow::bail!("binary plist object offset is invalid")
        }
        Ok(Self {
            data,
            offsets,
            ref_size,
            top,
        })
    }
    fn length(&self, offset: usize, nibble: usize) -> anyhow::Result<(usize, usize)> {
        if nibble < 15 {
            return Ok((nibble, offset + 1));
        }
        let marker = *self
            .data
            .get(offset + 1)
            .ok_or_else(|| anyhow::anyhow!("binary plist length is truncated"))?;
        if marker >> 4 != 1 {
            anyhow::bail!("binary plist extended length is not an integer")
        }
        let width = 1usize
            .checked_shl((marker & 15) as u32)
            .ok_or_else(|| anyhow::anyhow!("binary plist integer width overflow"))?;
        let start = offset + 2;
        let end = start
            .checked_add(width)
            .ok_or_else(|| anyhow::anyhow!("binary plist length overflow"))?;
        let value = read_be_integer(
            self.data
                .get(start..end)
                .ok_or_else(|| anyhow::anyhow!("binary plist length is truncated"))?,
        )
        .ok_or_else(|| anyhow::anyhow!("binary plist length is unsupported"))?
            as usize;
        Ok((value, end))
    }
    fn reference(&self, data: &[u8]) -> anyhow::Result<usize> {
        let value = read_be_integer(data)
            .ok_or_else(|| anyhow::anyhow!("invalid binary plist reference"))?
            as usize;
        if value >= self.offsets.len() {
            anyhow::bail!("binary plist reference is out of bounds")
        }
        Ok(value)
    }
    fn string(&self, object: usize) -> anyhow::Result<String> {
        let offset = *self
            .offsets
            .get(object)
            .ok_or_else(|| anyhow::anyhow!("binary plist object is missing"))?;
        let marker = self.data[offset];
        let kind = marker >> 4;
        let (length, start) = self.length(offset, (marker & 15) as usize)?;
        match kind {
            5 => Ok(std::str::from_utf8(
                self.data
                    .get(
                        start
                            ..start
                                .checked_add(length)
                                .ok_or_else(|| anyhow::anyhow!("plist string overflow"))?,
                    )
                    .ok_or_else(|| anyhow::anyhow!("plist string is truncated"))?,
            )?
            .to_owned()),
            6 => {
                let bytes = self
                    .data
                    .get(
                        start
                            ..start
                                .checked_add(
                                    length
                                        .checked_mul(2)
                                        .ok_or_else(|| anyhow::anyhow!("plist UTF-16 overflow"))?,
                                )
                                .ok_or_else(|| anyhow::anyhow!("plist UTF-16 overflow"))?,
                    )
                    .ok_or_else(|| anyhow::anyhow!("plist UTF-16 string is truncated"))?;
                let units = bytes
                    .chunks_exact(2)
                    .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                    .collect::<Vec<_>>();
                Ok(String::from_utf16(&units)?)
            }
            _ => anyhow::bail!("binary plist value is not a string"),
        }
    }
    fn root_strings(&self) -> anyhow::Result<BTreeMap<String, String>> {
        let offset = self.offsets[self.top];
        let marker = self.data[offset];
        if marker >> 4 != 13 {
            anyhow::bail!("binary plist root is not a dictionary")
        }
        let (count, start) = self.length(offset, (marker & 15) as usize)?;
        let refs_size = count
            .checked_mul(self.ref_size)
            .ok_or_else(|| anyhow::anyhow!("binary plist dictionary overflow"))?;
        let keys = self
            .data
            .get(start..start + refs_size)
            .ok_or_else(|| anyhow::anyhow!("binary plist dictionary keys are truncated"))?;
        let values = self
            .data
            .get(start + refs_size..start + refs_size * 2)
            .ok_or_else(|| anyhow::anyhow!("binary plist dictionary values are truncated"))?;
        let mut result = BTreeMap::new();
        for index in 0..count {
            let key_ref =
                self.reference(&keys[index * self.ref_size..(index + 1) * self.ref_size])?;
            let value_ref =
                self.reference(&values[index * self.ref_size..(index + 1) * self.ref_size])?;
            if let (Ok(key), Ok(value)) = (self.string(key_ref), self.string(value_ref)) {
                result.insert(key, value);
            }
        }
        Ok(result)
    }
}

fn parse_info_plist(data: &[u8]) -> anyhow::Result<BTreeMap<String, String>> {
    if data.starts_with(b"bplist00") {
        BinaryPlist::new(data)?.root_strings()
    } else {
        xml_plist_strings(data)
    }
}

fn unity_version_from_bytes(data: &[u8]) -> Option<String> {
    for start in 0..data.len().saturating_sub(8) {
        if data[start] != b'2' || data.get(start + 1) != Some(&b'0') {
            continue;
        }
        let mut end = start;
        while end < data.len()
            && end - start < 24
            && (data[end].is_ascii_alphanumeric() || matches!(data[end], b'.' | b'-'))
        {
            end += 1;
        }
        let candidate = std::str::from_utf8(&data[start..end]).ok()?;
        let dots = candidate.bytes().filter(|byte| *byte == b'.').count();
        if dots >= 2
            && candidate.len() >= 8
            && candidate
                .bytes()
                .any(|byte| matches!(byte, b'f' | b'p' | b'a' | b'b'))
        {
            return Some(candidate.to_owned());
        }
    }
    None
}

fn classify(modules: &[ModuleAnalysis], unity: &UnityAnalysis) -> (String, String, Vec<String>) {
    if unity.detected {
        return (
            match unity.scripting_backend.as_str() {
                "il2cpp" => "unity_il2cpp",
                "mono" => "unity_mono",
                _ => "unity_unknown_backend",
            }
            .into(),
            if unity.scripting_backend == "unknown" {
                "medium"
            } else {
                "high"
            }
            .into(),
            unity.evidence.clone(),
        );
    }
    let objc = modules.iter().any(|module| module.has_objc_metadata);
    let swift = modules.iter().any(|module| module.has_swift_metadata);
    let cpp = modules.iter().any(|module| module.has_cpp_symbols);
    let (kind, evidence) = match (objc, swift, cpp) {
        (true, true, _) => (
            "mixed_native",
            vec!["Objective-C and Swift metadata/imports".into()],
        ),
        (true, false, true) => (
            "native_objective_cpp",
            vec!["Objective-C metadata and C++ symbols".into()],
        ),
        (true, false, false) => ("native_objective_c", vec!["Objective-C metadata".into()]),
        (false, true, true) => (
            "mixed_native",
            vec!["Swift metadata and C++ symbols".into()],
        ),
        (false, true, false) => ("swift", vec!["Swift runtime/metadata".into()]),
        (false, false, true) => ("native_cpp", vec!["C++ symbols".into()]),
        _ => (
            "unknown_or_native_c",
            vec!["no high-confidence managed or Objective-C evidence".into()],
        ),
    };
    (
        kind.into(),
        if kind.starts_with("unknown") {
            "low"
        } else {
            "medium"
        }
        .into(),
        evidence,
    )
}

fn rendering(modules: &[ModuleAnalysis], unity: bool) -> RenderingAnalysis {
    let mut candidates = BTreeSet::new();
    let mut evidence = Vec::new();
    for module in modules {
        for import in &module.imports {
            let lower = import.to_ascii_lowercase();
            if lower.contains("metal") {
                candidates.insert("metal".to_owned());
                evidence.push(format!("{} imports {import}", module.name));
            }
            if lower.contains("opengles") {
                candidates.insert("opengl_es".to_owned());
                evidence.push(format!("{} imports {import}", module.name));
            }
            if lower.contains("scenekit") {
                candidates.insert("scenekit".to_owned());
                evidence.push(format!("{} imports {import}", module.name));
            }
            if lower.contains("spritekit") {
                candidates.insert("spritekit".to_owned());
                evidence.push(format!("{} imports {import}", module.name));
            }
            if lower.contains("uikit") {
                candidates.insert("uikit".to_owned());
            }
        }
    }
    if unity {
        candidates.insert("unity".into());
        evidence.push("Unity bundle evidence implies a Unity-managed render loop".into());
    }
    let selected = if candidates.contains("metal") || unity {
        "metal"
    } else {
        "metal_overlay_independent_of_host"
    };
    let mut warnings = Vec::new();
    if candidates.len() > 1 {
        warnings.push("multiple host rendering systems are plausible; the IAOT overlay uses an independent Metal surface".into());
    }
    RenderingAnalysis {
        selected: selected.into(),
        candidates: candidates.into_iter().collect(),
        evidence,
        warnings,
    }
}

pub fn analyze_ipa(path: &Path) -> anyhow::Result<IpaAnalysis> {
    if !path.is_file() {
        anyhow::bail!("input IPA does not exist: {}", path.display());
    }
    let mut zip = ZipArchive::new(File::open(path)?)?;
    let mut plist_members = Vec::new();
    let mut names = Vec::with_capacity(zip.len());
    for index in 0..zip.len() {
        let file = zip.by_index(index)?;
        let name = file.name().replace('\\', "/");
        if is_primary_info_plist(&name) {
            plist_members.push(name.clone());
        }
        names.push((name, file.size()));
    }
    plist_members.sort();
    plist_members.dedup();
    if plist_members.len() != 1 {
        anyhow::bail!(
            "expected exactly one primary Payload/*.app/Info.plist, found {}",
            plist_members.len()
        );
    }
    let info_plist_member = plist_members.remove(0);
    let plist_data = read_member(&mut zip, &info_plist_member)?;
    let plist = parse_info_plist(&plist_data)?;
    let executable_name = plist
        .get("CFBundleExecutable")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Info.plist is missing CFBundleExecutable"))?;
    if executable_name.contains(['/', '\\']) || executable_name == "." || executable_name == ".." {
        anyhow::bail!("Info.plist contains an unsafe CFBundleExecutable")
    }
    let app_bundle = info_plist_member
        .strip_suffix("/Info.plist")
        .unwrap()
        .to_owned();
    let executable_member = format!("{app_bundle}/{executable_name}");
    if !names.iter().any(|(name, _)| name == &executable_member) {
        anyhow::bail!("application executable is missing: {executable_member}")
    }

    let mut candidates = Vec::new();
    for (name, size) in &names {
        let role = if name == &executable_member {
            Some("main_executable")
        } else if name.starts_with(&format!("{app_bundle}/Frameworks/"))
            && (name.ends_with(".dylib")
                || name
                    .rsplit('/')
                    .next()
                    .is_some_and(|base| name.ends_with(&format!("{base}.framework/{base}")))
                || name.contains(".framework/"))
        {
            let parts: Vec<_> = name.split('/').collect();
            let framework_index = parts.iter().position(|part| part.ends_with(".framework"));
            if framework_index.is_some_and(|index| {
                parts.get(index + 1).is_some_and(|binary| {
                    *binary == parts[index].trim_end_matches(".framework")
                        && index + 2 == parts.len()
                })
            }) || name.ends_with(".dylib")
            {
                Some("embedded_module")
            } else {
                None
            }
        } else {
            None
        };
        if let Some(role) = role {
            candidates.push((name.clone(), role.to_owned(), *size));
        }
    }
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    candidates.dedup_by(|left, right| left.0 == right.0);
    let mut modules = Vec::new();
    for (member, role, _) in candidates {
        let data = read_member(&mut zip, &member)?;
        if !macho_architectures(&data).is_empty() {
            modules.push(module_analysis(&member, &role, &data));
        }
    }

    let unity_framework = modules
        .iter()
        .find(|module| module.name == "UnityFramework")
        .map(|module| module.member.clone());
    let metadata_member = names
        .iter()
        .find(|(name, _)| name.ends_with("/global-metadata.dat"))
        .map(|(name, _)| name.clone());
    let mono_evidence = names.iter().any(|(name, _)| {
        let lower = name.to_ascii_lowercase();
        lower.contains("monobleedingedge")
            || (lower.contains("/managed/") && lower.ends_with(".dll"))
    }) || modules.iter().any(|module| {
        module
            .imports
            .iter()
            .any(|name| name.to_ascii_lowercase().contains("mono"))
    });
    let il2cpp_module = modules
        .iter()
        .find(|module| module.has_il2cpp_exports)
        .map(|module| module.member.clone())
        .or_else(|| {
            metadata_member.as_ref().map(|_| {
                unity_framework
                    .clone()
                    .unwrap_or_else(|| executable_member.clone())
            })
        });
    let unity_detected = unity_framework.is_some()
        || metadata_member.is_some()
        || mono_evidence
        || names.iter().any(|(name, _)| {
            let lower = name.to_ascii_lowercase();
            lower.contains("/data/resources.assets") || lower.contains("/unity data/")
        });
    let mut unity_evidence = Vec::new();
    if unity_framework.is_some() {
        unity_evidence.push("UnityFramework.framework is embedded".into());
    }
    if let Some(member) = &metadata_member {
        unity_evidence.push(format!("IL2CPP metadata found at {member}"));
    }
    if mono_evidence {
        unity_evidence.push("Mono/Managed bundle files or imports were found".into());
    }
    let scripting_backend =
        if metadata_member.is_some() || modules.iter().any(|module| module.has_il2cpp_exports) {
            "il2cpp"
        } else if mono_evidence {
            "mono"
        } else {
            "unknown"
        };
    let metadata_version = if let Some(member) = &metadata_member {
        let data = read_member(&mut zip, member)?;
        if le_u32(&data, 0) == Some(0xfab1_1baf) {
            le_u32(&data, 4)
        } else {
            None
        }
    } else {
        None
    };
    let mut unity_version = None;
    for (name, _) in &names {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with("globalgamemanagers")
            || lower.ends_with("data.unity3d")
            || lower.ends_with("unityframework")
        {
            if let Ok(data) = read_member(&mut zip, name) {
                unity_version = unity_version_from_bytes(&data);
                if unity_version.is_some() {
                    break;
                }
            }
        }
    }
    let mut unity_warnings = Vec::new();
    if unity_detected && scripting_backend == "unknown" {
        unity_warnings.push("Unity was detected but the scripting backend is uncertain; configure an explicit override if needed".into());
    }
    if metadata_member.is_some() && metadata_version.is_none() {
        unity_warnings.push("global-metadata.dat does not have the standard IL2CPP header; it may be unsupported or transformed".into());
    }
    let unity = UnityAnalysis {
        detected: unity_detected,
        scripting_backend: scripting_backend.into(),
        version: unity_version,
        metadata_version,
        metadata_member,
        il2cpp_module,
        static_analyzer: if scripting_backend == "il2cpp" {
            "iaot_method_manifest_adapter".into()
        } else {
            "not_applicable".into()
        },
        runtime_resolver: if scripting_backend == "il2cpp" {
            "iaot_exported_api_resolver".into()
        } else {
            "not_applicable".into()
        },
        evidence: unity_evidence,
        capabilities: if scripting_backend == "il2cpp" {
            vec![
                "offline_method_manifest_adapter".into(),
                "runtime_exported_api_resolver".into(),
                "managed_strings_fields_and_invocation_when_exports_exist".into(),
            ]
        } else {
            vec![]
        },
        warnings: unity_warnings,
    };
    let (application_type, confidence, classification_evidence) = classify(&modules, &unity);
    let mut warnings = modules
        .iter()
        .flat_map(|module| {
            module
                .warnings
                .iter()
                .map(move |warning| format!("{}: {warning}", module.name))
        })
        .collect::<Vec<_>>();
    if modules
        .iter()
        .all(|module| !module.architectures.iter().any(|arch| arch == "arm64"))
    {
        warnings.push("no plain arm64 slice was found; static patching is unavailable".into());
    }
    let rendering = rendering(&modules, unity.detected);
    Ok(IpaAnalysis {
        schema_version: 1,
        input: path.display().to_string(),
        app_bundle,
        info_plist_member,
        bundle_id: plist.get("CFBundleIdentifier").cloned(),
        display_name: plist
            .get("CFBundleDisplayName")
            .cloned()
            .or_else(|| plist.get("CFBundleName").cloned()),
        short_version: plist.get("CFBundleShortVersionString").cloned(),
        bundle_version: plist.get("CFBundleVersion").cloned(),
        executable_name,
        executable_member,
        application_type,
        classification_confidence: confidence,
        classification_evidence,
        modules,
        unity,
        rendering,
        warnings,
    })
}

pub fn human_report(report: &IpaAnalysis) -> String {
    let mut output = String::new();
    output.push_str(&format!("Input IPA: {}\n", report.input));
    output.push_str(&format!("App bundle: {}\n", report.app_bundle));
    output.push_str(&format!("Executable: {}\n", report.executable_member));
    output.push_str(&format!(
        "Application version: {} (build {})\n",
        report.short_version.as_deref().unwrap_or("unknown"),
        report.bundle_version.as_deref().unwrap_or("unknown")
    ));
    output.push_str(&format!(
        "Application type: {} (confidence: {})\n",
        report.application_type, report.classification_confidence
    ));
    output.push_str(&format!(
        "Unity: {} / backend={} / version={} / metadata={}\n",
        report.unity.detected,
        report.unity.scripting_backend,
        report.unity.version.as_deref().unwrap_or("unknown"),
        report
            .unity
            .metadata_version
            .map(|version| version.to_string())
            .unwrap_or_else(|| "unknown".into())
    ));
    output.push_str(&format!(
        "IL2CPP module: {}\n",
        report
            .unity
            .il2cpp_module
            .as_deref()
            .unwrap_or("not detected")
    ));
    output.push_str(&format!(
        "Static analyzer: {}\nRuntime resolver: {}\n",
        report.unity.static_analyzer, report.unity.runtime_resolver
    ));
    output.push_str(&format!(
        "Rendering selection: {}\n",
        report.rendering.selected
    ));
    output.push_str("Modules:\n");
    for module in &report.modules {
        output.push_str(&format!(
            "  - {} [{}] arch={} imports={}\n",
            module.member,
            module.role,
            if module.architectures.is_empty() {
                "unknown".into()
            } else {
                module.architectures.join(",")
            },
            module.imports.len()
        ));
    }
    if !report.warnings.is_empty() || !report.unity.warnings.is_empty() {
        output.push_str("Warnings:\n");
        for warning in report.warnings.iter().chain(&report.unity.warnings) {
            output.push_str(&format!("  - {warning}\n"));
        }
    }
    output
}

pub fn write_reports(report: &IpaAnalysis, directory: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    std::fs::write(
        directory.join("ipa-analysis.yaml"),
        serde_yaml::to_string(report)?,
    )?;
    std::fs::write(directory.join("ipa-analysis.txt"), human_report(report))?;
    if report.unity.scripting_backend == "il2cpp" {
        let skeleton = "# Generated IL2CPP static-analysis adapter manifest.\n# Populate with an audited external analyzer export; do not guess RVAs.\nschema_version: 1\nmethods: []\n";
        let path = directory.join("il2cpp-methods.yaml");
        if !path.exists() {
            std::fs::write(path, skeleton)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::{write::SimpleFileOptions, ZipWriter};

    #[test]
    fn metadata_version_is_little_endian() {
        let bytes = [0xaf, 0x1b, 0xb1, 0xfa, 29, 0, 0, 0];
        assert_eq!(le_u32(&bytes, 0), Some(0xfab1_1baf));
        assert_eq!(le_u32(&bytes, 4), Some(29));
    }

    #[test]
    fn xml_plist_does_not_skip_keys_after_non_string_values() {
        let values = xml_plist_strings(
            br#"<plist><dict><key>UIDeviceFamily</key><array><integer>1</integer></array><key>CFBundleExecutable</key><string>OwnedApp</string></dict></plist>"#,
        )
        .unwrap();
        assert_eq!(
            values.get("CFBundleExecutable").map(String::as_str),
            Some("OwnedApp")
        );
        assert!(!values.contains_key("UIDeviceFamily"));
    }

    #[test]
    fn detects_thin_arm64e() {
        let mut data = [0u8; 32];
        data[0..4].copy_from_slice(&MH_MAGIC_64.to_le_bytes());
        data[4..8].copy_from_slice(&CPU_TYPE_ARM64.to_le_bytes());
        data[8..12].copy_from_slice(&CPU_SUBTYPE_ARM64E.to_le_bytes());
        assert_eq!(macho_architectures(&data), vec!["arm64e"]);
    }

    #[test]
    fn analyzes_xml_plist_and_il2cpp_metadata() {
        let directory = tempdir().unwrap();
        let ipa = directory.path().join("demo.ipa");
        let mut macho = vec![0u8; 32];
        macho[0..4].copy_from_slice(&MH_MAGIC_64.to_le_bytes());
        macho[4..8].copy_from_slice(&CPU_TYPE_ARM64.to_le_bytes());
        macho[12..16].copy_from_slice(&2u32.to_le_bytes());
        let mut zip = ZipWriter::new(File::create(&ipa).unwrap());
        zip.start_file("Payload/Demo.app/Info.plist", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(br#"<?xml version="1.0"?><plist><dict><key>CFBundleExecutable</key><string>Demo</string><key>CFBundleIdentifier</key><string>com.example.demo</string><key>CFBundleShortVersionString</key><string>1.2.3</string><key>CFBundleVersion</key><string>45</string></dict></plist>"#).unwrap();
        zip.start_file("Payload/Demo.app/Demo", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&macho).unwrap();
        zip.start_file(
            "Payload/Demo.app/Data/Managed/Metadata/global-metadata.dat",
            SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(&[0xaf, 0x1b, 0xb1, 0xfa, 29, 0, 0, 0])
            .unwrap();
        zip.finish().unwrap();
        let report = analyze_ipa(&ipa).unwrap();
        assert_eq!(report.executable_member, "Payload/Demo.app/Demo");
        assert_eq!(report.bundle_id.as_deref(), Some("com.example.demo"));
        assert_eq!(report.application_type, "unity_il2cpp");
        assert_eq!(report.unity.metadata_version, Some(29));
        assert_eq!(report.short_version.as_deref(), Some("1.2.3"));
        assert_eq!(report.bundle_version.as_deref(), Some("45"));
    }
}
