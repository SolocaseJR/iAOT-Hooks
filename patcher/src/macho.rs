use crate::{
    arm64,
    caves::{self, CaveRequest},
    manifest::{Locator, Manifest, PatchAction},
    pattern,
};
use object::{Object, ObjectSegment, ObjectSymbol};
use std::{collections::HashMap, path::Path};

const MH_MAGIC_64: u32 = 0xfeedfacf;
const CPU_TYPE_ARM64: u32 = 0x0100000c;
const MH_EXECUTE: u32 = 2;
const LC_SEGMENT_64: u32 = 0x19;
const LC_CODE_SIGNATURE: u32 = 0x1d;
const LC_LOAD_DYLIB: u32 = 0xc;
const HEADER_SIZE: usize = 32;
const SEGMENT_SIZE: usize = 72;
const CONFIG_SIZE: usize = 9800;
const FLAGS_OFFSET: usize = 72;
const CALLBACKS_OFFSET: usize = 1096;
const STRINGS_OFFSET: usize = 1608;

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
fn align(v: u64, a: u64) -> u64 {
    (v + a - 1) & !(a - 1)
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
        || u32at(data, 12)? != MH_EXECUTE
    {
        anyhow::bail!("input must be a thin ARM64 MH_EXECUTE Mach-O")
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
            let segment = Segment {
                name: name16(&data[p + 8..p + 24]),
                vmaddr: u64at(data, p + 24)?,
                vmsize: u64at(data, p + 32)?,
                fileoff: u64at(data, p + 40)?,
                filesize: u64at(data, p + 48)?,
                initprot: u32at(data, p + 60)?,
            };
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
fn dylib_command(name: &str) -> Vec<u8> {
    let path = format!("@executable_path/Frameworks/{name}");
    let size = align((24 + path.len() + 1) as u64, 8) as usize;
    let mut command = vec![0u8; size];
    put32(&mut command, 0, LC_LOAD_DYLIB);
    put32(&mut command, 4, size as u32);
    put32(&mut command, 8, 24);
    command[24..24 + path.len()].copy_from_slice(path.as_bytes());
    command
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
    parsed
        .segments
        .iter()
        .filter(|s| s.filesize > 0)
        .map(|s| s.vmaddr)
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
fn pattern_hits(data: &[u8], parsed: &Parsed, text: &str) -> anyhow::Result<Vec<usize>> {
    let alternatives = pattern::parse(text)?;
    let mut hits = Vec::new();
    for range in executable_ranges(parsed, data.len()) {
        for p in &alternatives {
            for local in pattern::find(&data[range.clone()], p) {
                let off = range.start + local;
                if off % 4 == 0 {
                    hits.push(off)
                }
            }
        }
    }
    Ok(hits)
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
fn locate(
    data: &[u8],
    parsed: &Parsed,
    locator: &Locator,
    label: &str,
) -> anyhow::Result<(usize, u64)> {
    let off = match locator {
        Locator::Pattern {
            pattern,
            match_index,
        } => choose(pattern_hits(data, parsed, pattern)?, *match_index, label)?,
        Locator::Symbol { symbol } => vm_to_file(parsed, symbol_address(data, symbol)?)?,
        Locator::Va { value } | Locator::Vmaddr { value } => vm_to_file(parsed, *value)?,
        Locator::Rva { value } => vm_to_file(parsed, image_base(parsed)? + value)?,
        Locator::Xref {
            pattern,
            xref_pattern,
            match_index,
            xref_match_index,
            xref_bl_index,
        } => {
            let targets = pattern_hits(data, parsed, pattern)?;
            if let Some(xpat) = xref_pattern {
                let target_off = choose(targets, *match_index, &format!("{label} target pattern"))?;
                let target_vm = file_to_vm(parsed, target_off)?;
                let calls = pattern_hits(data, parsed, xpat)?
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
    };
    if off % 4 != 0 {
        anyhow::bail!("{label}: resolved target is unaligned")
    }
    Ok((off, file_to_vm(parsed, off)?))
}
fn pc_relative(i: u32) -> bool {
    (i & 0x7c00_0000) == 0x1400_0000
        || (i & 0x1f00_0000) == 0x1000_0000
        || (i & 0xff00_0010) == 0x5400_0000
        || (i & 0x7e00_0000) == 0x3400_0000
        || (i & 0x7e00_0000) == 0x3600_0000
        || (i & 0x3b00_0000) == 0x1800_0000
}
fn config_bytes(m: &Manifest) -> Vec<u8> {
    let mut c = vec![0u8; CONFIG_SIZE];
    c[..8].copy_from_slice(b"IAOTCFG1");
    put32(&mut c, 8, 1);
    put32(&mut c, 12, CONFIG_SIZE as u32);
    put32(&mut c, 16, FLAGS_OFFSET as u32);
    put32(&mut c, 20, 256);
    put32(&mut c, 24, CALLBACKS_OFFSET as u32);
    put32(&mut c, 28, 64);
    put32(&mut c, 32, STRINGS_OFFSET as u32);
    put32(&mut c, 36, 32);
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
fn expected_bytes(text: &str) -> anyhow::Result<[u8; 4]> {
    let p = pattern::parse(text)?;
    let mut out = [0; 4];
    for (i, b) in p[0].0.iter().enumerate() {
        out[i] = b.unwrap()
    }
    Ok(out)
}

#[derive(Debug)]
pub struct PatchReport {
    pub patches: usize,
    pub config_vmaddr: u64,
    pub cave_vmaddr: u64,
    pub removed_code_signatures: usize,
    pub added_dylib_load_command: bool,
    pub resolutions: Vec<PatchResolution>,
}
#[derive(Debug)]
pub struct PatchResolution {
    pub name: String,
    pub vmaddr: u64,
    pub bytes: [u8; 4],
    pub automatic: bool,
}
#[derive(Debug, Default, Clone, Copy)]
pub struct PatchOptions {
    pub load_dylib: bool,
}
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
    let max_vm = parsed
        .segments
        .iter()
        .map(|s| s.vmaddr + s.vmsize)
        .max()
        .ok_or_else(|| anyhow::anyhow!("no segments"))?;
    let config_vm = align(max_vm, page);
    let config = config_bytes(m);
    let cave_vm = align(config_vm + config.len() as u64, page);
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
    let mut targets = Vec::new();
    let mut resolutions = Vec::new();
    for p in m.patches.iter().filter(|p| p.enabled) {
        let (off, vm) = locate(&original, &parsed, &p.locator, &p.name)?;
        let bytes: [u8; 4] = original[off..off + 4].try_into().unwrap();
        if let Some(expect) = &p.expect {
            if expect != "auto" && bytes != expected_bytes(expect)? {
                anyhow::bail!("{}: expected bytes do not match", p.name)
            }
        }
        resolutions.push(PatchResolution {
            name: p.name.clone(),
            vmaddr: vm,
            bytes,
            automatic: p.expect.as_deref().is_none_or(|x| x == "auto"),
        });
        let displaced = u32::from_le_bytes(bytes);
        if !matches!(p.patch.action, PatchAction::BranchToReplacement { .. })
            && pc_relative(displaced)
        {
            anyhow::bail!("{}: refusing to displace a PC-relative instruction", p.name)
        }
        targets.push((p, off, vm, displaced));
    }
    let mut cave = Vec::new();
    let mut redirects = Vec::new();
    for (p, off, vm, displaced) in targets {
        while cave.len() % 4 != 0 {
            cave.push(0)
        }
        let addr = cave_vm + cave.len() as u64;
        let (flag, callback, replacement) = match &p.patch.action {
            PatchAction::RetIfFlag { flag } | PatchAction::ZeroRegisterIfFlag { flag, .. } => {
                (Some(flags[flag.as_str()]), None, None)
            }
            PatchAction::CallbackIfFlag { flag, callback }
            | PatchAction::CallbackReturnZeroIfFlag { flag, callback } => (
                Some(flags[flag.as_str()]),
                Some(callbacks[callback.as_str()]),
                None,
            ),
            PatchAction::CallbackAfter { callback }
            | PatchAction::BranchToCallback { callback } => {
                (None, Some(callbacks[callback.as_str()]), None)
            }
            PatchAction::BranchToReplacement { symbol } => {
                (None, None, Some(symbol_address(&original, symbol)?))
            }
        };
        let built = caves::build(&CaveRequest {
            action: &p.patch.action,
            cave_address: addr,
            return_address: vm + 4,
            displaced,
            config_address: config_vm,
            flag_index: flag,
            callback_index: callback,
            replacement_address: replacement,
        })?;
        cave.extend_from_slice(&built);
        redirects.push((off, vm, addr));
    }
    if cave.is_empty() {
        cave.extend_from_slice(&arm64::ret().to_le_bytes())
    }
    let config_file = align(original.len() as u64, page);
    let cave_file = align(config_file + config.len() as u64, page);
    let mut commands = Vec::new();
    let mut kept = 0u32;
    let dylib_path = format!("@executable_path/Frameworks/{}", m.runtime.dylib_name);
    let already_loads_dylib = parsed.commands.iter().any(|(cmd, bytes)| {
        *cmd == LC_LOAD_DYLIB && load_command_name(bytes) == Some(dylib_path.as_str())
    });
    for (cmd, bytes) in &parsed.commands {
        if *cmd != LC_CODE_SIGNATURE {
            commands.extend_from_slice(bytes);
            kept += 1
        }
    }
    let added_dylib_load_command = options.load_dylib && !already_loads_dylib;
    if added_dylib_load_command {
        commands.extend_from_slice(&dylib_command(&m.runtime.dylib_name));
        kept += 1;
    }
    commands.extend_from_slice(&segment_command(
        &m.runtime.config_segment,
        config_vm,
        align(config.len() as u64, page),
        config_file,
        config.len() as u64,
        3,
    ));
    commands.extend_from_slice(&segment_command(
        &m.runtime.cave_segment,
        cave_vm,
        align(cave.len() as u64, page),
        cave_file,
        cave.len() as u64,
        5,
    ));
    if HEADER_SIZE + commands.len() > parsed.first_content {
        anyhow::bail!(
            "not enough load-command padding: need {} bytes, have {}",
            commands.len(),
            parsed.first_content - HEADER_SIZE
        )
    }
    let mut out = original.clone();
    for (off, len) in &parsed.signature_blobs {
        out[*off..*off + *len].fill(0)
    }
    out[HEADER_SIZE..parsed.first_content].fill(0);
    out[HEADER_SIZE..HEADER_SIZE + commands.len()].copy_from_slice(&commands);
    put32(&mut out, 16, kept + 2);
    put32(&mut out, 20, commands.len() as u32);
    out.resize(config_file as usize, 0);
    out.extend_from_slice(&config);
    out.resize(cave_file as usize, 0);
    out.extend_from_slice(&cave);
    for (off, vm, target) in &redirects {
        out[*off..*off + 4].copy_from_slice(&arm64::b(*vm, *target)?.to_le_bytes())
    }
    verify_data(&out, m, Some(&redirects))?;
    std::fs::write(output, &out)?;
    let reopened = std::fs::read(output)?;
    if let Err(error) = verify_data(&reopened, m, Some(&redirects)) {
        let _ = std::fs::remove_file(output);
        return Err(error.context("reopened output verification failed"));
    }
    if options.load_dylib {
        let reopened_parsed = parse(&reopened)?;
        if !reopened_parsed.commands.iter().any(|(cmd, bytes)| {
            *cmd == LC_LOAD_DYLIB && load_command_name(bytes) == Some(dylib_path.as_str())
        }) {
            let _ = std::fs::remove_file(output);
            anyhow::bail!("reopened output is missing the requested dylib load command")
        }
    }
    Ok(PatchReport {
        patches: redirects.len(),
        config_vmaddr: config_vm,
        cave_vmaddr: cave_vm,
        removed_code_signatures: parsed.signature_blobs.len(),
        added_dylib_load_command,
        resolutions,
    })
}

fn verify_data(
    data: &[u8],
    m: &Manifest,
    redirects: Option<&[(usize, u64, u64)]>,
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
        || u32at(data, cfg.fileoff as usize + 8)? != 1
        || u32at(data, cfg.fileoff as usize + 12)? as usize != CONFIG_SIZE
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
    if p.commands.iter().any(|x| x.0 == LC_CODE_SIGNATURE) {
        anyhow::bail!("stale LC_CODE_SIGNATURE remains")
    }
    if let Some(rs) = redirects {
        for (off, pc, target) in rs {
            let i = u32::from_le_bytes(data[*off..*off + 4].try_into().unwrap());
            if i != arm64::b(*pc, *target)?
                || *target < cave.vmaddr
                || *target >= cave.vmaddr + cave.vmsize
            {
                anyhow::bail!("redirect verification failed")
            }
        }
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
                || u32at(&data, start + 8)? != 1
                || u32at(&data, start + 12)? as usize != CONFIG_SIZE
                || u32at(&data, start + 16)? as usize != FLAGS_OFFSET
                || u32at(&data, start + 24)? as usize != CALLBACKS_OFFSET
                || u32at(&data, start + 32)? as usize != STRINGS_OFFSET)
        {
            anyhow::bail!("invalid IAOT runtime config layout")
        }
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
        let mut d = vec![0u8; 0x1000];
        put32(&mut d, 0, MH_MAGIC_64);
        put32(&mut d, 4, CPU_TYPE_ARM64);
        put32(&mut d, 12, MH_EXECUTE);
        put32(&mut d, 16, 2);
        put32(&mut d, 20, 168);
        let p = HEADER_SIZE;
        put32(&mut d, p, LC_SEGMENT_64);
        put32(&mut d, p + 4, 152);
        d[p + 8..p + 14].copy_from_slice(b"__TEXT");
        put64(&mut d, p + 24, 0x1_0000_0000);
        put64(&mut d, p + 32, 0x4000);
        put64(&mut d, p + 48, 0x1000);
        put32(&mut d, p + 56, 7);
        put32(&mut d, p + 60, 5);
        put32(&mut d, p + 64, 1);
        let s = p + 72;
        d[s..s + 6].copy_from_slice(b"__text");
        d[s + 16..s + 22].copy_from_slice(b"__TEXT");
        put64(&mut d, s + 32, 0x1_0000_0000 + section_offset as u64);
        put64(&mut d, s + 40, 0x100);
        put32(&mut d, s + 48, section_offset);
        let c = p + 152;
        put32(&mut d, c, LC_CODE_SIGNATURE);
        put32(&mut d, c + 4, 16);
        put32(&mut d, c + 8, 0x800);
        put32(&mut d, c + 12, 16);
        d[0x800..0x810].fill(0xaa);
        put32(&mut d, section_offset as usize, instruction);
        put32(&mut d, 0x400, instruction);
        d
    }
    fn manifest() -> Manifest {
        serde_yaml::from_str("project: { name: Demo, bundle_id: com.example.demo, platform: ios, arch: arm64, min_ios: '14.0' }\nruntime: { config_segment: __IAOTCFG, cave_segment: __IAOTCAVE, page_size: 16384, dylib_name: Demo.dylib }\napp: { authorized_only: true }\nconfig:\n  flags: [{ name: enabled, index: 0, default: true }]\n  strings: [{ name: text, index: 0, default: hello }]\n  callbacks: []\nfunctions: []\ngenerated_hooks: []\npatches:\n  - name: test\n    enabled: true\n    description: owned synthetic fixture\n    locator: { type: va, value: 0x100000400 }\n    patch: { type: branch_to_cave, action: { type: ret_if_flag, flag: enabled } }\n    expect: '1F 20 03 D5'\n").unwrap()
    }

    #[test]
    fn writes_segments_redirect_and_removes_signature() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in");
        let output = dir.path().join("out");
        std::fs::write(&input, fixture(0x400, 0xd503201f)).unwrap();
        let m = manifest();
        crate::manifest::validate(&m, false).unwrap();
        let report = patch(&m, &input, &output).unwrap();
        assert_eq!(report.patches, 1);
        assert_eq!(report.removed_code_signatures, 1);
        let out = std::fs::read(&output).unwrap();
        let p = parse(&out).unwrap();
        assert!(p.segments.iter().any(|s| s.name == "__IAOTCFG"));
        assert!(p.segments.iter().any(|s| s.name == "__IAOTCAVE"));
        assert!(!p.commands.iter().any(|x| x.0 == LC_CODE_SIGNATURE));
        let cfg = p.segments.iter().find(|s| s.name == "__IAOTCFG").unwrap();
        assert_eq!(
            &out[cfg.fileoff as usize..cfg.fileoff as usize + 8],
            b"IAOTCFG1"
        );
        assert_ne!(u32at(&out, 0x400).unwrap(), 0xd503201f);
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
    fn refuses_pc_relative_displacement() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in");
        let output = dir.path().join("out");
        std::fs::write(&input, fixture(0x400, 0x14000001)).unwrap();
        let mut m = manifest();
        m.patches[0].expect = Some("01 00 00 14".into());
        assert!(patch(&m, &input, &output)
            .unwrap_err()
            .to_string()
            .contains("PC-relative"));
    }
    #[test]
    fn resolves_bytes_and_adds_dylib_command() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in");
        let output = dir.path().join("out");
        std::fs::write(&input, fixture(0x400, 0xd503201f)).unwrap();
        let mut m = manifest();
        m.patches[0].expect = Some("auto".into());
        let report =
            patch_with_options(&m, &input, &output, PatchOptions { load_dylib: true }).unwrap();
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
}
