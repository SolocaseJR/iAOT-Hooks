use crate::{
    arm64,
    caves::{self, CaveRequest},
    il2cpp_name::{self, ManagedIndex, ManagedQuery},
    manifest::{
        ConflictPolicy, FailureBehavior, Instantiation, Locator, Manifest, Patch, PatchAction,
        Signature, SignatureKind,
    },
    pattern,
};
use object::{Object, ObjectSegment, ObjectSymbol};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::Path,
};

const MH_MAGIC_64: u32 = 0xfeedfacf;
const CPU_TYPE_ARM64: u32 = 0x0100000c;
const MH_EXECUTE: u32 = 2;
const MH_DYLIB: u32 = 6;
const LC_SEGMENT_64: u32 = 0x19;
const LC_CODE_SIGNATURE: u32 = 0x1d;
const LC_LOAD_DYLIB: u32 = 0xc;
const LC_SYMTAB: u32 = 0x2;
const LC_DYSYMTAB: u32 = 0xb;
const LC_TWOLEVEL_HINTS: u32 = 0x16;
const LC_SEGMENT_SPLIT_INFO: u32 = 0x1e;
const LC_DYLD_INFO: u32 = 0x22;
const LC_DYLD_INFO_ONLY: u32 = 0x8000_0022;
const LC_FUNCTION_STARTS: u32 = 0x26;
const LC_DATA_IN_CODE: u32 = 0x29;
const LC_DYLIB_CODE_SIGN_DRS: u32 = 0x2b;
const LC_LINKER_OPTIMIZATION_HINT: u32 = 0x2e;
const LC_NOTE: u32 = 0x31;
const LC_DYLD_EXPORTS_TRIE: u32 = 0x8000_0033;
const LC_DYLD_CHAINED_FIXUPS: u32 = 0x8000_0034;
const LC_ATOM_INFO: u32 = 0x36;
const HEADER_SIZE: usize = 32;
const SEGMENT_SIZE: usize = 72;
const CONFIG_SIZE: usize = 10312;
const FLAGS_OFFSET: usize = 72;
const CALLBACKS_OFFSET: usize = 1096;
const STRINGS_OFFSET: usize = 1608;
const ORIGINALS_OFFSET: usize = 9800;
const ORIGINALS_COUNT: usize = 64;

fn u32at(d: &[u8], o: usize) -> anyhow::Result<u32> {
    Ok(u32::from_le_bytes(
        d.get(o..o + 4)
            .ok_or_else(|| anyhow::anyhow!("truncated Mach-O"))?
            .try_into()
            .unwrap(),
    ))
}
fn u64at(d: &[u8], o: usize) -> anyhow::Result<u64> {
    Ok(u64::from_le_bytes(
        d.get(o..o + 8)
            .ok_or_else(|| anyhow::anyhow!("truncated Mach-O"))?
            .try_into()
            .unwrap(),
    ))
}
fn put32(d: &mut [u8], o: usize, v: u32) {
    d[o..o + 4].copy_from_slice(&v.to_le_bytes())
}
fn put64(d: &mut [u8], o: usize, v: u64) {
    d[o..o + 8].copy_from_slice(&v.to_le_bytes())
}
fn align(v: u64, a: u64) -> anyhow::Result<u64> {
    let rounded = v
        .checked_add(a - 1)
        .ok_or_else(|| anyhow::anyhow!("alignment overflow"))?;
    Ok(rounded & !(a - 1))
}
fn name16(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&x| x == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

#[derive(Clone, Debug)]
struct Segment {
    name: String,
    vmaddr: u64,
    vmsize: u64,
    fileoff: u64,
    filesize: u64,
    initprot: u32,
    sections: Vec<Section>,
}
#[derive(Clone, Debug)]
struct Section {
    name: String,
    fileoff: usize,
    size: usize,
}
#[derive(Debug)]
struct Parsed {
    commands: Vec<(u32, Vec<u8>)>,
    segments: Vec<Segment>,
    first_content: usize,
    signature_blobs: Vec<(usize, usize)>,
}
fn parse(data: &[u8]) -> anyhow::Result<Parsed> {
    if data.len() < HEADER_SIZE
        || u32at(data, 0)? != MH_MAGIC_64
        || u32at(data, 4)? != CPU_TYPE_ARM64
        || !matches!(u32at(data, 12)?, MH_EXECUTE | MH_DYLIB)
    {
        anyhow::bail!("input must be a thin ARM64 executable or dylib Mach-O")
    }
    if u32at(data, 8)? & 0x00ff_ffff == 2 {
        anyhow::bail!(
            "arm64e static callback-pointer bridges are not supported; provide a plain arm64 slice"
        )
    }
    let ncmds = u32at(data, 16)?;
    let sizeofcmds = u32at(data, 20)?;
    let end = HEADER_SIZE
        .checked_add(sizeofcmds as usize)
        .ok_or_else(|| anyhow::anyhow!("load-command size overflow"))?;
    if end > data.len() || ncmds > 65536 {
        anyhow::bail!("malformed Mach-O load commands")
    }
    let mut p = HEADER_SIZE;
    let mut commands = Vec::new();
    let mut segments = Vec::new();
    let mut first_content = data.len();
    let mut signatures = Vec::new();
    for _ in 0..ncmds {
        if p + 8 > end {
            anyhow::bail!("truncated load command")
        }
        let cmd = u32at(data, p)?;
        let size = u32at(data, p + 4)? as usize;
        if size < 8 || !size.is_multiple_of(4) || p + size > end {
            anyhow::bail!("invalid load command size")
        }
        let bytes = data[p..p + size].to_vec();
        if cmd == LC_SEGMENT_64 {
            if size < SEGMENT_SIZE {
                anyhow::bail!("short LC_SEGMENT_64")
            }
            let nsects = u32at(data, p + 64)? as usize;
            if SEGMENT_SIZE + nsects * 80 > size {
                anyhow::bail!("segment sections exceed command")
            };
            let mut sections = Vec::new();
            for i in 0..nsects {
                let section = p + SEGMENT_SIZE + i * 80;
                let offset = u32at(data, section + 48)? as usize;
                let size = u64at(data, section + 40)? as usize;
                sections.push(Section {
                    name: name16(&data[section..section + 16]),
                    fileoff: offset,
                    size,
                });
            }
            let segment = Segment {
                name: name16(&data[p + 8..p + 24]),
                vmaddr: u64at(data, p + 24)?,
                vmsize: u64at(data, p + 32)?,
                fileoff: u64at(data, p + 40)?,
                filesize: u64at(data, p + 48)?,
                initprot: u32at(data, p + 60)?,
                sections,
            };
            segment
                .vmaddr
                .checked_add(segment.vmsize)
                .ok_or_else(|| anyhow::anyhow!("segment VM range overflow"))?;
            let file_end = segment
                .fileoff
                .checked_add(segment.filesize)
                .ok_or_else(|| anyhow::anyhow!("segment file range overflow"))?;
            if file_end > data.len() as u64 {
                anyhow::bail!("segment file range exceeds the Mach-O")
            }
            for section in &segment.sections {
                if section.fileoff > 0
                    && section
                        .fileoff
                        .checked_add(section.size)
                        .is_none_or(|end| end > data.len())
                {
                    anyhow::bail!("section file range exceeds the Mach-O")
                }
            }
            for i in 0..nsects {
                let off = u32at(data, p + SEGMENT_SIZE + i * 80 + 48)? as usize;
                if off > 0 {
                    first_content = first_content.min(off)
                }
            }
            if nsects == 0 && segment.fileoff > 0 {
                first_content = first_content.min(segment.fileoff as usize)
            }
            segments.push(segment);
        }
        if cmd == LC_CODE_SIGNATURE && size >= 16 {
            let off = u32at(data, p + 8)? as usize;
            let len = u32at(data, p + 12)? as usize;
            if off.checked_add(len).is_some_and(|x| x <= data.len()) {
                signatures.push((off, len));
            }
        }
        commands.push((cmd, bytes));
        p += size;
    }
    if p != end {
        anyhow::bail!("load command accounting mismatch")
    }
    if first_content == data.len() {
        first_content = end
    }
    Ok(Parsed {
        commands,
        segments,
        first_content,
        signature_blobs: signatures,
    })
}

fn segment_command(
    name: &str,
    vmaddr: u64,
    vmsize: u64,
    fileoff: u64,
    filesize: u64,
    prot: u32,
) -> Vec<u8> {
    let mut c = vec![0u8; SEGMENT_SIZE];
    put32(&mut c, 0, LC_SEGMENT_64);
    put32(&mut c, 4, SEGMENT_SIZE as u32);
    c[8..8 + name.len()].copy_from_slice(name.as_bytes());
    put64(&mut c, 24, vmaddr);
    put64(&mut c, 32, vmsize);
    put64(&mut c, 40, fileoff);
    put64(&mut c, 48, filesize);
    put32(&mut c, 56, prot);
    put32(&mut c, 60, prot);
    c
}
fn dylib_command(name: &str) -> anyhow::Result<Vec<u8>> {
    let path = format!("@executable_path/Frameworks/{name}");
    let size = align((24 + path.len() + 1) as u64, 8)? as usize;
    let mut command = vec![0u8; size];
    put32(&mut command, 0, LC_LOAD_DYLIB);
    put32(&mut command, 4, size as u32);
    put32(&mut command, 8, 24);
    command[24..24 + path.len()].copy_from_slice(path.as_bytes());
    Ok(command)
}
fn load_command_name(command: &[u8]) -> Option<&str> {
    if command.len() < 24 {
        return None;
    }
    let offset = u32::from_le_bytes(command[8..12].try_into().ok()?) as usize;
    if offset >= command.len() {
        return None;
    }
    let tail = &command[offset..];
    let end = tail.iter().position(|&x| x == 0)?;
    std::str::from_utf8(&tail[..end]).ok()
}

fn shift_u32_file_offset(
    command: &mut [u8],
    field: usize,
    threshold: u64,
    delta: u64,
) -> anyhow::Result<()> {
    let value = u32at(command, field)? as u64;
    if value != 0 && value >= threshold {
        let shifted = value
            .checked_add(delta)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| anyhow::anyhow!("shifted Mach-O file offset exceeds 32 bits"))?;
        put32(command, field, shifted);
    }
    Ok(())
}

fn shift_u64_file_offset(
    command: &mut [u8],
    field: usize,
    threshold: u64,
    delta: u64,
) -> anyhow::Result<()> {
    let value = u64at(command, field)?;
    if value != 0 && value >= threshold {
        let shifted = value
            .checked_add(delta)
            .ok_or_else(|| anyhow::anyhow!("shifted Mach-O file offset overflow"))?;
        put64(command, field, shifted);
    }
    Ok(())
}

fn shift_linkedit_command_offsets(
    cmd: u32,
    command: &mut [u8],
    threshold: u64,
    delta: u64,
) -> anyhow::Result<()> {
    match cmd {
        LC_SYMTAB => {
            shift_u32_file_offset(command, 8, threshold, delta)?;
            shift_u32_file_offset(command, 16, threshold, delta)?;
        }
        LC_DYSYMTAB => {
            // tocoff, modtaboff, extrefsymoff, indirectsymoff, extreloff,
            // and locreloff. Each is followed by its element count.
            for field in [32, 40, 48, 56, 64, 72] {
                shift_u32_file_offset(command, field, threshold, delta)?;
            }
        }
        LC_DYLD_INFO | LC_DYLD_INFO_ONLY => {
            // rebase, bind, weak-bind, lazy-bind, and export offset/size pairs.
            for field in [8, 16, 24, 32, 40] {
                shift_u32_file_offset(command, field, threshold, delta)?;
            }
        }
        LC_CODE_SIGNATURE
        | LC_SEGMENT_SPLIT_INFO
        | LC_FUNCTION_STARTS
        | LC_DATA_IN_CODE
        | LC_DYLIB_CODE_SIGN_DRS
        | LC_LINKER_OPTIMIZATION_HINT
        | LC_DYLD_EXPORTS_TRIE
        | LC_DYLD_CHAINED_FIXUPS
        | LC_ATOM_INFO
        | LC_TWOLEVEL_HINTS => {
            shift_u32_file_offset(command, 8, threshold, delta)?;
        }
        LC_NOTE => shift_u64_file_offset(command, 24, threshold, delta)?,
        _ => {}
    }
    Ok(())
}
fn vm_to_file(parsed: &Parsed, address: u64) -> anyhow::Result<usize> {
    for s in &parsed.segments {
        if address >= s.vmaddr && address < s.vmaddr + s.filesize {
            return Ok((s.fileoff + address - s.vmaddr) as usize);
        }
    }
    anyhow::bail!("VM address 0x{address:x} is not file-backed")
}
fn file_to_vm(parsed: &Parsed, offset: usize) -> anyhow::Result<u64> {
    for s in &parsed.segments {
        if (offset as u64) >= s.fileoff && (offset as u64) < s.fileoff + s.filesize {
            return Ok(s.vmaddr + offset as u64 - s.fileoff);
        }
    }
    anyhow::bail!("file offset 0x{offset:x} is not mapped")
}
fn image_base(parsed: &Parsed) -> anyhow::Result<u64> {
    if let Some(text) = parsed
        .segments
        .iter()
        .find(|segment| segment.name == "__TEXT" && segment.filesize > 0)
    {
        return Ok(text.vmaddr);
    }
    parsed
        .segments
        .iter()
        .filter(|segment| segment.filesize > 0)
        .map(|segment| segment.vmaddr)
        .min()
        .ok_or_else(|| anyhow::anyhow!("Mach-O has no file-backed segments"))
}
fn executable_ranges(parsed: &Parsed, data_len: usize) -> Vec<std::ops::Range<usize>> {
    parsed
        .segments
        .iter()
        .filter(|s| s.initprot & 4 != 0)
        .filter_map(|s| {
            let a = s.fileoff as usize;
            let b = a.saturating_add(s.filesize as usize).min(data_len);
            (a < b).then_some(a..b)
        })
        .collect()
}
fn choose(mut hits: Vec<usize>, index: Option<usize>, label: &str) -> anyhow::Result<usize> {
    hits.sort_unstable();
    hits.dedup();
    match index {
        Some(i) => hits.get(i).copied().ok_or_else(|| {
            anyhow::anyhow!(
                "{label}: match_index {i} is out of range ({} matches)",
                hits.len()
            )
        }),
        None if hits.len() == 1 => Ok(hits[0]),
        None => anyhow::bail!(
            "{label}: expected one match, found {}; supply an explicit match index",
            hits.len()
        ),
    }
}
fn pattern_hits(
    data: &[u8],
    parsed: &Parsed,
    text: &str,
    segment_name: Option<&str>,
    section_name: Option<&str>,
) -> anyhow::Result<Vec<usize>> {
    let alternatives = pattern::parse(text)?;
    let mut hits = Vec::new();
    let ranges = if let Some(section_name) = section_name {
        let mut ranges = Vec::new();
        for segment in parsed
            .segments
            .iter()
            .filter(|segment| segment.initprot & 4 != 0)
            .filter(|segment| segment_name.is_none_or(|name| segment.name == name))
        {
            for section in segment
                .sections
                .iter()
                .filter(|section| section.name == section_name)
            {
                let end = section.fileoff.saturating_add(section.size).min(data.len());
                if section.fileoff < end {
                    ranges.push(section.fileoff..end);
                }
            }
        }
        if ranges.is_empty() {
            anyhow::bail!("no executable section matched {section_name}")
        }
        ranges
    } else if let Some(segment_name) = segment_name {
        let ranges = parsed
            .segments
            .iter()
            .filter(|segment| segment.name == segment_name && segment.initprot & 4 != 0)
            .filter_map(|segment| {
                let start = segment.fileoff as usize;
                let end = start
                    .saturating_add(segment.filesize as usize)
                    .min(data.len());
                (start < end).then_some(start..end)
            })
            .collect::<Vec<_>>();
        if ranges.is_empty() {
            anyhow::bail!("no executable segment matched {segment_name}")
        }
        ranges
    } else {
        executable_ranges(parsed, data.len())
    };
    for range in ranges {
        // Only instruction starts can match, so test aligned offsets only.
        let start = range.start + (4 - range.start % 4) % 4;
        if start >= range.end {
            continue;
        }
        for p in &alternatives {
            for local in pattern::find_aligned(&data[start..range.end], p, 4) {
                hits.push(start + local)
            }
        }
    }
    Ok(hits)
}
/// File offset of the target one signature points at.
fn resolve_signature(data: &[u8], parsed: &Parsed, signature: &Signature) -> anyhow::Result<usize> {
    let hits = pattern_hits(data, parsed, &signature.pattern, None, None)?;
    let hit = match (signature.match_index, hits.len()) {
        (None, 0) => anyhow::bail!("no match"),
        (None, 1) => hits[0],
        (None, count) => anyhow::bail!("{count} matches"),
        (Some(_), _) => choose(hits, signature.match_index, "signature")?,
    };
    match signature.kind {
        SignatureKind::Direct => {
            let target = (hit as i128) + signature.offset.unwrap_or(0) as i128;
            let target = usize::try_from(target)
                .map_err(|_| anyhow::anyhow!("offset moves the target before the file"))?;
            file_to_vm(parsed, target)?;
            Ok(target)
        }
        SignatureKind::Xref => {
            let pos = hit + signature.branch_index.unwrap_or(0) * 4;
            let instruction = u32at(data, pos)?;
            let target = pattern::resolve_branch_target(instruction, file_to_vm(parsed, pos)?)?;
            vm_to_file(parsed, target)
        }
    }
}
/// Resolves a `signatures` locator; returns the target and how it was found.
fn locate_signatures(
    data: &[u8],
    parsed: &Parsed,
    label: &str,
    signatures: &[Signature],
) -> anyhow::Result<(usize, String)> {
    let mut resolved = Vec::new();
    let mut problems = Vec::new();
    for (index, signature) in signatures.iter().enumerate() {
        match resolve_signature(data, parsed, signature) {
            Ok(off) => resolved.push((index + 1, off)),
            Err(error) => problems.push(format!(
                "signature {} ({:?}): {error}",
                index + 1,
                signature.kind
            )),
        }
    }
    if let Some(&(_, first)) = resolved.first() {
        if resolved.iter().any(|&(_, off)| off != first) {
            let found = resolved
                .iter()
                .map(|&(number, off)| {
                    let vm = file_to_vm(parsed, off).unwrap_or(0);
                    format!("signature {number} -> 0x{vm:x}")
                })
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!("{label}: signatures disagree ({found}); refusing to guess")
        }
        let numbers = resolved
            .iter()
            .map(|(number, _)| number.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let mut source = format!("signature {numbers} of {}", signatures.len());
        if !problems.is_empty() {
            source += &format!("; unresolved: {}", problems.join("; "));
        }
        return Ok((first, source));
    }
    anyhow::bail!("{label}: no signature resolved ({})", problems.join("; "))
}
fn symbol_address(data: &[u8], name: &str) -> anyhow::Result<u64> {
    let file = object::File::parse(data)?;
    let mut found = Vec::new();
    for s in file.symbols().chain(file.dynamic_symbols()) {
        if s.address() != 0 && s.name().ok() == Some(name) {
            found.push(s.address())
        }
    }
    found.sort_unstable();
    found.dedup();
    if found.len() != 1 {
        anyhow::bail!("symbol {name} resolved to {} addresses", found.len())
    }
    Ok(found[0])
}
#[derive(Debug, Deserialize)]
struct Il2CppMethodMap {
    #[serde(default)]
    methods: Vec<Il2CppMethodEntry>,
}
#[derive(Debug, Deserialize)]
struct Il2CppMethodEntry {
    id: Option<String>,
    assembly: String,
    namespace: String,
    class: String,
    method: String,
    parameter_count: Option<u32>,
    module: Option<String>,
    rva: Option<u64>,
    vmaddr: Option<u64>,
}
fn resolve_il2cpp_locator(
    manifest: &Manifest,
    patch: &Patch,
    locator: &Locator,
    parsed: &Parsed,
) -> anyhow::Result<u64> {
    let Locator::Il2cpp {
        assembly,
        namespace,
        class,
        method,
        parameter_count,
        method_id,
        rva,
        manifest: map_path,
    } = locator
    else {
        anyhow::bail!("internal error: non-IL2CPP locator passed to IL2CPP resolver")
    };
    if let Some(rva) = rva {
        return Ok(image_base(parsed)? + *rva);
    }
    let configured = map_path
        .as_ref()
        .or(manifest.analysis.il2cpp_method_manifest.as_ref())
        .ok_or_else(|| {
            anyhow::anyhow!("IL2CPP locator needs rva or analysis.il2cpp_method_manifest")
        })?;
    let path = if configured.is_absolute() {
        configured.clone()
    } else {
        manifest.source_dir.join(configured)
    };
    let text = std::fs::read_to_string(&path).map_err(|error| {
        anyhow::anyhow!(
            "cannot read IL2CPP method manifest {}: {error}",
            path.display()
        )
    })?;
    let map: Il2CppMethodMap = serde_yaml::from_str(&text)?;
    let mut matches = map
        .methods
        .iter()
        .filter(|entry| {
            let identity = method_id
                .as_deref()
                .is_some_and(|id| entry.id.as_deref() == Some(id))
                || (method_id.is_none()
                    && entry.assembly == assembly.as_str()
                    && entry.namespace == namespace.as_str()
                    && entry.class == class.as_str()
                    && entry.method == method.as_str()
                    && parameter_count.is_none_or(|count| entry.parameter_count == Some(count)));
            identity
                && patch.module.as_deref().is_none_or(|module| {
                    entry.module.as_deref().is_none_or(|value| value == module)
                })
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        anyhow::bail!(
            "IL2CPP method resolved to {} entries in {}",
            matches.len(),
            path.display()
        )
    }
    let entry = matches.remove(0);
    match (entry.vmaddr, entry.rva) {
        (Some(value), None) => Ok(value),
        (None, Some(value)) => Ok(image_base(parsed)? + value),
        _ => anyhow::bail!("IL2CPP method entry must contain exactly one of rva or vmaddr"),
    }
}
/// Everything `locate` may consult besides the module itself.
#[derive(Default)]
pub struct LocateContext<'a> {
    /// The managed index for this module, when the IPA had usable metadata.
    managed: Option<&'a ManagedIndex<'a>>,
    /// Why there is no managed index, for the error a named hook deserves.
    managed_error: Option<String>,
}

/// A resolved target plus how it was found.
#[cfg_attr(test, derive(Debug))]
struct Located {
    offset: usize,
    vmaddr: u64,
    source: String,
    notes: Vec<String>,
    /// Set when a managed name chose one generic instantiation.
    instantiation: Option<String>,
    /// Set when an entry stub was followed: the address before the hop.
    stub_from: Option<u64>,
}

/// Builds a [`ManagedQuery`] from a manifest locator's managed options.
fn managed_query(
    spec: &str,
    assembly: &Option<String>,
    instantiation: &Option<Instantiation>,
    offset: Option<i64>,
    follow_entry_stub: Option<bool>,
) -> anyhow::Result<ManagedQuery> {
    let mut query = ManagedQuery::parse(spec)?;
    query.assembly = assembly.clone();
    query.offset = offset.unwrap_or(0);
    query.follow_entry_stub = follow_entry_stub.unwrap_or(true);
    query.instantiation = match instantiation {
        None => None,
        Some(Instantiation::Named(word)) => Some(match word.as_str() {
            "any" => il2cpp_name::Instantiation::Any,
            _ => il2cpp_name::Instantiation::Shared,
        }),
        Some(Instantiation::Args(args)) => Some(il2cpp_name::Instantiation::Args(args.clone())),
    };
    Ok(query)
}

/// Resolves a managed name against the index, or explains why it cannot.
fn resolve_managed(
    ctx: &LocateContext<'_>,
    query: &ManagedQuery,
) -> anyhow::Result<il2cpp_name::Resolution> {
    let Some(index) = ctx.managed else {
        anyhow::bail!(
            "managed name {} needs the IPA's global-metadata.dat: {}",
            query.display(),
            ctx.managed_error
                .as_deref()
                .unwrap_or("no metadata was supplied to the patcher")
        )
    };
    index.resolve(query)
}

fn locate(
    data: &[u8],
    parsed: &Parsed,
    manifest: &Manifest,
    patch: &Patch,
    ctx: &LocateContext<'_>,
) -> anyhow::Result<Located> {
    let locator = &patch.locator;
    let label = &patch.name;
    let mut source = locator_name(locator).to_string();
    let mut notes = Vec::new();
    let mut chosen_instantiation = None;
    let mut stub_from = None;
    let off = match locator {
        Locator::Managed {
            method,
            assembly,
            instantiation,
            offset,
            follow_entry_stub,
        } => {
            let query = managed_query(
                method,
                assembly,
                instantiation,
                *offset,
                *follow_entry_stub,
            )?;
            let resolution = resolve_managed(ctx, &query)?;
            source = resolution.how.clone();
            chosen_instantiation = resolution.instantiation.clone();
            stub_from = resolution.stub_from;
            notes.extend(resolution.notes);
            vm_to_file(parsed, resolution.vmaddr)?
        }
        Locator::Signatures {
            signatures,
            managed,
            use_managed,
            assembly,
            instantiation,
            on_conflict,
            offset,
            follow_entry_stub,
        } => {
            let managed_spec = managed.as_deref().filter(|_| *use_managed);
            let Some(spec) = managed_spec else {
                if let Some(name) = managed {
                    notes.push(format!(
                        "managed name {name} is recorded but ignored (use_managed is false)"
                    ));
                }
                let (off, found) = locate_signatures(data, parsed, label, signatures)?;
                source = found;
                return Ok(Located {
                    offset: off,
                    vmaddr: file_to_vm(parsed, off)?,
                    source,
                    notes,
                    instantiation: None,
                    stub_from: None,
                });
            };
            let query = managed_query(
                spec,
                assembly,
                instantiation,
                *offset,
                *follow_entry_stub,
            )?;
            let managed_result = resolve_managed(ctx, &query);
            // Resolve every signature independently so a cross-check can name
            // the ones that have drifted.
            let mut resolved = Vec::new();
            let mut problems = Vec::new();
            for (index, signature) in signatures.iter().enumerate() {
                match resolve_signature(data, parsed, signature) {
                    Ok(off) => resolved.push((index + 1, off)),
                    Err(error) => problems.push(format!(
                        "signature {} ({:?}): {error}",
                        index + 1,
                        signature.kind
                    )),
                }
            }
            match managed_result {
                Ok(resolution) => {
                    let named = vm_to_file(parsed, resolution.vmaddr)?;
                    let agreeing: Vec<usize> = resolved
                        .iter()
                        .filter(|(_, off)| *off == named)
                        .map(|(number, _)| *number)
                        .collect();
                    let stale: Vec<(usize, usize)> = resolved
                        .iter()
                        .copied()
                        .filter(|(_, off)| *off != named)
                        .collect();
                    // Two or more signatures agreeing with each other against
                    // the name means the name found the wrong body.
                    let consensus_against = stale.len() >= 2
                        && stale.iter().all(|(_, off)| *off == stale[0].1);
                    if consensus_against
                        && on_conflict.unwrap_or_default() == ConflictPolicy::Fail
                    {
                        anyhow::bail!(
                            "{label}: managed name {} resolves to 0x{:x} but {} signatures agree \
                             on 0x{:x}; refusing to guess. Re-check the name, or set \
                             on_conflict: prefer_managed once you know the signatures are stale.",
                            query.display(),
                            resolution.vmaddr,
                            stale.len(),
                            file_to_vm(parsed, stale[0].1)?
                        )
                    }
                    source = format!(
                        "{}{}",
                        resolution.how,
                        if agreeing.is_empty() {
                            String::new()
                        } else {
                            format!(
                                ", cross-checked by signature {}",
                                agreeing
                                    .iter()
                                    .map(|n| n.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        }
                    );
                    for (number, off) in &stale {
                        notes.push(format!(
                            "signature {number} is stale: it resolves to 0x{:x}, the name resolves \
                             to 0x{:x}",
                            file_to_vm(parsed, *off)?,
                            resolution.vmaddr
                        ));
                    }
                    for problem in &problems {
                        notes.push(format!("{problem} (name resolved, so this is not fatal)"));
                    }
                    notes.extend(resolution.notes);
                    chosen_instantiation = resolution.instantiation.clone();
                    stub_from = resolution.stub_from;
                    named
                }
                Err(managed_error) => {
                    let (off, found) = locate_signatures(data, parsed, label, signatures)
                        .map_err(|signature_error| {
                            anyhow::anyhow!(
                                "{label}: neither the managed name nor the signatures resolved.\n\
                                 name: {managed_error}\nsignatures: {signature_error}"
                            )
                        })?;
                    source = format!("{found} (managed name did not resolve)");
                    notes.push(format!(
                        "managed name {} did not resolve, fell back to signatures: {managed_error}",
                        query.display()
                    ));
                    off
                }
            }
        }
        Locator::Pattern {
            pattern,
            match_index,
            expected_matches,
            segment,
            section,
        } => {
            let hits = pattern_hits(data, parsed, pattern, segment.as_deref(), section.as_deref())?;
            if expected_matches.is_some_and(|count| hits.len() != count) {
                anyhow::bail!(
                    "{label}: expected {expected_matches:?} pattern matches, found {}",
                    hits.len()
                )
            }
            choose(hits, *match_index, label)?
        }
        Locator::Symbol { symbol } | Locator::Export { symbol } => vm_to_file(parsed, symbol_address(data, symbol)?)?,
        Locator::Va { value } | Locator::Vmaddr { value } => vm_to_file(parsed, *value)?,
        Locator::Rva { value } => vm_to_file(parsed, image_base(parsed)? + value)?,
        Locator::Xref {
            pattern,
            xref_pattern,
            match_index,
            xref_match_index,
            xref_bl_index,
        } => {
            let targets = pattern_hits(data, parsed, pattern, None, None)?;
            if let Some(xpat) = xref_pattern {
                let target_off = choose(targets, *match_index, &format!("{label} target pattern"))?;
                let target_vm = file_to_vm(parsed, target_off)?;
                let calls = pattern_hits(data, parsed, xpat, None, None)?
                    .into_iter()
                    .filter(|off| {
                        let pos = *off + xref_bl_index.unwrap_or(0) * 4;
                        data.get(pos..pos + 4).is_some_and(|bytes| {
                            file_to_vm(parsed, pos)
                                .is_ok_and(|pc| pattern::find_bl_xrefs(bytes, pc, target_vm) == [0])
                        })
                    })
                    .collect();
                let _ = choose(calls, *xref_match_index, &format!("{label} xref pattern"))?;
                target_off
            } else {
                let call = choose(targets, *match_index, label)?;
                let pos = call + xref_bl_index.unwrap_or(0) * 4;
                let insn = u32::from_le_bytes(
                    data.get(pos..pos + 4)
                        .ok_or_else(|| anyhow::anyhow!("{label}: BL is truncated"))?
                        .try_into()
                        .unwrap(),
                );
                let target = pattern::resolve_bl_target(insn, file_to_vm(parsed, pos)?)?;
                vm_to_file(parsed, target)?
            }
        }
        Locator::Il2cpp { .. } => {
            vm_to_file(parsed, resolve_il2cpp_locator(manifest, patch, locator, parsed)?)?
        }
        Locator::Custom { resolver, value } => anyhow::bail!(
            "{label}: custom resolver '{resolver}' ({value}) needs an offline adapter; arbitrary resolver code is not executed"
        ),
    };
    if off % 4 != 0 {
        anyhow::bail!("{label}: resolved target is unaligned")
    }
    Ok(Located {
        offset: off,
        vmaddr: file_to_vm(parsed, off)?,
        source,
        notes,
        instantiation: chosen_instantiation,
        stub_from,
    })
}
fn locator_name(locator: &Locator) -> &'static str {
    match locator {
        Locator::Pattern { .. } => "pattern",
        Locator::Xref { .. } => "xref",
        Locator::Signatures { .. } => "signatures",
        Locator::Managed { .. } => "managed name",
        Locator::Symbol { .. } => "symbol",
        Locator::Export { .. } => "export",
        Locator::Va { .. } | Locator::Vmaddr { .. } => "fixed vmaddr",
        Locator::Rva { .. } => "fixed rva",
        Locator::Il2cpp { .. } => "il2cpp method map",
        Locator::Custom { .. } => "custom",
    }
}
/// `expect` may use wildcards so one expectation holds across versions.
fn expect_matches(expect: &str, bytes: &[u8; 4]) -> anyhow::Result<bool> {
    let parsed = pattern::parse(expect)?;
    let expected = &parsed[0].0;
    if expected.len() != 4 {
        anyhow::bail!("expect must be exactly four bytes or wildcards")
    }
    Ok(expected
        .iter()
        .zip(bytes)
        .all(|(want, have)| want.is_none_or(|value| value == *have)))
}
fn config_bytes(m: &Manifest) -> Vec<u8> {
    let mut c = vec![0u8; CONFIG_SIZE];
    c[..8].copy_from_slice(b"IAOTCFG1");
    put32(&mut c, 8, 2);
    put32(&mut c, 12, CONFIG_SIZE as u32);
    put32(&mut c, 16, FLAGS_OFFSET as u32);
    put32(&mut c, 20, 256);
    put32(&mut c, 24, CALLBACKS_OFFSET as u32);
    put32(&mut c, 28, 64);
    put32(&mut c, 32, STRINGS_OFFSET as u32);
    put32(&mut c, 36, 32);
    put32(&mut c, 40, ORIGINALS_OFFSET as u32);
    put32(&mut c, 44, ORIGINALS_COUNT as u32);
    for f in &m.config.flags {
        put32(
            &mut c,
            FLAGS_OFFSET + f.index as usize * 4,
            u32::from(f.default),
        );
    }
    for s in &m.config.strings {
        let start = STRINGS_OFFSET + s.index as usize * 256;
        let b = s.default.as_bytes();
        c[start..start + b.len()].copy_from_slice(b);
    }
    c
}

#[derive(Debug)]
pub struct PatchReport {
    pub patches: usize,
    pub config_vmaddr: u64,
    pub cave_vmaddr: u64,
    pub near_cave_vmaddr: Option<u64>,
    pub removed_code_signatures: usize,
    pub added_dylib_load_command: bool,
    pub resolutions: Vec<PatchResolution>,
    pub failures: Vec<PatchFailure>,
}
#[derive(Debug)]
pub struct PatchResolution {
    pub name: String,
    pub vmaddr: u64,
    pub bytes: [u8; 4],
    pub automatic: bool,
    pub original_vmaddr: Option<u64>,
    /// How the target was found, e.g. "signature 1, 2 of 2".
    pub located_by: String,
    /// Cross-check, stub and degradation notes from a managed resolution.
    pub notes: Vec<String>,
    /// The generic instantiation that was hooked, when one was chosen.
    pub instantiation: Option<String>,
    /// The entry address before an entry stub was followed.
    pub stub_from: Option<u64>,
}
#[derive(Debug)]
pub struct PatchFailure {
    pub name: String,
    pub reason: String,
}
#[derive(Debug, Default, Clone)]
pub struct PatchOptions {
    pub load_dylib: bool,
    /// `global-metadata.dat` from the same IPA, for `managed` locators.
    pub metadata: Option<std::path::PathBuf>,
}
#[cfg(test)]
pub fn patch(m: &Manifest, input: &Path, output: &Path) -> anyhow::Result<PatchReport> {
    patch_with_options(m, input, output, PatchOptions::default())
}
pub fn patch_with_options(
    m: &Manifest,
    input: &Path,
    output: &Path,
    options: PatchOptions,
) -> anyhow::Result<PatchReport> {
    if input == output {
        anyhow::bail!("input and output paths must differ")
    }
    let original = std::fs::read(input)?;
    let parsed = parse(&original)?;
    let page = m.runtime.page_size;
    let linkedit = parsed
        .segments
        .iter()
        .find(|segment| segment.name == "__LINKEDIT")
        .ok_or_else(|| anyhow::anyhow!("Mach-O has no __LINKEDIT segment"))?;
    let linkedit_end = linkedit
        .fileoff
        .checked_add(linkedit.filesize)
        .ok_or_else(|| anyhow::anyhow!("__LINKEDIT file range overflow"))?;
    if linkedit_end != original.len() as u64 {
        anyhow::bail!(
            "__LINKEDIT must occupy the original file tail (ends at 0x{linkedit_end:x}, file is 0x{:x})",
            original.len()
        )
    }
    let max_vm = parsed
        .segments
        .iter()
        .filter(|segment| segment.name != "__LINKEDIT")
        .map(|s| s.vmaddr + s.vmsize)
        .max()
        .ok_or_else(|| anyhow::anyhow!("no segments"))?;
    let config_vm = align(max_vm, page)?;
    let mut config = config_bytes(m);
    let cave_vm = align(
        config_vm
            .checked_add(config.len() as u64)
            .ok_or_else(|| anyhow::anyhow!("config VM range overflow"))?,
        page,
    )?;
    let flags: HashMap<_, _> = m
        .config
        .flags
        .iter()
        .map(|x| (x.name.as_str(), x.index))
        .collect();
    let callbacks: HashMap<_, _> = m
        .config
        .callbacks
        .iter()
        .map(|x| (x.name.as_str(), x.index))
        .collect();
    // A managed index is only built when a hook actually names its target.
    let wants_managed = m.patches.iter().filter(|p| p.enabled).any(|p| {
        matches!(&p.locator, Locator::Managed { .. })
            || matches!(
                &p.locator,
                Locator::Signatures {
                    managed: Some(_),
                    use_managed: true,
                    ..
                }
            )
    });
    let metadata_bytes;
    let mut managed_index = None;
    let mut managed_error = None;
    if wants_managed {
        match &options.metadata {
            Some(path) => match std::fs::read(path) {
                Ok(bytes) => {
                    metadata_bytes = bytes;
                    match ManagedIndex::build(&original, &metadata_bytes) {
                        Ok(index) => managed_index = Some(index),
                        Err(error) => managed_error = Some(error.to_string()),
                    }
                }
                Err(error) => {
                    managed_error = Some(format!("cannot read {}: {error}", path.display()))
                }
            },
            None => {
                managed_error = Some(
                    "this IPA has no global-metadata.dat, or this module is not the IL2CPP module"
                        .into(),
                )
            }
        }
    }
    let locate_context = LocateContext {
        managed: managed_index.as_ref(),
        managed_error,
    };
    if let Some(index) = &locate_context.managed {
        let summary = index.summary();
        println!(
            "  managed locator: {}",
            summary
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    let mut targets = Vec::new();
    let mut resolutions = Vec::new();
    let mut failures = Vec::new();
    let mut target_offsets = HashSet::new();
    let mut sources: HashMap<String, String> = HashMap::new();
    let mut patch_notes: HashMap<String, Vec<String>> = HashMap::new();
    let mut patch_managed: HashMap<String, (Option<String>, Option<u64>)> = HashMap::new();
    for p in m.patches.iter().filter(|p| p.enabled) {
        let resolved = (|| -> anyhow::Result<_> {
            let located = locate(&original, &parsed, m, p, &locate_context)?;
            let (off, vm, source, notes) = (
                located.offset,
                located.vmaddr,
                located.source,
                located.notes,
            );
            patch_managed.insert(p.name.clone(), (located.instantiation, located.stub_from));
            let bytes: [u8; 4] = original
                .get(off..off + 4)
                .ok_or_else(|| anyhow::anyhow!("resolved instruction is truncated"))?
                .try_into()
                .unwrap();
            if let Some(expect) = &p.expect {
                if expect != "auto" && !expect_matches(expect, &bytes)? {
                    anyhow::bail!(
                        "expected bytes do not match at 0x{vm:x} (found {:02X} {:02X} {:02X} {:02X}, located by {source})",
                        bytes[0], bytes[1], bytes[2], bytes[3]
                    )
                }
            }
            if !target_offsets.insert(off) {
                anyhow::bail!("another enabled patch already targets file offset 0x{off:x}")
            }
            sources.insert(p.name.clone(), source);
            patch_notes.insert(p.name.clone(), notes);
            Ok((off, vm, bytes, u32::from_le_bytes(bytes)))
        })();
        let (off, vm, bytes, displaced) = match resolved {
            Ok(value) => value,
            Err(error) if p.failure_behavior == FailureBehavior::DisableHook => {
                failures.push(PatchFailure {
                    name: p.name.clone(),
                    reason: error.to_string(),
                });
                continue;
            }
            Err(error) => return Err(error.context(format!("patch {} resolution failed", p.name))),
        };
        targets.push((p, off, vm, bytes, displaced));
    }

    let mut direct_rewrites = Vec::new();
    let mut cave_targets = Vec::new();
    for target @ (patch, off, vm, bytes, _) in targets {
        if patch.patch.kind == "branch_to_instruction_target" {
            let built = (|| -> anyhow::Result<u64> {
                let instruction_offset = patch.patch.instruction_offset.unwrap_or(0);
                let source_off = (off as i128)
                    .checked_add(instruction_offset as i128)
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| anyhow::anyhow!("branch source offset overflow"))?;
                if !source_off.is_multiple_of(4) {
                    anyhow::bail!("branch source instruction is unaligned")
                }
                let source_vm = file_to_vm(&parsed, source_off)?;
                let source_instruction = u32at(&original, source_off)?;
                let target_vm = arm64::encoded_branch_target(source_instruction, source_vm)?;
                vm_to_file(&parsed, target_vm)?;
                arm64::b(vm, target_vm)?;
                Ok(target_vm)
            })();
            let target_vm = match built {
                Ok(value) => value,
                Err(error) if patch.failure_behavior == FailureBehavior::DisableHook => {
                    failures.push(PatchFailure {
                        name: patch.name.clone(),
                        reason: error.to_string(),
                    });
                    continue;
                }
                Err(error) => {
                    return Err(error.context(format!(
                        "patch {} direct branch generation failed",
                        patch.name
                    )))
                }
            };
            let managed_detail = patch_managed.remove(&patch.name).unwrap_or_default();
            resolutions.push(PatchResolution {
                name: patch.name.clone(),
                vmaddr: vm,
                bytes,
                automatic: patch.expect.as_deref().is_none_or(|value| value == "auto"),
                original_vmaddr: None,
                located_by: sources.remove(&patch.name).unwrap_or_default(),
                notes: patch_notes.remove(&patch.name).unwrap_or_default(),
                instantiation: managed_detail.0,
                stub_from: managed_detail.1,
            });
            direct_rewrites.push((off, vm, target_vm));
        } else {
            cave_targets.push(target);
        }
    }

    // Most images can reach the appended cave with a normal ARM64 B. Very large
    // main executables cannot. A file-backed executable segment below __TEXT is
    // rejected by stock dyld in this layout, so those sites use a prebuilt
    // ADRP/ADD/BR veneer at the original site instead of a near segment.
    let mut cave = Vec::new();
    let mut redirects = Vec::new();
    let mut occupied_targets: HashSet<usize> =
        direct_rewrites.iter().map(|(off, _, _)| *off).collect();
    for (p, off, vm, displaced_bytes, _displaced) in cave_targets {
        while cave.len() % 4 != 0 {
            cave.push(0)
        }
        let far_start = cave_vm
            .checked_add(cave.len() as u64)
            .ok_or_else(|| anyhow::anyhow!("code-cave VM range overflow"))?;
        let patch_width = if arm64::b(vm, far_start).is_err() {
            3
        } else {
            1
        };
        for index in 0..patch_width {
            if !occupied_targets.insert(off + index * 4) {
                anyhow::bail!("patch {} overlaps another patch's branch veneer", p.name)
            }
        }
        let displaced = (0..patch_width)
            .map(|index| u32at(&original, off + index * 4))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let start = far_start;
        let built = (|| -> anyhow::Result<(Vec<u8>, u64, Option<u64>)> {
            let mut local = Vec::new();
            let original_vm = if p.original_slot.is_some() {
                let address = start;
                local
                    .extend_from_slice(&caves::build_original_trampoline(address, vm, &displaced)?);
                while local.len() % 4 != 0 {
                    local.push(0)
                }
                Some(address)
            } else {
                None
            };
            let address = start + local.len() as u64;
            let action = p.patch.action.as_ref().unwrap();
            let (flag, callback, replacement) = match action {
                PatchAction::RetIfFlag { flag } | PatchAction::ZeroRegisterIfFlag { flag, .. } => {
                    (Some(flags[flag.as_str()]), None, None)
                }
                PatchAction::CallbackIfFlag { flag, callback }
                | PatchAction::CallbackReturnZeroIfFlag { flag, callback } => (
                    Some(flags[flag.as_str()]),
                    Some(callbacks[callback.as_deref().unwrap()]),
                    None,
                ),
                PatchAction::CallbackAfter { callback }
                | PatchAction::BranchToCallback { callback } => {
                    (None, Some(callbacks[callback.as_deref().unwrap()]), None)
                }
                PatchAction::BranchToReplacement { symbol } => {
                    (None, None, Some(symbol_address(&original, symbol)?))
                }
            };
            let bridge = caves::build(&CaveRequest {
                action,
                cave_address: address,
                // A direct B replaces one instruction.  An out-of-range
                // redirect uses ADRP/ADD/BR and replaces three, so the
                // trampoline must resume after the complete veneer.
                return_address: vm + (patch_width as u64) * 4,
                original_address: vm,
                displaced: &displaced,
                config_address: config_vm,
                flag_index: flag,
                callback_index: callback,
                replacement_address: replacement,
            })?;
            if patch_width == 1 {
                arm64::b(vm, address)?;
            } else {
                arm64::long_branch(vm, address)?;
            }
            local.extend_from_slice(&bridge);
            Ok((local, address, original_vm))
        })();
        let (bytes, bridge_address, original_vm) = match built {
            Ok(value) => value,
            Err(error) if p.failure_behavior == FailureBehavior::DisableHook => {
                failures.push(PatchFailure {
                    name: p.name.clone(),
                    reason: error.to_string(),
                });
                continue;
            }
            Err(error) => {
                return Err(error.context(format!("patch {} bridge generation failed", p.name)))
            }
        };
        if let (Some(slot), Some(original_vm)) = (p.original_slot, original_vm) {
            put64(
                &mut config,
                ORIGINALS_OFFSET + slot as usize * 8,
                original_vm,
            );
        }
        let managed_detail = patch_managed.remove(&p.name).unwrap_or_default();
        resolutions.push(PatchResolution {
            name: p.name.clone(),
            vmaddr: vm,
            bytes: displaced_bytes,
            automatic: p.expect.as_deref().is_none_or(|x| x == "auto"),
            original_vmaddr: original_vm,
            located_by: sources.remove(&p.name).unwrap_or_default(),
            notes: patch_notes.remove(&p.name).unwrap_or_default(),
            instantiation: managed_detail.0,
            stub_from: managed_detail.1,
        });
        cave.extend_from_slice(&bytes);
        redirects.push((off, vm, bridge_address, patch_width));
    }
    if cave.is_empty() {
        cave.extend_from_slice(&arm64::ret().to_le_bytes())
    }

    // Fully linked Mach-O images require __LINKEDIT to remain the final
    // file-backed segment. Insert the IAOT segments immediately before it,
    // move the complete linkedit payload to the new tail, and shift every load
    // command that points into that payload. This also leaves the code
    // signature at EOF where codesign/ldid expect to replace it.
    let config_vm_size = align(config.len() as u64, page)?;
    let cave_vm_size = align(cave.len() as u64, page)?;
    let config_file = align(linkedit.fileoff, page)?;
    if config_file != linkedit.fileoff {
        anyhow::bail!("__LINKEDIT file offset is not page aligned")
    }
    let cave_file = align(
        config_file
            .checked_add(config_vm_size)
            .ok_or_else(|| anyhow::anyhow!("config file range overflow"))?,
        page,
    )?;
    let moved_linkedit_file = align(
        cave_file
            .checked_add(cave_vm_size)
            .ok_or_else(|| anyhow::anyhow!("cave file range overflow"))?,
        page,
    )?;
    let moved_linkedit_vm = align(
        cave_vm
            .checked_add(cave_vm_size)
            .ok_or_else(|| anyhow::anyhow!("cave VM range overflow"))?,
        page,
    )?;
    let linkedit_file_delta = moved_linkedit_file
        .checked_sub(linkedit.fileoff)
        .ok_or_else(|| anyhow::anyhow!("__LINKEDIT file relocation underflow"))?;

    let mut commands = Vec::new();
    let mut kept = 0u32;
    let signature_commands: Vec<Vec<u8>> = parsed
        .commands
        .iter()
        .filter(|(cmd, _)| *cmd == LC_CODE_SIGNATURE)
        .map(|(_, bytes)| bytes.clone())
        .collect();
    if signature_commands.len() > 1 {
        anyhow::bail!(
            "expected exactly one LC_CODE_SIGNATURE before patching, found {}",
            signature_commands.len()
        )
    }
    let dylib_path = format!("@executable_path/Frameworks/{}", m.runtime.dylib_name);
    let already_loads_dylib = parsed.commands.iter().any(|(cmd, bytes)| {
        *cmd == LC_LOAD_DYLIB && load_command_name(bytes) == Some(dylib_path.as_str())
    });
    for (cmd, bytes) in &parsed.commands {
        // Keep the original signature load command.  The output is unsigned,
        // but external signers such as TrollStore's ldid require this command
        // in order to place a replacement CodeDirectory and signature blob.
        // It is appended below so it remains the final load command.
        if *cmd == LC_CODE_SIGNATURE {
            continue;
        }
        if *cmd == LC_SEGMENT_64 && name16(&bytes[8..24]) == "__LINKEDIT" {
            commands.extend_from_slice(&segment_command(
                &m.runtime.config_segment,
                config_vm,
                config_vm_size,
                config_file,
                config.len() as u64,
                3,
            ));
            commands.extend_from_slice(&segment_command(
                &m.runtime.cave_segment,
                cave_vm,
                cave_vm_size,
                cave_file,
                cave.len() as u64,
                5,
            ));
            let mut moved_linkedit = bytes.clone();
            put64(&mut moved_linkedit, 24, moved_linkedit_vm);
            put64(&mut moved_linkedit, 40, moved_linkedit_file);
            commands.extend_from_slice(&moved_linkedit);
            kept += 3;
            continue;
        }
        let mut command = bytes.clone();
        shift_linkedit_command_offsets(*cmd, &mut command, linkedit.fileoff, linkedit_file_delta)?;
        commands.extend_from_slice(&command);
        kept += 1;
    }
    let added_dylib_load_command = options.load_dylib && !already_loads_dylib;
    if added_dylib_load_command {
        commands.extend_from_slice(&dylib_command(&m.runtime.dylib_name)?);
        kept += 1;
    }
    if let Some(signature_command) = signature_commands.first() {
        let mut command = signature_command.clone();
        shift_linkedit_command_offsets(
            LC_CODE_SIGNATURE,
            &mut command,
            linkedit.fileoff,
            linkedit_file_delta,
        )?;
        commands.extend_from_slice(&command);
        kept += 1;
    }
    if HEADER_SIZE + commands.len() > parsed.first_content {
        anyhow::bail!(
            "not enough load-command padding: need {} bytes, have {}",
            commands.len(),
            parsed.first_content - HEADER_SIZE
        )
    }
    let mut out = original[..linkedit.fileoff as usize].to_vec();
    out.resize(config_file as usize, 0);
    out.extend_from_slice(&config);
    out.resize(cave_file as usize, 0);
    out.extend_from_slice(&cave);
    out.resize(moved_linkedit_file as usize, 0);
    out.extend_from_slice(&original[linkedit.fileoff as usize..]);
    // Keep the existing blob parseable until the external signer replaces it.
    // Zeroing it makes ldid/TrollStore reject the Mach-O before it gets a
    // chance to generate the new signature.  The old signature is invalidated
    // by the changed executable and is intentionally superseded during resign.
    out[HEADER_SIZE..parsed.first_content].fill(0);
    out[HEADER_SIZE..HEADER_SIZE + commands.len()].copy_from_slice(&commands);
    put32(&mut out, 16, kept);
    put32(&mut out, 20, commands.len() as u32);
    for (off, vm, target, width) in &redirects {
        if *width == 1 {
            out[*off..*off + 4].copy_from_slice(&arm64::b(*vm, *target)?.to_le_bytes())
        } else {
            for (index, instruction) in arm64::long_branch(*vm, *target)?.iter().enumerate() {
                out[*off + index * 4..*off + index * 4 + 4]
                    .copy_from_slice(&instruction.to_le_bytes());
            }
        }
    }
    for (off, vm, target) in &direct_rewrites {
        out[*off..*off + 4].copy_from_slice(&arm64::b(*vm, *target)?.to_le_bytes());
    }
    verify_data(&out, m, Some(&redirects), Some(&direct_rewrites))?;
    let output_parent = output.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(output_parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(output_parent)?;
    temporary.write_all(&out)?;
    temporary.as_file_mut().sync_all()?;
    let reopened = std::fs::read(temporary.path())?;
    if let Err(error) = verify_data(&reopened, m, Some(&redirects), Some(&direct_rewrites)) {
        return Err(error.context("reopened output verification failed"));
    }
    if options.load_dylib {
        let reopened_parsed = parse(&reopened)?;
        if !reopened_parsed.commands.iter().any(|(cmd, bytes)| {
            *cmd == LC_LOAD_DYLIB && load_command_name(bytes) == Some(dylib_path.as_str())
        }) {
            anyhow::bail!("reopened output is missing the requested dylib load command")
        }
    }
    temporary.persist(output).map_err(|error| error.error)?;
    Ok(PatchReport {
        patches: redirects.len() + direct_rewrites.len(),
        config_vmaddr: config_vm,
        cave_vmaddr: cave_vm,
        near_cave_vmaddr: None,
        removed_code_signatures: parsed.signature_blobs.len(),
        added_dylib_load_command,
        resolutions,
        failures,
    })
}

fn verify_data(
    data: &[u8],
    m: &Manifest,
    redirects: Option<&[(usize, u64, u64, usize)]>,
    direct_rewrites: Option<&[(usize, u64, u64)]>,
) -> anyhow::Result<()> {
    let p = parse(data)?;
    let cfg = p
        .segments
        .iter()
        .find(|s| s.name == m.runtime.config_segment)
        .ok_or_else(|| anyhow::anyhow!("config segment missing"))?;
    let cave = p
        .segments
        .iter()
        .find(|s| s.name == m.runtime.cave_segment)
        .ok_or_else(|| anyhow::anyhow!("cave segment missing"))?;
    if cfg.initprot != 3
        || cave.initprot != 5
        || cfg.fileoff as usize + CONFIG_SIZE > data.len()
        || &data[cfg.fileoff as usize..cfg.fileoff as usize + 8] != b"IAOTCFG1"
        || u32at(data, cfg.fileoff as usize + 8)? != 2
        || u32at(data, cfg.fileoff as usize + 12)? as usize != CONFIG_SIZE
        || u32at(data, cfg.fileoff as usize + 40)? as usize != ORIGINALS_OFFSET
        || u32at(data, cfg.fileoff as usize + 44)? as usize != ORIGINALS_COUNT
    {
        anyhow::bail!("IAOT segment protection or config layout verification failed")
    }
    for (i, a) in p.segments.iter().enumerate() {
        for b in p.segments.iter().skip(i + 1) {
            if a.vmsize > 0
                && b.vmsize > 0
                && a.vmaddr < b.vmaddr + b.vmsize
                && b.vmaddr < a.vmaddr + a.vmsize
            {
                anyhow::bail!("overlapping segment VM ranges: {} and {}", a.name, b.name)
            }
        }
    }
    // Stock dyld requires both VM addresses and file offsets to be monotonic in
    // segment load-command order. Keep this check here because structural
    // overlap validation alone does not catch a launch-time DYLD abort.
    let mut previous_vmaddr: Option<u64> = None;
    let mut previous_fileoff: Option<u64> = None;
    for s in &p.segments {
        if let Some(prev) = previous_vmaddr {
            if s.vmaddr < prev {
                anyhow::bail!(
                    "segment {} VM address out of order (0x{:x} follows 0x{:x}); dyld will refuse to launch this binary",
                    s.name, s.vmaddr, prev
                )
            }
        }
        previous_vmaddr = Some(s.vmaddr);
        if s.filesize == 0 && s.fileoff == 0 {
            continue;
        }
        if let Some(prev) = previous_fileoff {
            if s.fileoff < prev {
                anyhow::bail!(
                    "segment {} file offset out of order (0x{:x} follows 0x{:x} in load-command order); dyld will refuse to launch this binary",
                    s.name,
                    s.fileoff,
                    prev
                )
            }
        }
        previous_fileoff = Some(s.fileoff);
    }
    if p.segments
        .iter()
        .rfind(|segment| segment.filesize > 0)
        .is_none_or(|segment| segment.name != "__LINKEDIT")
    {
        anyhow::bail!("__LINKEDIT is not the final file-backed segment")
    }
    if p.commands
        .iter()
        .filter(|x| x.0 == LC_CODE_SIGNATURE)
        .count()
        > 1
    {
        anyhow::bail!("duplicate LC_CODE_SIGNATURE load commands")
    }
    verify_code_signature_layout(data, &p)?;
    if let Some(rs) = redirects {
        for (off, pc, target, width) in rs {
            let i = u32::from_le_bytes(data[*off..*off + 4].try_into().unwrap());
            let in_far_cave = *target >= cave.vmaddr && *target < cave.vmaddr + cave.vmsize;
            let valid_branch = if *width == 1 {
                i == arm64::b(*pc, *target)?
            } else {
                let veneer = arm64::long_branch(*pc, *target)?;
                let expected: Vec<u8> = veneer.iter().flat_map(|word| word.to_le_bytes()).collect();
                data.get(*off..*off + expected.len()) == Some(expected.as_slice())
            };
            if !valid_branch || !in_far_cave {
                anyhow::bail!("redirect verification failed")
            }
        }
    }
    if let Some(rewrites) = direct_rewrites {
        for (off, pc, target) in rewrites {
            let instruction = u32::from_le_bytes(data[*off..*off + 4].try_into().unwrap());
            if instruction != arm64::b(*pc, *target)? {
                anyhow::bail!("direct branch rewrite verification failed")
            }
        }
    }
    Ok(())
}

// The code signature is linkedit data: its file range must remain within the
// __LINKEDIT segment.  A signer may replace its contents and update the load
// command, but placing the replacement after an unrelated appended segment
// produces a Mach-O that can pass simple range checks yet be rejected by dyld.
fn verify_code_signature_layout(data: &[u8], parsed: &Parsed) -> anyhow::Result<()> {
    // Minimal synthetic fixtures and some bare test dylibs legitimately have
    // no linkedit segment.  For a production image that does have one, though,
    // every signature range must be contained in it.
    let Some(linkedit) = parsed
        .segments
        .iter()
        .find(|segment| segment.name == "__LINKEDIT")
    else {
        return Ok(());
    };
    let signatures = parsed
        .commands
        .iter()
        .filter(|(cmd, _)| *cmd == LC_CODE_SIGNATURE)
        .collect::<Vec<_>>();
    if signatures.len() != 1 {
        anyhow::bail!(
            "expected exactly one LC_CODE_SIGNATURE load command, found {}",
            signatures.len()
        )
    }
    let command = &signatures[0].1;
    if command.len() < 16 {
        anyhow::bail!("short LC_CODE_SIGNATURE load command")
    }
    let offset = u32at(command, 8)? as u64;
    let size = u32at(command, 12)? as u64;
    let end = offset
        .checked_add(size)
        .ok_or_else(|| anyhow::anyhow!("code-signature range overflow"))?;
    let linkedit_end = linkedit
        .fileoff
        .checked_add(linkedit.filesize)
        .ok_or_else(|| anyhow::anyhow!("__LINKEDIT file range overflow"))?;
    if size == 0 || end > data.len() as u64 || offset < linkedit.fileoff || end > linkedit_end {
        anyhow::bail!(
            "LC_CODE_SIGNATURE range 0x{offset:x}..0x{end:x} is not contained in __LINKEDIT file range 0x{:x}..0x{:x}",
            linkedit.fileoff,
            linkedit_end
        )
    }
    Ok(())
}

pub fn verify(path: &Path) -> anyhow::Result<()> {
    let data = std::fs::read(path)?;
    let p = parse(&data)?;
    for (i, a) in p.segments.iter().enumerate() {
        for b in p.segments.iter().skip(i + 1) {
            if a.vmsize > 0
                && b.vmsize > 0
                && a.vmaddr < b.vmaddr + b.vmsize
                && b.vmaddr < a.vmaddr + a.vmsize
            {
                anyhow::bail!("overlapping VM segments")
            }
        }
    }
    for segment in &p.segments {
        let start = segment.fileoff as usize;
        if segment.filesize as usize >= CONFIG_SIZE
            && data.get(start..start + 8) == Some(b"IAOTCFG1")
            && (segment.initprot != 3
                || u32at(&data, start + 8)? != 2
                || u32at(&data, start + 12)? as usize != CONFIG_SIZE
                || u32at(&data, start + 16)? as usize != FLAGS_OFFSET
                || u32at(&data, start + 24)? as usize != CALLBACKS_OFFSET
                || u32at(&data, start + 32)? as usize != STRINGS_OFFSET
                || u32at(&data, start + 40)? as usize != ORIGINALS_OFFSET
                || u32at(&data, start + 44)? as usize != ORIGINALS_COUNT)
        {
            anyhow::bail!("invalid IAOT runtime config layout")
        }
    }
    verify_code_signature_layout(&data, &p)?;
    Ok(())
}
pub fn verify_dylib(path: &Path) -> anyhow::Result<()> {
    let data = std::fs::read(path)?;
    parse(&data)?;
    if u32at(&data, 12)? != MH_DYLIB {
        anyhow::bail!("runtime artifact is not an MH_DYLIB Mach-O")
    }
    Ok(())
}
pub fn inspect(path: &Path) -> anyhow::Result<String> {
    let data = std::fs::read(path)?;
    let file = object::File::parse(&*data)?;
    let mut out = format!(
        "format={:?} arch={:?} kind={:?}\n",
        file.format(),
        file.architecture(),
        file.kind()
    );
    for s in file.segments() {
        out.push_str(&format!(
            "segment {:?}: address=0x{:x} size=0x{:x}\n",
            s.name().ok().flatten(),
            s.address(),
            s.size()
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn fixture(section_offset: u32, instruction: u32) -> Vec<u8> {
        let mut d = vec![0u8; 0x5000];
        put32(&mut d, 0, MH_MAGIC_64);
        put32(&mut d, 4, CPU_TYPE_ARM64);
        put32(&mut d, 12, MH_EXECUTE);
        put32(&mut d, 16, 3);
        put32(&mut d, 20, 240);
        let p = HEADER_SIZE;
        put32(&mut d, p, LC_SEGMENT_64);
        put32(&mut d, p + 4, 152);
        d[p + 8..p + 14].copy_from_slice(b"__TEXT");
        put64(&mut d, p + 24, 0x1_0000_0000);
        put64(&mut d, p + 32, 0x4000);
        put64(&mut d, p + 48, 0x4000);
        put32(&mut d, p + 56, 7);
        put32(&mut d, p + 60, 5);
        put32(&mut d, p + 64, 1);
        let s = p + 72;
        d[s..s + 6].copy_from_slice(b"__text");
        d[s + 16..s + 22].copy_from_slice(b"__TEXT");
        put64(&mut d, s + 32, 0x1_0000_0000 + section_offset as u64);
        put64(&mut d, s + 40, 0x100);
        put32(&mut d, s + 48, section_offset);
        let linkedit = p + 152;
        put32(&mut d, linkedit, LC_SEGMENT_64);
        put32(&mut d, linkedit + 4, SEGMENT_SIZE as u32);
        d[linkedit + 8..linkedit + 18].copy_from_slice(b"__LINKEDIT");
        put64(&mut d, linkedit + 24, 0x1_0000_4000);
        put64(&mut d, linkedit + 32, 0x4000);
        put64(&mut d, linkedit + 40, 0x4000);
        put64(&mut d, linkedit + 48, 0x1000);
        put32(&mut d, linkedit + 56, 1);
        put32(&mut d, linkedit + 60, 1);
        let c = linkedit + SEGMENT_SIZE;
        put32(&mut d, c, LC_CODE_SIGNATURE);
        put32(&mut d, c + 4, 16);
        put32(&mut d, c + 8, 0x4800);
        put32(&mut d, c + 12, 0x800);
        d[0x4800..0x5000].fill(0xaa);
        put32(&mut d, section_offset as usize, instruction);
        put32(&mut d, 0x400, instruction);
        d
    }
    fn far_fixture(instruction: u32) -> Vec<u8> {
        let mut d = vec![0u8; 0x5000];
        put32(&mut d, 0, MH_MAGIC_64);
        put32(&mut d, 4, CPU_TYPE_ARM64);
        put32(&mut d, 12, MH_EXECUTE);
        put32(&mut d, 16, 4);
        put32(&mut d, 20, 312);

        let pagezero = HEADER_SIZE;
        put32(&mut d, pagezero, LC_SEGMENT_64);
        put32(&mut d, pagezero + 4, SEGMENT_SIZE as u32);
        d[pagezero + 8..pagezero + 18].copy_from_slice(b"__PAGEZERO");
        put64(&mut d, pagezero + 24, 0);
        put64(&mut d, pagezero + 32, 0x1_0000_0000);

        let text = pagezero + SEGMENT_SIZE;
        put32(&mut d, text, LC_SEGMENT_64);
        put32(&mut d, text + 4, 152);
        d[text + 8..text + 14].copy_from_slice(b"__TEXT");
        put64(&mut d, text + 24, 0x1_0000_0000);
        put64(&mut d, text + 32, 0x1000_0000);
        put64(&mut d, text + 48, 0x4000);
        put32(&mut d, text + 56, 7);
        put32(&mut d, text + 60, 5);
        put32(&mut d, text + 64, 1);
        let section = text + SEGMENT_SIZE;
        d[section..section + 6].copy_from_slice(b"__text");
        d[section + 16..section + 22].copy_from_slice(b"__TEXT");
        put64(&mut d, section + 32, 0x1_0000_0400);
        put64(&mut d, section + 40, 0x100);
        put32(&mut d, section + 48, 0x400);

        let linkedit = text + 152;
        put32(&mut d, linkedit, LC_SEGMENT_64);
        put32(&mut d, linkedit + 4, SEGMENT_SIZE as u32);
        d[linkedit + 8..linkedit + 18].copy_from_slice(b"__LINKEDIT");
        put64(&mut d, linkedit + 24, 0x1_1000_0000);
        put64(&mut d, linkedit + 32, 0x4000);
        put64(&mut d, linkedit + 40, 0x4000);
        put64(&mut d, linkedit + 48, 0x1000);
        put32(&mut d, linkedit + 56, 1);
        put32(&mut d, linkedit + 60, 1);

        let signature = linkedit + SEGMENT_SIZE;
        put32(&mut d, signature, LC_CODE_SIGNATURE);
        put32(&mut d, signature + 4, 16);
        put32(&mut d, signature + 8, 0x4800);
        put32(&mut d, signature + 12, 0x800);
        d[0x4800..0x5000].fill(0xaa);
        put32(&mut d, 0x400, instruction);
        d
    }

    fn manifest() -> Manifest {
        serde_yaml::from_str("project: { name: Demo, bundle_id: com.example.demo, platform: ios, arch: arm64, min_ios: '14.0' }\nruntime: { config_segment: __IAOTCFG, cave_segment: __IAOTCAVE, page_size: 16384, dylib_name: Demo.dylib }\napp: {}\nconfig:\n  flags: [{ name: enabled, index: 0, default: true }]\n  strings: [{ name: text, index: 0, default: hello }]\n  callbacks: []\nfunctions: []\ngenerated_hooks: []\npatches:\n  - name: test\n    enabled: true\n    description: owned synthetic fixture\n    locator: { type: va, value: 0x100000400 }\n    patch: { type: branch_to_cave, action: { type: ret_if_flag, flag: enabled } }\n    expect: '1F 20 03 D5'\n").unwrap()
    }

    /// Target function at 0x800, a BL to it at 0x600 and a tail-call B at 0x704.
    fn signature_fixture() -> Vec<u8> {
        let mut d = fixture(0x400, 0xd503201f);
        for (index, word) in [0xd10043ffu32, 0x52800540, 0x910043ff, 0xd65f03c0]
            .into_iter()
            .enumerate()
        {
            put32(&mut d, 0x800 + index * 4, word);
        }
        put32(&mut d, 0x600, 0x9400_0080); // bl 0x800
        put32(&mut d, 0x604, 0xaa1303e0); // mov x0, x19
        put32(&mut d, 0x700, 0xd280_0000); // mov x0, #0
        put32(&mut d, 0x704, 0x1400_003f); // b 0x800
        d
    }

    fn signature(kind: SignatureKind, pattern: &str) -> Signature {
        Signature {
            kind,
            pattern: pattern.into(),
            branch_index: None,
            offset: None,
            match_index: None,
        }
    }

    #[test]
    fn signatures_agree_across_direct_xref_and_tail_call() {
        let d = signature_fixture();
        let parsed = parse(&d).unwrap();
        let mut tail = signature(SignatureKind::Xref, "00 00 80 D2 ?? ?? ?? 14");
        tail.branch_index = Some(1);
        let signatures = [
            signature(SignatureKind::Direct, "FF 43 00 D1 40 05 80 52"),
            signature(SignatureKind::Xref, "?? ?? ?? 94 E0 03 13 AA"),
            tail,
            signature(SignatureKind::Direct, "DE AD BE EF"),
        ];
        let (off, source) = locate_signatures(&d, &parsed, "t", &signatures).unwrap();
        assert_eq!(off, 0x800);
        assert!(source.starts_with("signature 1, 2, 3 of 4"), "{source}");
        assert!(
            source.contains("signature 4 (Direct): no match"),
            "{source}"
        );
    }

    #[test]
    fn direct_offset_reaches_a_site_inside_the_match() {
        let d = signature_fixture();
        let parsed = parse(&d).unwrap();
        let mut site = signature(SignatureKind::Direct, "FF 43 00 D1 40 05 80 52");
        site.offset = Some(8);
        let (off, _) = locate_signatures(&d, &parsed, "t", &[site]).unwrap();
        assert_eq!(off, 0x808);
    }

    #[test]
    fn disagreeing_signatures_are_refused() {
        let d = signature_fixture();
        let parsed = parse(&d).unwrap();
        let mut shifted = signature(SignatureKind::Direct, "FF 43 00 D1 40 05 80 52");
        shifted.offset = Some(4);
        let signatures = [
            signature(SignatureKind::Xref, "?? ?? ?? 94 E0 03 13 AA"),
            shifted,
        ];
        let error = locate_signatures(&d, &parsed, "t", &signatures).unwrap_err();
        assert!(error.to_string().contains("disagree"), "{error}");
    }

    /// Builds a patch whose locator is `signatures` plus a managed name.
    fn managed_patch(
        managed: Option<&str>,
        use_managed: bool,
        on_conflict: Option<ConflictPolicy>,
        signatures: Vec<Signature>,
    ) -> Patch {
        let mut m = manifest();
        let mut patch = m.patches.remove(0);
        patch.locator = Locator::Signatures {
            signatures,
            managed: managed.map(str::to_string),
            use_managed,
            assembly: None,
            instantiation: None,
            on_conflict,
            offset: None,
            follow_entry_stub: None,
        };
        patch
    }

    /// Without metadata there is no index, so a named hook must fall back to
    /// its signatures and say so rather than failing the build.
    #[test]
    fn a_named_hook_falls_back_to_signatures_without_metadata() {
        let d = signature_fixture();
        let parsed = parse(&d).unwrap();
        let m = manifest();
        let patch = managed_patch(
            Some("Ns.Class::Method()"),
            true,
            None,
            vec![signature(SignatureKind::Direct, "FF 43 00 D1 40 05 80 52")],
        );
        let ctx = LocateContext {
            managed: None,
            managed_error: Some("no metadata was supplied".into()),
        };
        let located = locate(&d, &parsed, &m, &patch, &ctx).unwrap();
        assert_eq!(located.offset, 0x800);
        assert!(
            located.source.contains("managed name did not resolve"),
            "{}",
            located.source
        );
        assert!(
            located
                .notes
                .iter()
                .any(|note| note.contains("fell back to signatures")),
            "{:?}",
            located.notes
        );
    }

    /// `use_managed: false` keeps the name in the manifest without acting on
    /// it, so the hook behaves exactly like a signatures-only hook.
    #[test]
    fn a_recorded_name_is_ignored_when_the_toggle_is_off() {
        let d = signature_fixture();
        let parsed = parse(&d).unwrap();
        let m = manifest();
        let patch = managed_patch(
            Some("Ns.Class::Method()"),
            false,
            None,
            vec![signature(SignatureKind::Direct, "FF 43 00 D1 40 05 80 52")],
        );
        let ctx = LocateContext::default();
        let located = locate(&d, &parsed, &m, &patch, &ctx).unwrap();
        assert_eq!(located.offset, 0x800);
        assert!(
            located.source.starts_with("signature 1 of 1"),
            "{}",
            located.source
        );
        assert!(
            located
                .notes
                .iter()
                .any(|note| note.contains("recorded but ignored")),
            "{:?}",
            located.notes
        );
    }

    /// Neither side resolving must report both failures, not just one.
    #[test]
    fn a_named_hook_reports_both_failures() {
        let d = signature_fixture();
        let parsed = parse(&d).unwrap();
        let m = manifest();
        let patch = managed_patch(
            Some("Ns.Class::Method()"),
            true,
            None,
            vec![signature(SignatureKind::Direct, "DE AD BE EF")],
        );
        let ctx = LocateContext {
            managed: None,
            managed_error: Some("no metadata was supplied".into()),
        };
        let error = locate(&d, &parsed, &m, &patch, &ctx)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("neither the managed name nor the signatures"),
            "{error}"
        );
        assert!(error.contains("no metadata was supplied"), "{error}");
        assert!(error.contains("no match"), "{error}");
    }

    #[test]
    fn wildcard_expect_matches_instruction_class() {
        let sub_sp = 0xd10043ffu32.to_le_bytes();
        assert!(expect_matches("FF ?? ?? D1", &sub_sp).unwrap());
        assert!(!expect_matches("FF ?? ?? 91", &sub_sp).unwrap());
        assert!(expect_matches("FF 43 00 D1", &sub_sp).unwrap());
    }

    #[test]
    fn writes_segments_redirect_and_preserves_signature_command_for_resigning() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in");
        let output = dir.path().join("out");
        std::fs::write(&input, fixture(0x400, 0xd503201f)).unwrap();
        let m = manifest();
        crate::manifest::validate(&m).unwrap();
        let report = patch(&m, &input, &output).unwrap();
        assert_eq!(report.patches, 1);
        assert_eq!(report.removed_code_signatures, 1);
        let out = std::fs::read(&output).unwrap();
        let p = parse(&out).unwrap();
        assert!(p.segments.iter().any(|s| s.name == "__IAOTCFG"));
        assert!(p.segments.iter().any(|s| s.name == "__IAOTCAVE"));
        assert_eq!(
            p.commands
                .iter()
                .filter(|x| x.0 == LC_CODE_SIGNATURE)
                .count(),
            1
        );
        let cfg = p.segments.iter().find(|s| s.name == "__IAOTCFG").unwrap();
        assert_eq!(
            &out[cfg.fileoff as usize..cfg.fileoff as usize + 8],
            b"IAOTCFG1"
        );
        assert_ne!(u32at(&out, 0x400).unwrap(), 0xd503201f);
    }

    #[test]
    fn writes_and_verifies_exact_direct_branch_rewrite() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in");
        let output = dir.path().join("out");
        let mut input_bytes = fixture(0x400, 0xd503201f);
        put32(&mut input_bytes, 0x404, 0x34000020);
        std::fs::write(&input, input_bytes).unwrap();
        let mut m = manifest();
        m.patches[0].patch.kind = "branch_to_instruction_target".into();
        m.patches[0].patch.action = None;
        m.patches[0].patch.instruction_offset = Some(4);
        crate::manifest::validate(&m).unwrap();

        let report = patch(&m, &input, &output).unwrap();
        assert_eq!(report.patches, 1);
        assert_eq!(report.resolutions[0].bytes, [0x1f, 0x20, 0x03, 0xd5]);
        let out = std::fs::read(&output).unwrap();
        assert_eq!(u32at(&out, 0x400).unwrap(), 0x14000002);
    }

    #[test]
    fn uses_signed_long_branch_when_direct_branch_is_out_of_range() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in");
        let output = dir.path().join("out");
        std::fs::write(&input, far_fixture(0xd503201f)).unwrap();
        let report = patch(&manifest(), &input, &output).unwrap();
        assert_eq!(report.patches, 1);
        assert_eq!(report.near_cave_vmaddr, None);

        let out = std::fs::read(&output).unwrap();
        let parsed = parse(&out).unwrap();
        assert!(!parsed
            .segments
            .iter()
            .any(|segment| segment.name == "__IAOTNRC"));
        assert_eq!(u32at(&out, 0x400).unwrap() & 0x9f00_001f, 0x9000_0010);
        assert_eq!(image_base(&parsed).unwrap(), 0x1_0000_0000);
    }

    #[test]
    fn refuses_missing_load_command_padding() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in");
        let output = dir.path().join("out");
        std::fs::write(&input, fixture(0xd0, 0xd503201f)).unwrap();
        assert!(patch(&manifest(), &input, &output)
            .unwrap_err()
            .to_string()
            .contains("load-command padding"));
        assert!(!output.exists());
    }
    #[test]
    fn rejects_overflowing_segment_ranges() {
        let mut data = fixture(0x400, 0xd503201f);
        put64(&mut data, HEADER_SIZE + 24, u64::MAX - 0x1000);
        put64(&mut data, HEADER_SIZE + 32, 0x4000);
        assert!(parse(&data)
            .unwrap_err()
            .to_string()
            .contains("segment VM range overflow"));
    }
    #[test]
    fn relocates_supported_pc_relative_displacement() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in");
        let output = dir.path().join("out");
        std::fs::write(&input, fixture(0x400, 0x14000001)).unwrap();
        let mut m = manifest();
        m.patches[0].expect = Some("01 00 00 14".into());
        let report = patch(&m, &input, &output).unwrap();
        assert_eq!(report.patches, 1);
        assert!(output.is_file());
    }
    #[test]
    fn resolves_bytes_and_adds_dylib_command() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in");
        let output = dir.path().join("out");
        std::fs::write(&input, fixture(0x400, 0xd503201f)).unwrap();
        let mut m = manifest();
        m.patches[0].expect = Some("auto".into());
        let report = patch_with_options(
            &m,
            &input,
            &output,
            PatchOptions {
                load_dylib: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(report.resolutions[0].automatic);
        assert_eq!(report.resolutions[0].bytes, [0x1f, 0x20, 0x03, 0xd5]);
        assert!(report.added_dylib_load_command);
        let parsed = parse(&std::fs::read(output).unwrap()).unwrap();
        assert!(parsed
            .commands
            .iter()
            .any(|(cmd, bytes)| *cmd == LC_LOAD_DYLIB
                && load_command_name(bytes) == Some("@executable_path/Frameworks/Demo.dylib")));
    }
    #[test]
    fn isolated_bridge_failure_is_not_reported_as_applied() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in");
        let output = dir.path().join("out");
        std::fs::write(&input, fixture(0x400, 0xd503201f)).unwrap();
        let mut m = manifest();
        m.patches[0].failure_behavior = FailureBehavior::DisableHook;
        m.patches[0].patch.action = Some(PatchAction::BranchToReplacement {
            symbol: "_MissingOwnedSymbol".into(),
        });
        let report = patch(&m, &input, &output).unwrap();
        assert_eq!(report.patches, 0);
        assert!(report.resolutions.is_empty());
        assert_eq!(report.failures.len(), 1);
    }

    fn linkedit_fixture(instruction: u32) -> Vec<u8> {
        // __TEXT [0,0x4000) vm 0x1_0000_0000 with __text at 0x400; __LINKEDIT
        // [0x4000,0x4400) vm 0x1_0000_4000 holding a symbol table and a trailing
        // code-signature blob. LC_SYMTAB and LC_CODE_SIGNATURE point into
        // __LINKEDIT so the relocation offset fix-ups are exercised.
        let mut d = vec![0u8; 0x4400];
        put32(&mut d, 0, MH_MAGIC_64);
        put32(&mut d, 4, CPU_TYPE_ARM64);
        put32(&mut d, 12, MH_EXECUTE);
        put32(&mut d, 16, 4);
        put32(&mut d, 20, (152 + SEGMENT_SIZE + 24 + 16) as u32);

        let text = HEADER_SIZE;
        put32(&mut d, text, LC_SEGMENT_64);
        put32(&mut d, text + 4, 152);
        d[text + 8..text + 14].copy_from_slice(b"__TEXT");
        put64(&mut d, text + 24, 0x1_0000_0000);
        put64(&mut d, text + 32, 0x4000);
        put64(&mut d, text + 40, 0);
        put64(&mut d, text + 48, 0x4000);
        put32(&mut d, text + 56, 7);
        put32(&mut d, text + 60, 5);
        put32(&mut d, text + 64, 1);
        let section = text + SEGMENT_SIZE;
        d[section..section + 6].copy_from_slice(b"__text");
        d[section + 16..section + 22].copy_from_slice(b"__TEXT");
        put64(&mut d, section + 32, 0x1_0000_0400);
        put64(&mut d, section + 40, 0x100);
        put32(&mut d, section + 48, 0x400);

        let linkedit = text + 152;
        put32(&mut d, linkedit, LC_SEGMENT_64);
        put32(&mut d, linkedit + 4, SEGMENT_SIZE as u32);
        d[linkedit + 8..linkedit + 18].copy_from_slice(b"__LINKEDIT");
        put64(&mut d, linkedit + 24, 0x1_0000_4000);
        put64(&mut d, linkedit + 32, 0x4000);
        put64(&mut d, linkedit + 40, 0x4000);
        put64(&mut d, linkedit + 48, 0x400);
        put32(&mut d, linkedit + 56, 1);
        put32(&mut d, linkedit + 60, 1);

        let symtab = linkedit + SEGMENT_SIZE;
        put32(&mut d, symtab, LC_SYMTAB);
        put32(&mut d, symtab + 4, 24);
        put32(&mut d, symtab + 8, 0x4000); // symoff -> start of __LINKEDIT
        put32(&mut d, symtab + 12, 1);
        put32(&mut d, symtab + 16, 0x4100); // stroff -> inside __LINKEDIT
        put32(&mut d, symtab + 20, 0x40);

        let signature = symtab + 24;
        put32(&mut d, signature, LC_CODE_SIGNATURE);
        put32(&mut d, signature + 4, 16);
        put32(&mut d, signature + 8, 0x4200); // dataoff -> tail of __LINKEDIT
        put32(&mut d, signature + 12, 0x100);
        d[0x4200..0x4300].fill(0xaa);

        put32(&mut d, 0x400, instruction);
        d
    }

    #[test]
    fn moves_linkedit_behind_iaot_segments_and_shifts_offsets() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in");
        let output = dir.path().join("out");
        std::fs::write(&input, linkedit_fixture(0xd503201f)).unwrap();
        let report = patch(&manifest(), &input, &output).unwrap();
        assert_eq!(report.patches, 1);

        let out = std::fs::read(&output).unwrap();
        let p = parse(&out).unwrap();
        let mut previous_vm = 0;
        let mut previous_file = 0;
        for s in &p.segments {
            assert!(s.vmaddr >= previous_vm, "segment {} VM order", s.name);
            if s.filesize > 0 || s.fileoff > 0 {
                assert!(s.fileoff >= previous_file, "segment {} file order", s.name);
                previous_file = s.fileoff;
            }
            previous_vm = s.vmaddr;
        }
        assert!(p.segments.iter().any(|s| s.name == "__IAOTCFG"));
        assert!(p.segments.iter().any(|s| s.name == "__IAOTCAVE"));
        assert_eq!(p.segments.last().unwrap().name, "__LINKEDIT");
        let moved_linkedit = p.segments.last().unwrap();
        assert_eq!(moved_linkedit.vmaddr, 0x1_0000_c000);
        assert_eq!(moved_linkedit.fileoff, 0xc000);
        assert_eq!(out.len(), 0xc400);

        // All linkedit-relative offsets move by the same 0x8000 delta.
        let symtab = p
            .commands
            .iter()
            .find(|(cmd, _)| *cmd == LC_SYMTAB)
            .map(|(_, bytes)| bytes.clone())
            .unwrap();
        assert_eq!(u32at(&symtab, 8).unwrap() as u64, 0xc000);
        assert_eq!(u32at(&symtab, 16).unwrap() as u64, 0xc100);
        let signature = p
            .commands
            .iter()
            .find(|(cmd, _)| *cmd == LC_CODE_SIGNATURE)
            .map(|(_, bytes)| bytes.clone())
            .unwrap();
        assert_eq!(u32at(&signature, 8).unwrap() as u64, 0xc200);
        assert_eq!(&out[0xc200..0xc300], &[0xaa; 0x100]);
    }
}
