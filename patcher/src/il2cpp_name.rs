//! Build-time managed-name -> native address resolution, from the IPA alone.
//!
//! A `managed` locator names its target the way the game's own source does
//! (`Namespace.Class::Method(System.Int32)`) and this module turns that name
//! into a vmaddr in the module being patched. Nothing is read from a dump: the
//! only inputs are the Mach-O under the patcher's hands and the
//! `global-metadata.dat` that ships beside it in the same IPA.
//!
//! # How a name becomes an address
//!
//! 1. `global-metadata.dat` gives the managed side: assemblies (images), type
//!    definitions, methods, parameters, and the string blob all names live in.
//!    A method definition carries its MethodDef token.
//! 2. The binary gives the native side. `Il2CppCodeGenModule` holds one
//!    `methodPointers` array per assembly, indexed by `token & 0xFFFFFF` minus
//!    one, so the token from step 1 indexes straight into it.
//! 3. Those arrays hold chained-fixup rebase targets rather than literal
//!    addresses, so every pointer read here goes through [`Binary::rebases`].
//!
//! Generic methods have no entry in `methodPointers`; their native copies are
//! reached through `methodSpecs` -> `genericMethodTable` ->
//! `genericMethodPointers`, which is what [`Instantiation`] selects between.
//!
//! # Self-checks before any address is trusted
//!
//! The layout below was derived from a shipping v39 metadata file, so the
//! resolver proves it before using it, and fails loudly rather than returning a
//! plausible wrong address:
//!
//! * the method table partitions exactly across type definitions, with every
//!   method's `declaringType` pointing back at its owner;
//! * each `Il2CppCodeGenModule.methodPointerCount` equals the method count the
//!   metadata reports for that assembly;
//! * `types[idx]` round-trips: the `Il2CppType` at a type definition's
//!   `byvalTypeIndex` names that same type definition;
//! * `Il2CppMetadataRegistration`'s `fieldOffsets` and `typeDefinitionsSizes`
//!   counts both equal the type-definition count.
//!
//! Generic tables are validated separately and degrade on their own: when they
//! do not check out, non-generic names still resolve and a generic name reports
//! why it could not.

use std::collections::{BTreeMap, HashMap};

/// IL2CPP global-metadata sanity value.
const SANITY: u32 = 0xFAB1_1BAF;
/// The one metadata version whose layout this module has been proven against.
const SUPPORTED_VERSION: i32 = 39;
/// `(offset, size, count)` triplets in the v39 header.
const HEADER_SECTIONS: usize = 31;

// v39 section indices. The unused ones are kept because they document the
// layout this module was derived from, which is what a port to another
// metadata version starts from.
const SEC_STRINGS: usize = 2;
#[allow(dead_code)]
const SEC_PROPERTIES: usize = 4;
const SEC_METHODS: usize = 5;
const SEC_PARAMETERS: usize = 10;
#[allow(dead_code)]
const SEC_FIELDS: usize = 11;
const SEC_NESTED_TYPES: usize = 15;
const SEC_TYPE_DEFINITIONS: usize = 19;
const SEC_IMAGES: usize = 20;

// v39 record strides.
const METHOD_STRIDE: usize = 32;
const TYPE_STRIDE: usize = 82;
const PARAM_STRIDE: usize = 12;
const IMAGE_STRIDE: usize = 36;

// Il2CppType `type` byte values that matter here (ECMA-335 element types).
const TYPE_VOID: u8 = 0x01;
const TYPE_PTR: u8 = 0x0F;
const TYPE_VALUETYPE: u8 = 0x11;
const TYPE_CLASS: u8 = 0x12;
const TYPE_VAR: u8 = 0x13;
const TYPE_ARRAY: u8 = 0x14;
const TYPE_GENERICINST: u8 = 0x15;
const TYPE_SZARRAY: u8 = 0x1D;
const TYPE_MVAR: u8 = 0x1E;

/// Which native copy of a generic method to hook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instantiation {
    /// The copy every reference-type argument runs: IL2CPP shares one body for
    /// them, so this is also the only copy a fully generic method has.
    Shared,
    /// Any single copy, when exactly one exists.
    Any,
    /// One named instantiation, e.g. `[System.Int32]`.
    Args(Vec<String>),
}

/// A parsed `managed:` string plus its per-hook options.
#[derive(Clone, Debug)]
pub struct ManagedQuery {
    /// Full managed type name, nested types separated by `/`.
    pub class: String,
    /// Method name; `.ctor` for a constructor.
    pub method: String,
    /// `None` when the name carried no parameter list.
    pub params: Option<Vec<ParamPattern>>,
    pub assembly: Option<String>,
    pub instantiation: Option<Instantiation>,
    pub offset: i64,
    pub follow_entry_stub: bool,
}

/// One position in a parameter list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParamPattern {
    /// `*`: matches whatever is in that position.
    Any,
    /// A type name, matched full (`System.Int32`) or simple (`Int32`).
    Named(String),
}

/// What the resolver found, and how.
#[derive(Clone, Debug)]
pub struct Resolution {
    pub vmaddr: u64,
    /// Human-readable provenance for the build log and the preflight report.
    pub how: String,
    /// Cross-check and degradation notes worth keeping in the report.
    pub notes: Vec<String>,
    /// Set when the entry stub was followed: the address before the hop.
    pub stub_from: Option<u64>,
    /// The instantiation that was hooked, for a generic method.
    pub instantiation: Option<String>,
}

impl ManagedQuery {
    /// Parses `Namespace.Class::Method` with an optional parameter list.
    ///
    /// Nested types accept `/` or `+`; generic classes keep their backtick
    /// arity (``List`1``). Parameters may be full names, simple names, `*`, and
    /// may carry `[]`, `*` and `&` suffixes.
    pub fn parse(spec: &str) -> anyhow::Result<Self> {
        let spec = spec.trim();
        let (head, tail) = spec
            .split_once("::")
            .ok_or_else(|| anyhow::anyhow!("managed name needs Class::Method, got '{spec}'"))?;
        let class = head.trim().replace('+', "/");
        if class.is_empty() {
            anyhow::bail!("managed name has an empty class: '{spec}'")
        }
        let tail = tail.trim();
        let (method, params) = match tail.split_once('(') {
            Some((name, rest)) => {
                let rest = rest.trim_end();
                let inner = rest
                    .strip_suffix(')')
                    .ok_or_else(|| anyhow::anyhow!("managed name has an unclosed '(': '{spec}'"))?
                    .trim();
                let list = if inner.is_empty() {
                    Vec::new()
                } else {
                    inner
                        .split(',')
                        .map(|part| {
                            let part = part.trim();
                            if part == "*" {
                                ParamPattern::Any
                            } else {
                                ParamPattern::Named(part.replace('+', "/"))
                            }
                        })
                        .collect()
                };
                (name.trim(), Some(list))
            }
            None => (tail, None),
        };
        if method.is_empty() {
            anyhow::bail!("managed name has an empty method: '{spec}'")
        }
        if params.as_ref().is_some_and(|list| {
            list.iter()
                .any(|p| matches!(p, ParamPattern::Named(n) if n.is_empty()))
        }) {
            anyhow::bail!("managed name has an empty parameter type: '{spec}'")
        }
        Ok(Self {
            class,
            method: method.to_string(),
            params,
            assembly: None,
            instantiation: None,
            offset: 0,
            follow_entry_stub: true,
        })
    }

    /// `Class::Method(...)`, for messages.
    pub fn display(&self) -> String {
        match &self.params {
            Some(list) => {
                let rendered = list
                    .iter()
                    .map(|p| match p {
                        ParamPattern::Any => "*".to_string(),
                        ParamPattern::Named(name) => name.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}::{}({rendered})", self.class, self.method)
            }
            None => format!("{}::{}", self.class, self.method),
        }
    }
}

// ---------------------------------------------------------------------------
// Mach-O view: segments, sections, and chained-fixup rebases
// ---------------------------------------------------------------------------

struct Seg {
    vmaddr: u64,
    vmsize: u64,
    fileoff: u64,
    filesize: u64,
}

struct Sect {
    segment: String,
    name: String,
    addr: u64,
    size: u64,
    fileoff: u64,
}

/// The parts of the module the resolver reads.
struct Binary<'a> {
    data: &'a [u8],
    segments: Vec<Seg>,
    sections: Vec<Sect>,
    /// vmaddr of a pointer slot -> the address it rebases to.
    rebases: HashMap<u64, u64>,
    /// Target address -> the pointer slots that hold it.
    pointers_to: HashMap<u64, Vec<u64>>,
}

fn u16at(d: &[u8], o: usize) -> anyhow::Result<u16> {
    Ok(u16::from_le_bytes(
        d.get(o..o + 2)
            .ok_or_else(|| anyhow::anyhow!("read past end of file"))?
            .try_into()
            .unwrap(),
    ))
}
fn u32at(d: &[u8], o: usize) -> anyhow::Result<u32> {
    Ok(u32::from_le_bytes(
        d.get(o..o + 4)
            .ok_or_else(|| anyhow::anyhow!("read past end of file"))?
            .try_into()
            .unwrap(),
    ))
}
fn u64at(d: &[u8], o: usize) -> anyhow::Result<u64> {
    Ok(u64::from_le_bytes(
        d.get(o..o + 8)
            .ok_or_else(|| anyhow::anyhow!("read past end of file"))?
            .try_into()
            .unwrap(),
    ))
}
fn i32at(d: &[u8], o: usize) -> anyhow::Result<i32> {
    Ok(u32at(d, o)? as i32)
}

impl<'a> Binary<'a> {
    fn parse(data: &'a [u8]) -> anyhow::Result<Self> {
        if u32at(data, 0)? != 0xFEED_FACF {
            anyhow::bail!("managed locator needs a 64-bit Mach-O")
        }
        let ncmds = u32at(data, 16)?;
        if ncmds > 65536 {
            anyhow::bail!("malformed Mach-O load commands")
        }
        let mut segments = Vec::new();
        let mut sections = Vec::new();
        let mut chained = None;
        let mut off = 32usize;
        for _ in 0..ncmds {
            let cmd = u32at(data, off)?;
            let size = u32at(data, off + 4)? as usize;
            if size < 8 {
                anyhow::bail!("invalid load command size")
            }
            match cmd {
                0x19 => {
                    // LC_SEGMENT_64
                    let vmaddr = u64at(data, off + 24)?;
                    let vmsize = u64at(data, off + 32)?;
                    let fileoff = u64at(data, off + 40)?;
                    let filesize = u64at(data, off + 48)?;
                    let nsects = u32at(data, off + 64)? as usize;
                    for i in 0..nsects {
                        let s = off + 72 + i * 80;
                        sections.push(Sect {
                            name: cstr16(data, s),
                            segment: cstr16(data, s + 16),
                            addr: u64at(data, s + 32)?,
                            size: u64at(data, s + 40)?,
                            fileoff: u32at(data, s + 48)? as u64,
                        });
                    }
                    segments.push(Seg {
                        vmaddr,
                        vmsize,
                        fileoff,
                        filesize,
                    });
                }
                0x8000_0034 => {
                    // LC_DYLD_CHAINED_FIXUPS
                    chained = Some((
                        u32at(data, off + 8)? as usize,
                        u32at(data, off + 12)? as usize,
                    ));
                }
                _ => {}
            }
            off += size;
        }
        let mut binary = Self {
            data,
            segments,
            sections,
            rebases: HashMap::new(),
            pointers_to: HashMap::new(),
        };
        let Some((fixups_off, _)) = chained else {
            anyhow::bail!(
                "module has no LC_DYLD_CHAINED_FIXUPS; the managed locator reads IL2CPP tables \
                 through chained-fixup rebases and cannot read this layout"
            )
        };
        binary.read_rebases(fixups_off)?;
        for (slot, target) in &binary.rebases {
            binary.pointers_to.entry(*target).or_default().push(*slot);
        }
        Ok(binary)
    }

    /// Walks every rebase chain so IL2CPP's pointer tables can be read.
    fn read_rebases(&mut self, base: usize) -> anyhow::Result<()> {
        let data = self.data;
        let starts_off = u32at(data, base + 4)? as usize;
        let starts = base + starts_off;
        let seg_count = u32at(data, starts)? as usize;
        if seg_count > 256 {
            anyhow::bail!("implausible chained-fixups segment count {seg_count}")
        }
        let mut rebases = HashMap::new();
        for i in 0..seg_count {
            let info_off = u32at(data, starts + 4 + i * 4)? as usize;
            if info_off == 0 {
                continue;
            }
            let seg = starts + info_off;
            let page_size = u16at(data, seg + 4)? as usize;
            let pointer_format = u16at(data, seg + 6)?;
            let segment_offset = u64at(data, seg + 8)? as usize;
            let page_count = u16at(data, seg + 20)? as usize;
            if !matches!(pointer_format, 2 | 6 | 9 | 12) {
                anyhow::bail!(
                    "unsupported chained-fixups pointer format {pointer_format}; use signatures \
                     for this app"
                )
            }
            if page_size == 0 {
                anyhow::bail!("chained-fixups page size is zero")
            }
            for page in 0..page_count {
                let start = u16at(data, seg + 22 + page * 2)?;
                if start == 0xFFFF {
                    continue;
                }
                let mut at = segment_offset + page * page_size + start as usize;
                loop {
                    let raw = u64at(data, at)?;
                    let (bind, next, stride) = match pointer_format {
                        2 | 6 => ((raw >> 63) & 1, (raw >> 51) & 0xFFF, 4usize),
                        _ => ((raw >> 62) & 1, (raw >> 51) & 0x7FF, 8usize),
                    };
                    if bind == 0 {
                        let auth = pointer_format >= 9 && (raw >> 63) & 1 == 1;
                        let target = if auth {
                            raw & 0xFFFF_FFFF
                        } else {
                            (raw & 0xF_FFFF_FFFF) | (((raw >> 36) & 0xFF) << 56)
                        };
                        if let Some(vm) = self.file_to_vm(at) {
                            rebases.insert(vm, target);
                        }
                    }
                    if next == 0 {
                        break;
                    }
                    at += next as usize * stride;
                }
            }
        }
        self.rebases = rebases;
        Ok(())
    }

    fn file_to_vm(&self, offset: usize) -> Option<u64> {
        let offset = offset as u64;
        self.segments
            .iter()
            .find(|s| s.filesize > 0 && offset >= s.fileoff && offset < s.fileoff + s.filesize)
            .map(|s| s.vmaddr + (offset - s.fileoff))
    }

    fn vm_to_file(&self, address: u64) -> Option<usize> {
        self.segments
            .iter()
            .find(|s| {
                s.filesize > 0
                    && address >= s.vmaddr
                    && address < s.vmaddr + s.vmsize
                    && (address - s.vmaddr) < s.filesize
            })
            .map(|s| (s.fileoff + (address - s.vmaddr)) as usize)
    }

    fn section_of(&self, address: u64) -> Option<&Sect> {
        self.sections
            .iter()
            .find(|s| address >= s.addr && address < s.addr + s.size)
    }

    fn u32_at_vm(&self, address: u64) -> Option<u32> {
        self.vm_to_file(address)
            .and_then(|o| u32at(self.data, o).ok())
    }

    fn u64_at_vm(&self, address: u64) -> Option<u64> {
        self.vm_to_file(address)
            .and_then(|o| u64at(self.data, o).ok())
    }

    /// A pointer field read through the rebase table.
    fn ptr_at_vm(&self, address: u64) -> Option<u64> {
        self.rebases.get(&address).copied()
    }

    /// Finds a NUL-terminated string in `__cstring` and returns its address.
    fn cstring_address(&self, text: &str) -> Option<u64> {
        let mut needle = Vec::with_capacity(text.len() + 2);
        needle.push(0u8);
        needle.extend_from_slice(text.as_bytes());
        needle.push(0u8);
        for section in self.sections.iter().filter(|s| s.name == "__cstring") {
            let start = section.fileoff as usize;
            let end = start + section.size as usize;
            let slice = self.data.get(start..end)?;
            if let Some(at) = find_bytes(slice, &needle) {
                return Some(section.addr + (at + 1) as u64);
            }
        }
        None
    }

    /// Every `(offset, section)` where an 8-byte little-endian `value` appears
    /// in initialized data, used to find IL2CPP's own tables structurally.
    fn scan_u64_in_data(&self, value: u64) -> Vec<u64> {
        let needle = value.to_le_bytes();
        let mut hits = Vec::new();
        for section in self
            .sections
            .iter()
            .filter(|s| (s.segment == "__DATA" || s.segment == "__DATA_CONST") && s.fileoff > 0)
        {
            let start = section.fileoff as usize;
            let end = start + section.size as usize;
            let Some(slice) = self.data.get(start..end) else {
                continue;
            };
            let mut at = 0usize;
            while let Some(found) = find_bytes(&slice[at..], &needle) {
                let absolute = at + found;
                if absolute.is_multiple_of(8) {
                    hits.push(section.addr + absolute as u64);
                }
                at = absolute + 8;
                if at >= slice.len() {
                    break;
                }
            }
        }
        hits
    }
}

fn cstr16(data: &[u8], at: usize) -> String {
    let bytes = data.get(at..at + 16).unwrap_or_default();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// ---------------------------------------------------------------------------
// global-metadata.dat
// ---------------------------------------------------------------------------

struct MethodDef {
    name_index: u32,
    declaring_type: u16,
    param_start: i32,
    generic_container: i16,
    token: u32,
    param_count: u16,
    flags: u16,
}

struct TypeDef {
    name_index: u32,
    namespace_index: u32,
    byval_type: i32,
    method_start: i32,
    method_count: u16,
    nested_start: i32,
    nested_count: u16,
    generic_container: i16,
}

struct ImageDef {
    name_index: u32,
    type_start: u16,
    type_count: u16,
}

/// A read-only view over the metadata file's tables.
struct Metadata<'a> {
    data: &'a [u8],
    sections: Vec<(usize, usize, usize)>,
}

impl<'a> Metadata<'a> {
    fn parse(data: &'a [u8]) -> anyhow::Result<Self> {
        let sanity = u32at(data, 0)?;
        if sanity != SANITY {
            anyhow::bail!(
                "global-metadata.dat does not start with the IL2CPP sanity value \
                 (found 0x{sanity:08X}); it may be encrypted or transformed, so use signatures \
                 for this app"
            )
        }
        let version = i32at(data, 4)?;
        if version != SUPPORTED_VERSION {
            anyhow::bail!(
                "global-metadata.dat is version {version}; the managed locator has only been \
                 proven against version {SUPPORTED_VERSION}. Use signatures for this app, or \
                 extend il2cpp_name.rs with this version's layout."
            )
        }
        let mut sections = Vec::with_capacity(HEADER_SECTIONS);
        for i in 0..HEADER_SECTIONS {
            let at = 8 + i * 12;
            let offset = i32at(data, at)?;
            let size = i32at(data, at + 4)?;
            let count = i32at(data, at + 8)?;
            if offset < 0 || size < 0 || count < 0 {
                anyhow::bail!("metadata section {i} has a negative offset, size or count")
            }
            let (offset, size) = (offset as usize, size as usize);
            if offset.saturating_add(size) > data.len() {
                anyhow::bail!("metadata section {i} runs past the end of the file")
            }
            sections.push((offset, size, count as usize));
        }
        let header_size = 8 + HEADER_SECTIONS * 12;
        if sections[0].0 != header_size {
            anyhow::bail!(
                "metadata header is {} bytes but the first section starts at {}; this is not the \
                 v39 layout",
                header_size,
                sections[0].0
            )
        }
        Ok(Self { data, sections })
    }

    fn section(&self, index: usize) -> (usize, usize, usize) {
        self.sections[index]
    }

    fn count(&self, index: usize, stride: usize) -> usize {
        self.sections[index].1 / stride
    }

    /// A name from the string blob, by byte offset into it.
    fn string(&self, index: u32) -> Option<&'a str> {
        let (offset, size, _) = self.section(SEC_STRINGS);
        let index = index as usize;
        if index >= size {
            return None;
        }
        let start = offset + index;
        let end = self.data[start..offset + size]
            .iter()
            .position(|&b| b == 0)?
            + start;
        std::str::from_utf8(&self.data[start..end]).ok()
    }

    fn method(&self, index: usize) -> anyhow::Result<MethodDef> {
        let (offset, _, _) = self.section(SEC_METHODS);
        let at = offset + index * METHOD_STRIDE;
        Ok(MethodDef {
            name_index: u32at(self.data, at)?,
            declaring_type: u16at(self.data, at + 4)?,
            param_start: i32at(self.data, at + 14)?,
            generic_container: u16at(self.data, at + 18)? as i16,
            token: u32at(self.data, at + 20)?,
            flags: u16at(self.data, at + 24)?,
            param_count: u16at(self.data, at + 30)?,
        })
    }

    fn type_def(&self, index: usize) -> anyhow::Result<TypeDef> {
        let (offset, _, _) = self.section(SEC_TYPE_DEFINITIONS);
        let at = offset + index * TYPE_STRIDE;
        Ok(TypeDef {
            name_index: u32at(self.data, at)?,
            namespace_index: u32at(self.data, at + 4)?,
            byval_type: i32at(self.data, at + 8)?,
            generic_container: u16at(self.data, at + 20)? as i16,
            method_start: i32at(self.data, at + 30)?,
            nested_start: i32at(self.data, at + 42)?,
            method_count: u16at(self.data, at + 58)?,
            nested_count: u16at(self.data, at + 66)?,
        })
    }

    fn image(&self, index: usize) -> anyhow::Result<ImageDef> {
        let (offset, _, _) = self.section(SEC_IMAGES);
        let at = offset + index * IMAGE_STRIDE;
        Ok(ImageDef {
            name_index: u32at(self.data, at)?,
            type_start: u16at(self.data, at + 8)?,
            type_count: u16at(self.data, at + 10)?,
        })
    }

    /// `(name_index, token, type_index)` for one parameter.
    fn param_type(&self, index: usize) -> anyhow::Result<u32> {
        let (offset, _, _) = self.section(SEC_PARAMETERS);
        u32at(self.data, offset + index * PARAM_STRIDE + 8)
    }

    fn nested_type(&self, index: usize) -> anyhow::Result<i32> {
        let (offset, _, _) = self.section(SEC_NESTED_TYPES);
        i32at(self.data, offset + index * 4)
    }
}

// ---------------------------------------------------------------------------
// The index
// ---------------------------------------------------------------------------

struct Module {
    method_count: u64,
    method_pointers: u64,
}

struct GenericTables {
    method_specs: u64,
    method_spec_count: usize,
    generic_method_pointers: u64,
    generic_method_pointer_count: usize,
    /// methodSpec index -> index into `generic_method_pointers`.
    spec_to_pointer: HashMap<u32, i32>,
    /// method definition index -> its methodSpec indices.
    specs_by_definition: HashMap<u32, Vec<u32>>,
    generic_insts: u64,
    generic_inst_count: usize,
}

/// Everything needed to turn a managed name into an address in one module.
pub struct ManagedIndex<'a> {
    binary: Binary<'a>,
    meta: Metadata<'a>,
    type_count: usize,
    method_count: usize,
    /// Full managed type name -> type definition indices.
    types_by_name: HashMap<String, Vec<usize>>,
    /// Type definition index -> owning image index.
    image_of_type: Vec<u32>,
    /// Nested type definition index -> its declaring type.
    parent_of_type: HashMap<usize, usize>,
    images: Vec<String>,
    modules: HashMap<String, Module>,
    /// `Il2CppMetadataRegistration.types`, for parameter and argument names.
    types_table: Option<u64>,
    generics: Option<GenericTables>,
    /// Native address -> the managed methods that start there.
    methods_at: HashMap<u64, Vec<u32>>,
    /// Notes about what could not be set up, surfaced on the hooks that need it.
    degraded: Vec<String>,
}

impl<'a> ManagedIndex<'a> {
    /// Builds the index, proving the metadata layout before returning.
    pub fn build(module: &'a [u8], metadata: &'a [u8]) -> anyhow::Result<Self> {
        let binary = Binary::parse(module)?;
        let meta = Metadata::parse(metadata)?;
        let type_count = meta.count(SEC_TYPE_DEFINITIONS, TYPE_STRIDE);
        let method_count = meta.count(SEC_METHODS, METHOD_STRIDE);
        if type_count == 0 || method_count == 0 {
            anyhow::bail!("global-metadata.dat has no type or method definitions")
        }
        let mut index = Self {
            binary,
            meta,
            type_count,
            method_count,
            types_by_name: HashMap::new(),
            image_of_type: vec![u32::MAX; type_count],
            parent_of_type: HashMap::new(),
            images: Vec::new(),
            modules: HashMap::new(),
            types_table: None,
            generics: None,
            methods_at: HashMap::new(),
            degraded: Vec::new(),
        };
        index.verify_metadata_layout()?;
        index.build_type_names()?;
        index.build_modules()?;
        index.find_types_table();
        index.find_generic_tables();
        index.build_address_map()?;
        Ok(index)
    }

    /// The method table must partition exactly across type definitions, and
    /// every method must point back at its owner. A layout that is off by even
    /// one field fails this.
    fn verify_metadata_layout(&self) -> anyhow::Result<()> {
        let mut ranges: Vec<(i32, u16, usize)> = Vec::new();
        for index in 0..self.type_count {
            let t = self.meta.type_def(index)?;
            if t.method_count > 0 {
                if t.method_start < 0
                    || t.method_start as usize + t.method_count as usize > self.method_count
                {
                    anyhow::bail!(
                        "type definition {index} claims methods {}..{} of {}; the metadata layout \
                         does not match version {SUPPORTED_VERSION}",
                        t.method_start,
                        t.method_start as i64 + t.method_count as i64,
                        self.method_count
                    )
                }
                ranges.push((t.method_start, t.method_count, index));
            }
        }
        ranges.sort_unstable();
        let mut covered = 0usize;
        let mut end = 0i64;
        for (start, count, owner) in &ranges {
            if i64::from(*start) < end {
                anyhow::bail!(
                    "type definitions {owner} and an earlier type claim overlapping method ranges; \
                     the metadata layout does not match version {SUPPORTED_VERSION}"
                )
            }
            end = i64::from(*start) + i64::from(*count);
            covered += *count as usize;
        }
        if covered != self.method_count {
            anyhow::bail!(
                "type definitions cover {covered} of {} methods; the metadata layout does not \
                 match version {SUPPORTED_VERSION}",
                self.method_count
            )
        }
        // Spot-check the back-pointer on a spread of types rather than all of
        // them: a wrong layout fails this immediately.
        let step = (self.type_count / 512).max(1);
        for index in (0..self.type_count).step_by(step) {
            let t = self.meta.type_def(index)?;
            for k in 0..t.method_count as usize {
                let m = self.meta.method(t.method_start as usize + k)?;
                if m.declaring_type as usize != index {
                    anyhow::bail!(
                        "method {} says it belongs to type {} but type {index} lists it; the \
                         metadata layout does not match version {SUPPORTED_VERSION}",
                        t.method_start as usize + k,
                        m.declaring_type
                    )
                }
                if m.token >> 24 != 0x06 {
                    anyhow::bail!(
                        "method {} has token 0x{:08X}, which is not a MethodDef token; the \
                         metadata layout does not match version {SUPPORTED_VERSION}",
                        t.method_start as usize + k,
                        m.token
                    )
                }
            }
        }
        Ok(())
    }

    fn build_type_names(&mut self) -> anyhow::Result<()> {
        let image_count = self.meta.count(SEC_IMAGES, IMAGE_STRIDE);
        for index in 0..image_count {
            let image = self.meta.image(index)?;
            let name = self
                .meta
                .string(image.name_index)
                .ok_or_else(|| anyhow::anyhow!("image {index} has an unreadable name"))?
                .to_string();
            for t in image.type_start as usize
                ..(image.type_start as usize + image.type_count as usize).min(self.type_count)
            {
                self.image_of_type[t] = index as u32;
            }
            self.images.push(name);
        }
        for index in 0..self.type_count {
            let t = self.meta.type_def(index)?;
            for k in 0..t.nested_count as usize {
                let child = self.meta.nested_type(t.nested_start as usize + k)?;
                if child >= 0 && (child as usize) < self.type_count {
                    self.parent_of_type.insert(child as usize, index);
                }
            }
        }
        for index in 0..self.type_count {
            let name = self.full_type_name(index)?;
            self.types_by_name.entry(name).or_default().push(index);
        }
        Ok(())
    }

    /// `Namespace.Outer/Inner`, built from the nested-type table.
    fn full_type_name(&self, index: usize) -> anyhow::Result<String> {
        let mut parts = vec![self
            .meta
            .string(self.meta.type_def(index)?.name_index)
            .unwrap_or("?")
            .to_string()];
        let mut current = index;
        let mut guard = 0;
        while let Some(parent) = self.parent_of_type.get(&current) {
            current = *parent;
            parts.push(
                self.meta
                    .string(self.meta.type_def(current)?.name_index)
                    .unwrap_or("?")
                    .to_string(),
            );
            guard += 1;
            if guard > 32 {
                anyhow::bail!("nested type chain for type {index} does not terminate")
            }
        }
        parts.reverse();
        let nested = parts.join("/");
        let namespace = self
            .meta
            .string(self.meta.type_def(current)?.namespace_index)
            .unwrap_or("");
        Ok(if namespace.is_empty() {
            nested
        } else {
            format!("{namespace}.{nested}")
        })
    }

    /// Finds each assembly's `Il2CppCodeGenModule` and checks its method count
    /// against the metadata before accepting it.
    fn build_modules(&mut self) -> anyhow::Result<()> {
        let mut expected: HashMap<String, u64> = HashMap::new();
        let image_count = self.meta.count(SEC_IMAGES, IMAGE_STRIDE);
        for index in 0..image_count {
            let image = self.meta.image(index)?;
            let mut total = 0u64;
            for t in image.type_start as usize
                ..(image.type_start as usize + image.type_count as usize).min(self.type_count)
            {
                total += u64::from(self.meta.type_def(t)?.method_count);
            }
            let name = self.images[index].clone();
            expected.insert(name, total);
        }
        let mut matched = 0usize;
        let mut expected_modules = 0usize;
        for (name, want) in &expected {
            if *want == 0 {
                continue;
            }
            expected_modules += 1;
            let Some(string_vm) = self.binary.cstring_address(name) else {
                continue;
            };
            let Some(slots) = self.binary.pointers_to.get(&string_vm) else {
                continue;
            };
            for slot in slots {
                // Il2CppCodeGenModule: moduleName, methodPointerCount, methodPointers.
                let Some(count) = self.binary.u64_at_vm(slot + 8) else {
                    continue;
                };
                let Some(pointers) = self.binary.ptr_at_vm(slot + 16) else {
                    continue;
                };
                if count != *want {
                    continue;
                }
                self.modules.insert(
                    name.clone(),
                    Module {
                        method_count: count,
                        method_pointers: pointers,
                    },
                );
                matched += 1;
                break;
            }
        }
        if matched == 0 {
            anyhow::bail!(
                "no Il2CppCodeGenModule in this module matched the metadata's method counts, so \
                 managed names cannot be resolved here; use signatures for this app"
            )
        }
        if matched < expected_modules {
            self.degraded.push(format!(
                "{} of {expected_modules} assemblies matched an Il2CppCodeGenModule; names in the \
                 rest cannot be resolved",
                matched
            ));
        }
        Ok(())
    }

    /// Locates `Il2CppMetadataRegistration.types` by looking for the array that
    /// agrees with every type definition's `byvalTypeIndex`.
    fn find_types_table(&mut self) {
        for index in 0..self.type_count.min(96) {
            let Ok(t) = self.meta.type_def(index) else {
                continue;
            };
            if t.byval_type < 0 {
                continue;
            }
            for candidate_type in self.binary.scan_u64_in_data(index as u64) {
                let Some(bits) = self.binary.u32_at_vm(candidate_type + 8) else {
                    continue;
                };
                let kind = ((bits >> 16) & 0xFF) as u8;
                if !matches!(kind, TYPE_CLASS | TYPE_VALUETYPE) || bits & 0xFFFF != 0 {
                    continue;
                }
                let Some(slots) = self.binary.pointers_to.get(&candidate_type) else {
                    continue;
                };
                for slot in slots {
                    let base = slot.wrapping_sub(t.byval_type as u64 * 8);
                    if self.score_types_table(base) >= 380 {
                        self.types_table = Some(base);
                        return;
                    }
                }
            }
        }
        self.degraded.push(
            "could not locate Il2CppMetadataRegistration.types, so parameter types and generic \
             arguments cannot be named; name a method by parameter count or use signatures"
                .into(),
        );
    }

    /// How many of 400 sampled type definitions round-trip through a candidate
    /// `types` array.
    fn score_types_table(&self, base: u64) -> usize {
        let step = (self.type_count / 400).max(1);
        let mut ok = 0usize;
        for index in (0..self.type_count).step_by(step).take(400) {
            let Ok(t) = self.meta.type_def(index) else {
                continue;
            };
            if t.byval_type < 0 {
                continue;
            }
            let Some(entry) = self.binary.ptr_at_vm(base + t.byval_type as u64 * 8) else {
                continue;
            };
            let Some(data) = self.binary.u64_at_vm(entry) else {
                continue;
            };
            let Some(bits) = self.binary.u32_at_vm(entry + 8) else {
                continue;
            };
            let kind = ((bits >> 16) & 0xFF) as u8;
            if data == index as u64 && matches!(kind, TYPE_CLASS | TYPE_VALUETYPE) {
                ok += 1;
            }
        }
        ok
    }

    /// Reads the generic tables around the already-located `types` array, and
    /// validates them before use. Failure here only costs generic names.
    fn find_generic_tables(&mut self) {
        let Some(types_table) = self.types_table else {
            return;
        };
        let Some(slots) = self.binary.pointers_to.get(&types_table) else {
            self.degraded
                .push("nothing points at the types array, so generic tables were not read".into());
            return;
        };
        for field in slots.clone() {
            // Il2CppMetadataRegistration, around the `types` field:
            //   -40 genericInstsCount  -32 genericInsts
            //   -24 genericMethodTableCount  -16 genericMethodTable
            //    -8 typesCount          +0 types
            //    +8 methodSpecsCount   +16 methodSpecs
            //   +24 fieldOffsetsCount  +32 fieldOffsets
            //   +40 typeDefinitionsSizesCount +48 typeDefinitionsSizes
            let field_offsets_count = self.binary.u64_at_vm(field + 24);
            let sizes_count = self.binary.u64_at_vm(field + 40);
            if field_offsets_count != Some(self.type_count as u64)
                || sizes_count != Some(self.type_count as u64)
            {
                continue;
            }
            let Some(method_spec_count) = self.binary.u64_at_vm(field + 8) else {
                continue;
            };
            let Some(method_specs) = self.binary.ptr_at_vm(field + 16) else {
                continue;
            };
            let Some(generic_method_table_count) = self.binary.u64_at_vm(field.wrapping_sub(24))
            else {
                continue;
            };
            let Some(generic_method_table) = self.binary.ptr_at_vm(field.wrapping_sub(16)) else {
                continue;
            };
            let generic_inst_count = self
                .binary
                .u64_at_vm(field.wrapping_sub(40))
                .unwrap_or_default();
            let generic_insts = self
                .binary
                .ptr_at_vm(field.wrapping_sub(32))
                .unwrap_or_default();
            let Some((generic_method_pointers, generic_method_pointer_count)) =
                self.find_generic_method_pointers()
            else {
                self.degraded.push(
                    "could not locate Il2CppCodeRegistration.genericMethodPointers, so generic \
                     instantiations cannot be hooked by name"
                        .into(),
                );
                return;
            };
            // Walk genericMethodTable and keep it only if every entry is in range.
            let mut spec_to_pointer = HashMap::new();
            let mut sane = true;
            for i in 0..generic_method_table_count as usize {
                let at = generic_method_table + (i * 16) as u64;
                let Some(spec) = self.binary.u32_at_vm(at) else {
                    sane = false;
                    break;
                };
                let Some(pointer_index) = self.binary.u32_at_vm(at + 4) else {
                    sane = false;
                    break;
                };
                let pointer_index = pointer_index as i32;
                if spec as u64 >= method_spec_count
                    || pointer_index < -1
                    || pointer_index as i64 >= generic_method_pointer_count as i64
                {
                    sane = false;
                    break;
                }
                spec_to_pointer.insert(spec, pointer_index);
            }
            if !sane {
                continue;
            }
            let mut specs_by_definition: HashMap<u32, Vec<u32>> = HashMap::new();
            for i in 0..method_spec_count as usize {
                let at = method_specs + (i * 12) as u64;
                let Some(definition) = self.binary.u32_at_vm(at) else {
                    sane = false;
                    break;
                };
                if definition as usize >= self.method_count {
                    sane = false;
                    break;
                }
                specs_by_definition
                    .entry(definition)
                    .or_default()
                    .push(i as u32);
            }
            if !sane {
                continue;
            }
            self.generics = Some(GenericTables {
                method_specs,
                method_spec_count: method_spec_count as usize,
                generic_method_pointers,
                generic_method_pointer_count,
                spec_to_pointer,
                specs_by_definition,
                generic_insts,
                generic_inst_count: generic_inst_count as usize,
            });
            return;
        }
        self.degraded.push(
            "Il2CppMetadataRegistration did not pass its count checks, so generic instantiations \
             cannot be hooked by name"
                .into(),
        );
    }

    /// `Il2CppCodeRegistration` sits in front of the `codeGenModules` array it
    /// owns; `genericMethodPointers` is a fixed distance back from that field.
    fn find_generic_method_pointers(&self) -> Option<(u64, usize)> {
        // Find the codeGenModules array from any module struct we accepted: the
        // struct's address is the moduleName slot, and the array holds pointers
        // to those structs.
        let mut array_slots: Vec<u64> = Vec::new();
        for module in self.modules.values() {
            // The module struct starts at the moduleName field, 16 bytes before
            // the methodPointers field we recorded.
            let Some(slots) = self.binary.pointers_to.get(&module.method_pointers) else {
                continue;
            };
            for slot in slots {
                let struct_vm = slot.wrapping_sub(16);
                if let Some(array) = self.binary.pointers_to.get(&struct_vm) {
                    array_slots.extend(array.iter().copied());
                }
            }
        }
        for slot in array_slots {
            // Walk back to the start of the codeGenModules array.
            let mut base = slot;
            while let Some(previous) = self.binary.ptr_at_vm(base.wrapping_sub(8)) {
                if self.binary.section_of(previous).is_none() {
                    break;
                }
                base -= 8;
            }
            let Some(fields) = self.binary.pointers_to.get(&base) else {
                continue;
            };
            for field in fields {
                let count = self.binary.u64_at_vm(field.wrapping_sub(8))?;
                if count == 0 || count as usize > self.images.len() + 8 {
                    continue;
                }
                // Il2CppCodeRegistration: ... genericMethodPointersCount,
                // genericMethodPointers, genericAdjustorThunks, invokerPointersCount,
                // invokerPointers, ... codeGenModulesCount, codeGenModules.
                let pointer_count = self.binary.u64_at_vm(field.wrapping_sub(112))?;
                let pointers = self.binary.ptr_at_vm(field.wrapping_sub(104))?;
                if pointer_count == 0 || pointer_count > 1 << 24 {
                    continue;
                }
                return Some((pointers, pointer_count as usize));
            }
        }
        None
    }

    /// Native entry address -> managed methods, for overload reporting,
    /// identical-code-folding warnings and the entry-stub test.
    fn build_address_map(&mut self) -> anyhow::Result<()> {
        let mut map: HashMap<u64, Vec<u32>> = HashMap::new();
        for index in 0..self.type_count {
            let image = self.image_of_type[index];
            if image == u32::MAX {
                continue;
            }
            let Some(module) = self.modules.get(&self.images[image as usize]) else {
                continue;
            };
            let t = self.meta.type_def(index)?;
            for k in 0..t.method_count as usize {
                let method_index = t.method_start as usize + k;
                let m = self.meta.method(method_index)?;
                if let Some(address) = method_pointer(&self.binary, module, m.token) {
                    map.entry(address).or_default().push(method_index as u32);
                }
            }
        }
        self.methods_at = map;
        Ok(())
    }

    /// Resolves a query to one address, or explains what to change.
    pub fn resolve(&self, query: &ManagedQuery) -> anyhow::Result<Resolution> {
        let candidates = self
            .types_by_name
            .get(&query.class)
            .ok_or_else(|| self.class_not_found(&query.class))?;
        let mut in_assembly: Vec<usize> = Vec::new();
        for index in candidates {
            let image = self.image_of_type[*index];
            if image == u32::MAX {
                continue;
            }
            let name = &self.images[image as usize];
            if let Some(wanted) = &query.assembly {
                if !assembly_matches(name, wanted) {
                    continue;
                }
            }
            in_assembly.push(*index);
        }
        if in_assembly.is_empty() {
            if query.assembly.is_some() {
                let present = candidates
                    .iter()
                    .filter_map(|i| self.image_of_type.get(*i))
                    .filter(|i| **i != u32::MAX)
                    .map(|i| self.images[*i as usize].clone())
                    .collect::<Vec<_>>();
                anyhow::bail!(
                    "class {} is not in assembly {}; it is in {}",
                    query.class,
                    query.assembly.as_deref().unwrap_or_default(),
                    present.join(", ")
                )
            }
            anyhow::bail!(
                "class {} is in no assembly this module registers",
                query.class
            )
        }
        if in_assembly.len() > 1 {
            let names = in_assembly
                .iter()
                .map(|i| self.images[self.image_of_type[*i] as usize].clone())
                .collect::<Vec<_>>();
            anyhow::bail!(
                "class {} exists in {} assemblies ({}); add `assembly:` to pick one",
                query.class,
                in_assembly.len(),
                names.join(", ")
            )
        }
        let type_index = in_assembly[0];
        let image = &self.images[self.image_of_type[type_index] as usize];
        let module = self
            .modules
            .get(image)
            .ok_or_else(|| anyhow::anyhow!("assembly {image} has no code module in this binary"))?;
        let t = self.meta.type_def(type_index)?;

        let mut named = Vec::new();
        for k in 0..t.method_count as usize {
            let method_index = t.method_start as usize + k;
            let m = self.meta.method(method_index)?;
            if self.meta.string(m.name_index) == Some(query.method.as_str()) {
                named.push((method_index, m));
            }
        }
        if named.is_empty() {
            anyhow::bail!(
                "class {} has no method named {}{}",
                query.class,
                query.method,
                self.nearby_methods(&t, &query.method)?
            )
        }
        let mut matching = Vec::new();
        for (method_index, m) in &named {
            if let Some(patterns) = &query.params {
                if patterns.len() != m.param_count as usize {
                    continue;
                }
                if !self.params_match(m, patterns)? {
                    continue;
                }
            }
            matching.push((*method_index, m));
        }
        if matching.is_empty() {
            anyhow::bail!(
                "no overload of {}::{} matches those parameter types. Overloads present:\n{}",
                query.class,
                query.method,
                self.overload_list(&named)?
            )
        }
        if matching.len() > 1 {
            anyhow::bail!(
                "{} overloads of {}::{} match; name the parameter types. Overloads present:\n{}",
                matching.len(),
                query.class,
                query.method,
                self.overload_list(&named)?
            )
        }
        let (method_index, method) = matching[0];
        let mut notes = Vec::new();
        let is_generic = method.generic_container >= 0 || t.generic_container >= 0;
        let (mut address, instantiation) = if is_generic {
            let (address, label) = self.resolve_generic(method_index, query)?;
            (address, Some(label))
        } else {
            let address = method_pointer(&self.binary, module, method.token)
                .ok_or_else(|| self.no_body_error(&query.display(), method))?;
            (address, None)
        };
        if address == 0 {
            anyhow::bail!(
                "{} resolves to a null method pointer, so it has no native body in this build \
                 (abstract, extern, or stripped); use signatures or hook a caller",
                query.display()
            )
        }
        if let Some(others) = self.methods_at.get(&address) {
            if others.len() > 1 {
                let names = others
                    .iter()
                    .filter(|other| **other as usize != method_index)
                    .filter_map(|other| self.describe_method(*other as usize).ok())
                    .collect::<Vec<_>>();
                notes.push(format!(
                    "identical code folding: this body is shared with {}; hooking it affects all \
                     of them",
                    names.join(", ")
                ));
            }
        }
        let mut stub_from = None;
        if query.follow_entry_stub {
            if let Some(target) = self.entry_stub_target(address) {
                stub_from = Some(address);
                notes.push(format!(
                    "entry stub at 0x{address:x} is a single branch; hooked its body at 0x{target:x}"
                ));
                address = target;
            }
        }
        if query.offset != 0 {
            let moved = address
                .checked_add_signed(query.offset)
                .ok_or_else(|| anyhow::anyhow!("offset moves the target out of range"))?;
            let same_section = match (
                self.binary.section_of(address),
                self.binary.section_of(moved),
            ) {
                (Some(a), Some(b)) => a.addr == b.addr,
                _ => false,
            };
            if !same_section {
                anyhow::bail!(
                    "offset {} moves the target out of the function's section",
                    query.offset
                )
            }
            address = moved;
        }
        for note in &self.degraded {
            notes.push(note.clone());
        }
        let how = format!(
            "managed name {} in {image}{}",
            query.display(),
            instantiation
                .as_ref()
                .map(|i| format!(" <{i}>"))
                .unwrap_or_default()
        );
        Ok(Resolution {
            vmaddr: address,
            how,
            notes,
            stub_from,
            instantiation,
        })
    }

    /// Picks one native copy of a generic method.
    fn resolve_generic(
        &self,
        method_index: usize,
        query: &ManagedQuery,
    ) -> anyhow::Result<(u64, String)> {
        let generics = self.generics.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "{} is generic, and this build's generic tables could not be read, so its native \
                 copies cannot be found by name; use signatures",
                query.display()
            )
        })?;
        let specs = generics
            .specs_by_definition
            .get(&(method_index as u32))
            .map(|v| v.as_slice())
            .unwrap_or_default();
        let mut found: Vec<(String, u64, bool)> = Vec::new();
        for spec in specs {
            let at = generics.method_specs + (*spec as usize * 12) as u64;
            let class_inst = self
                .binary
                .u32_at_vm(at + 4)
                .map(|v| v as i32)
                .unwrap_or(-1);
            let method_inst = self
                .binary
                .u32_at_vm(at + 8)
                .map(|v| v as i32)
                .unwrap_or(-1);
            let mut args = Vec::new();
            args.extend(self.inst_arguments(generics, class_inst));
            args.extend(self.inst_arguments(generics, method_inst));
            let label = args.join(", ");
            let Some(pointer_index) = generics.spec_to_pointer.get(spec) else {
                continue;
            };
            if *pointer_index < 0 {
                continue;
            }
            let Some(address) = self
                .binary
                .ptr_at_vm(generics.generic_method_pointers + (*pointer_index as u64) * 8)
            else {
                continue;
            };
            let shared = args.iter().all(|a| a == "System.Object");
            found.push((label, address, shared));
        }
        if found.is_empty() {
            anyhow::bail!(
                "{} is generic and no instantiation in this build has its own native body; it \
                 runs as shared code reached through a caller. Use signatures for it.",
                query.display()
            )
        }
        let wanted = query.instantiation.clone().unwrap_or(Instantiation::Shared);
        match wanted {
            Instantiation::Shared => {
                if let Some((label, address, _)) = found.iter().find(|(_, _, shared)| *shared) {
                    return Ok((*address, label.clone()));
                }
                if found.len() == 1 {
                    let (label, address, _) = &found[0];
                    return Ok((*address, label.clone()));
                }
                anyhow::bail!(
                    "{} has no fully shared (System.Object) copy; name one instantiation with \
                     `instantiation:`. Instantiations with their own body:\n{}",
                    query.display(),
                    instantiation_list(&found)
                )
            }
            Instantiation::Any => {
                if found.len() == 1 {
                    let (label, address, _) = &found[0];
                    return Ok((*address, label.clone()));
                }
                anyhow::bail!(
                    "{} has {} instantiations with their own body, so `instantiation: any` is \
                     ambiguous:\n{}",
                    query.display(),
                    found.len(),
                    instantiation_list(&found)
                )
            }
            Instantiation::Args(args) => {
                let mut hits = found
                    .iter()
                    .filter(|(label, _, _)| instantiation_matches(label, &args));
                let first = hits.next();
                match (first, hits.next()) {
                    (Some((label, address, _)), None) => Ok((*address, label.clone())),
                    (Some(_), Some(_)) => anyhow::bail!(
                        "`instantiation:` matches more than one copy of {}:\n{}",
                        query.display(),
                        instantiation_list(&found)
                    ),
                    _ => anyhow::bail!(
                        "{} has no instantiation <{}> with its own body. Instantiations present:\n{}",
                        query.display(),
                        args.join(", "),
                        instantiation_list(&found)
                    ),
                }
            }
        }
    }

    fn inst_arguments(&self, generics: &GenericTables, inst: i32) -> Vec<String> {
        if inst < 0 || inst as usize >= generics.generic_inst_count {
            return Vec::new();
        }
        let Some(inst_vm) = self
            .binary
            .ptr_at_vm(generics.generic_insts + inst as u64 * 8)
        else {
            return Vec::new();
        };
        let Some(argc) = self.binary.u32_at_vm(inst_vm) else {
            return Vec::new();
        };
        if argc == 0 || argc > 32 {
            return Vec::new();
        }
        let Some(argv) = self.binary.ptr_at_vm(inst_vm + 8) else {
            return Vec::new();
        };
        (0..argc as u64)
            .map(|i| {
                self.binary
                    .ptr_at_vm(argv + i * 8)
                    .and_then(|type_vm| self.type_name_at(type_vm, 0))
                    .unwrap_or_else(|| "?".into())
            })
            .collect()
    }

    fn params_match(&self, method: &MethodDef, patterns: &[ParamPattern]) -> anyhow::Result<bool> {
        for (position, pattern) in patterns.iter().enumerate() {
            let ParamPattern::Named(wanted) = pattern else {
                continue;
            };
            let type_index = self
                .meta
                .param_type(method.param_start as usize + position)?;
            let Some(actual) = self.type_name(type_index) else {
                // Without the types array we cannot compare names; a parameter
                // count match is as far as this can go.
                anyhow::bail!(
                    "parameter types cannot be read in this build, so an overload cannot be \
                     selected by type; name it by parameter count instead, or use signatures"
                )
            };
            if !type_name_matches(&actual, wanted) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The managed name of the type at `types[index]`.
    fn type_name(&self, index: u32) -> Option<String> {
        let base = self.types_table?;
        let entry = self.binary.ptr_at_vm(base + index as u64 * 8)?;
        self.type_name_at(entry, 0)
    }

    fn type_name_at(&self, type_vm: u64, depth: usize) -> Option<String> {
        if depth > 6 {
            return Some("?".into());
        }
        let data = self.binary.u64_at_vm(type_vm)?;
        let bits = self.binary.u32_at_vm(type_vm + 8)?;
        let kind = ((bits >> 16) & 0xFF) as u8;
        let byref = (bits >> 29) & 1 == 1;
        let suffix = if byref { "&" } else { "" };
        let name = match kind {
            TYPE_VOID => "System.Void".to_string(),
            0x02 => "System.Boolean".into(),
            0x03 => "System.Char".into(),
            0x04 => "System.SByte".into(),
            0x05 => "System.Byte".into(),
            0x06 => "System.Int16".into(),
            0x07 => "System.UInt16".into(),
            0x08 => "System.Int32".into(),
            0x09 => "System.UInt32".into(),
            0x0A => "System.Int64".into(),
            0x0B => "System.UInt64".into(),
            0x0C => "System.Single".into(),
            0x0D => "System.Double".into(),
            0x0E => "System.String".into(),
            0x16 => "System.TypedReference".into(),
            0x18 => "System.IntPtr".into(),
            0x19 => "System.UIntPtr".into(),
            0x1C => "System.Object".into(),
            TYPE_CLASS | TYPE_VALUETYPE => {
                let index = data as usize;
                if index < self.type_count {
                    self.full_type_name(index).ok()?
                } else {
                    return None;
                }
            }
            TYPE_SZARRAY => {
                let inner = self.binary.ptr_at_vm(type_vm).unwrap_or(data);
                format!("{}[]", self.type_name_at(inner, depth + 1)?)
            }
            TYPE_PTR => {
                let inner = self.binary.ptr_at_vm(type_vm).unwrap_or(data);
                format!("{}*", self.type_name_at(inner, depth + 1)?)
            }
            TYPE_ARRAY => {
                let array = self.binary.ptr_at_vm(type_vm).unwrap_or(data);
                let element = self.binary.ptr_at_vm(array)?;
                let rank = self.binary.u32_at_vm(array + 8).unwrap_or(1) & 0xFF;
                let commas = ",".repeat(rank.saturating_sub(1) as usize);
                format!("{}[{commas}]", self.type_name_at(element, depth + 1)?)
            }
            TYPE_GENERICINST => {
                let generic_class = self.binary.ptr_at_vm(type_vm).unwrap_or(data);
                let definition = self.binary.u64_at_vm(generic_class)? as usize;
                if definition < self.type_count {
                    self.full_type_name(definition).ok()?
                } else {
                    return None;
                }
            }
            TYPE_VAR => format!("!{data}"),
            TYPE_MVAR => format!("!!{data}"),
            other => format!("type0x{other:X}"),
        };
        Some(format!("{name}{suffix}"))
    }

    fn describe_method(&self, method_index: usize) -> anyhow::Result<String> {
        let m = self.meta.method(method_index)?;
        let class = self.full_type_name(m.declaring_type as usize)?;
        Ok(format!(
            "{class}::{}",
            self.meta.string(m.name_index).unwrap_or("?")
        ))
    }

    fn overload_list(&self, named: &[(usize, MethodDef)]) -> anyhow::Result<String> {
        let mut lines = Vec::new();
        for (index, m) in named {
            let mut params = Vec::new();
            for position in 0..m.param_count as usize {
                let type_index = self.meta.param_type(m.param_start as usize + position)?;
                params.push(
                    self.type_name(type_index)
                        .unwrap_or_else(|| format!("arg{position}")),
                );
            }
            let generic = if m.generic_container >= 0 {
                " (generic)"
            } else {
                ""
            };
            let body = match self.image_of_type.get(m.declaring_type as usize) {
                Some(image) if *image != u32::MAX => self
                    .modules
                    .get(&self.images[*image as usize])
                    .and_then(|module| method_pointer(&self.binary, module, m.token))
                    .map(|address| format!("0x{address:x}"))
                    .unwrap_or_else(|| "no native body".into()),
                _ => "no native body".into(),
            };
            lines.push(format!(
                "  {}({}){generic} -> {body}   [method {index}]",
                self.meta.string(m.name_index).unwrap_or("?"),
                params.join(", ")
            ));
        }
        Ok(lines.join("\n"))
    }

    fn nearby_methods(&self, t: &TypeDef, wanted: &str) -> anyhow::Result<String> {
        let mut close = Vec::new();
        let lowered = wanted.to_ascii_lowercase();
        for k in 0..t.method_count as usize {
            let m = self.meta.method(t.method_start as usize + k)?;
            let name = self.meta.string(m.name_index).unwrap_or("");
            if name.to_ascii_lowercase().contains(&lowered)
                || lowered.contains(&name.to_ascii_lowercase())
            {
                close.push(name.to_string());
            }
        }
        close.sort();
        close.dedup();
        Ok(if close.is_empty() {
            String::new()
        } else {
            format!(". Similar names: {}", close.join(", "))
        })
    }

    fn class_not_found(&self, class: &str) -> anyhow::Error {
        let simple = class.rsplit('.').next().unwrap_or(class);
        let mut matches: Vec<&String> = self
            .types_by_name
            .keys()
            .filter(|name| name.rsplit('.').next() == Some(simple))
            .collect();
        matches.sort();
        matches.truncate(8);
        if matches.is_empty() {
            anyhow::anyhow!("class {class} is not in this build's metadata")
        } else {
            anyhow::anyhow!(
                "class {class} is not in this build's metadata. Types with that name: {}",
                matches
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }

    fn no_body_error(&self, display: &str, method: &MethodDef) -> anyhow::Error {
        let abstract_bit = method.flags & 0x0400 != 0;
        let reason = if abstract_bit {
            "it is abstract"
        } else {
            "it has no entry in methodPointers (extern, stripped, or inlined away)"
        };
        anyhow::anyhow!("{display} has no native body in this build: {reason}; use signatures")
    }

    /// Returns the body address when `address` holds exactly one unconditional
    /// branch that is not itself another managed method's entry. A branch into
    /// another method is a tail call, and following it would hook that method.
    fn entry_stub_target(&self, address: u64) -> Option<u64> {
        let at = self.binary.vm_to_file(address)?;
        let instruction = u32at(self.binary.data, at).ok()?;
        if instruction & 0xFC00_0000 != 0x1400_0000 {
            return None;
        }
        let imm = ((instruction & 0x03FF_FFFF) << 2) as i32;
        let displacement = (imm << 4) >> 4; // sign-extend 28 bits
        let target = address.checked_add_signed(displacement as i64)?;
        if self.methods_at.contains_key(&target) {
            return None;
        }
        let same_section = match (
            self.binary.section_of(address),
            self.binary.section_of(target),
        ) {
            (Some(a), Some(b)) => a.addr == b.addr,
            _ => false,
        };
        if !same_section {
            return None;
        }
        Some(target)
    }

    /// Diagnostics for the preflight report.
    pub fn summary(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        out.insert("metadata_version".into(), SUPPORTED_VERSION.to_string());
        out.insert("assemblies".into(), self.modules.len().to_string());
        out.insert("type_definitions".into(), self.type_count.to_string());
        out.insert("methods".into(), self.method_count.to_string());
        out.insert(
            "methods_with_native_body".into(),
            self.methods_at.len().to_string(),
        );
        out.insert(
            "generic_tables".into(),
            match &self.generics {
                Some(g) => format!(
                    "{} method specs, {} generic bodies",
                    g.method_spec_count, g.generic_method_pointer_count
                ),
                None => "unavailable".into(),
            },
        );
        if !self.degraded.is_empty() {
            out.insert("degraded".into(), self.degraded.join("; "));
        }
        out
    }
}

/// `methodPointers[(token & 0xFFFFFF) - 1]`, read through the rebase table.
fn method_pointer(binary: &Binary<'_>, module: &Module, token: u32) -> Option<u64> {
    let rid = u64::from(token & 0x00FF_FFFF);
    if rid == 0 || rid > module.method_count {
        return None;
    }
    binary.ptr_at_vm(module.method_pointers + (rid - 1) * 8)
}

/// `Assembly-CSharp` and `Assembly-CSharp.dll` both name the same assembly.
fn assembly_matches(actual: &str, wanted: &str) -> bool {
    let strip = |name: &str| {
        name.strip_suffix(".dll")
            .unwrap_or(name)
            .to_ascii_lowercase()
    };
    strip(actual) == strip(wanted)
}

/// A parameter type matches by full name, or by simple name when the pattern
/// carries no namespace. Suffixes (`[]`, `*`, `&`) must match either way.
fn type_name_matches(actual: &str, wanted: &str) -> bool {
    if actual.eq_ignore_ascii_case(wanted) {
        return true;
    }
    let split = |name: &str| -> (String, String) {
        let suffix_start = name.find(['[', '*', '&']).unwrap_or(name.len());
        (
            name[..suffix_start].to_string(),
            name[suffix_start..].to_string(),
        )
    };
    let (actual_core, actual_suffix) = split(actual);
    let (wanted_core, wanted_suffix) = split(wanted);
    if actual_suffix != wanted_suffix {
        return false;
    }
    if wanted_core.contains('.') {
        return actual_core.eq_ignore_ascii_case(&wanted_core);
    }
    let simple = actual_core
        .rsplit('.')
        .next()
        .unwrap_or(&actual_core)
        .to_string();
    simple.eq_ignore_ascii_case(&wanted_core)
        || actual_core
            .rsplit('/')
            .next()
            .is_some_and(|last| last.eq_ignore_ascii_case(&wanted_core))
}

fn instantiation_matches(label: &str, wanted: &[String]) -> bool {
    let actual: Vec<&str> = label.split(", ").filter(|s| !s.is_empty()).collect();
    if actual.len() != wanted.len() {
        return false;
    }
    actual
        .iter()
        .zip(wanted)
        .all(|(a, w)| w == "*" || type_name_matches(a, w))
}

fn instantiation_list(found: &[(String, u64, bool)]) -> String {
    let mut lines: Vec<String> = found
        .iter()
        .take(24)
        .map(|(label, address, _)| format!("  <{label}> -> 0x{address:x}"))
        .collect();
    if found.len() > 24 {
        lines.push(format!("  ... and {} more", found.len() - 24));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_plain_name() {
        let q = ManagedQuery::parse("Ns.Class::Method").unwrap();
        assert_eq!(q.class, "Ns.Class");
        assert_eq!(q.method, "Method");
        assert!(q.params.is_none());
    }

    #[test]
    fn parses_parameters_and_nested_types() {
        let q = ManagedQuery::parse("Ns.Outer+Inner::Method(System.Int32, *, Byte[])").unwrap();
        assert_eq!(q.class, "Ns.Outer/Inner");
        let params = q.params.unwrap();
        assert_eq!(params.len(), 3);
        assert_eq!(params[0], ParamPattern::Named("System.Int32".into()));
        assert_eq!(params[1], ParamPattern::Any);
        assert_eq!(params[2], ParamPattern::Named("Byte[]".into()));
    }

    #[test]
    fn parses_an_empty_parameter_list() {
        let q = ManagedQuery::parse("Ns.Class::get_Value()").unwrap();
        assert_eq!(q.params.unwrap().len(), 0);
    }

    #[test]
    fn keeps_generic_arity_and_constructors() {
        let q = ManagedQuery::parse("System.Collections.Generic.List`1::.ctor()").unwrap();
        assert_eq!(q.class, "System.Collections.Generic.List`1");
        assert_eq!(q.method, ".ctor");
    }

    #[test]
    fn rejects_malformed_names() {
        assert!(ManagedQuery::parse("Ns.Class.Method").is_err());
        assert!(ManagedQuery::parse("::Method").is_err());
        assert!(ManagedQuery::parse("Ns.Class::").is_err());
        assert!(ManagedQuery::parse("Ns.Class::Method(Int32").is_err());
        assert!(ManagedQuery::parse("Ns.Class::Method(Int32,)").is_err());
    }

    #[test]
    fn matches_simple_and_full_type_names() {
        assert!(type_name_matches("System.Int32", "System.Int32"));
        assert!(type_name_matches("System.Int32", "Int32"));
        assert!(!type_name_matches("System.Int32", "System.Int64"));
        assert!(type_name_matches("System.Byte[]", "Byte[]"));
        assert!(!type_name_matches("System.Byte[]", "Byte"));
        assert!(type_name_matches("System.Char*", "Char*"));
        assert!(type_name_matches("System.Int32&", "Int32&"));
        assert!(type_name_matches("Ns.Outer/Inner", "Inner"));
        assert!(type_name_matches("Ns.Outer/Inner", "Ns.Outer/Inner"));
    }

    #[test]
    fn matches_assembly_names_with_and_without_dll() {
        assert!(assembly_matches("Assembly-CSharp.dll", "Assembly-CSharp"));
        assert!(assembly_matches(
            "Assembly-CSharp.dll",
            "assembly-csharp.dll"
        ));
        assert!(!assembly_matches("mscorlib.dll", "Assembly-CSharp"));
    }

    /// End-to-end resolution against a real app, which cannot ship in the
    /// repository. Point both variables at files taken from the same IPA:
    ///
    /// ```text
    /// IAOT_MANAGED_MODULE=/tmp/UnityFramework     /// IAOT_MANAGED_METADATA=/tmp/global-metadata.dat     /// IAOT_MANAGED_EXPECT='Ns.Class::Method(System.Int32)=0x2b952fc'     ///   cargo test managed_resolves_real_app -- --ignored --nocapture
    /// ```
    ///
    /// `IAOT_MANAGED_EXPECT` is a semicolon-separated list of
    /// `managed name=0xaddress` pairs; without it the test only prints the
    /// index summary. A name may carry `|arg,arg` before the `=` to pick a
    /// generic instantiation, e.g. ``List`1::Add|System.Int32=0x...``.
    #[test]
    #[ignore = "needs a real IL2CPP module and its metadata"]
    fn managed_resolves_real_app() {
        let (Ok(module_path), Ok(metadata_path)) = (
            std::env::var("IAOT_MANAGED_MODULE"),
            std::env::var("IAOT_MANAGED_METADATA"),
        ) else {
            panic!("set IAOT_MANAGED_MODULE and IAOT_MANAGED_METADATA")
        };
        let module = std::fs::read(module_path).expect("module");
        let metadata = std::fs::read(metadata_path).expect("metadata");
        let index = ManagedIndex::build(&module, &metadata).expect("index");
        for (key, value) in index.summary() {
            println!("{key}: {value}");
        }
        let Ok(expectations) = std::env::var("IAOT_MANAGED_EXPECT") else {
            return;
        };
        for expectation in expectations.split(';').filter(|s| !s.trim().is_empty()) {
            let (spec, wanted) = expectation
                .rsplit_once('=')
                .expect("each expectation is name=0xaddress");
            let wanted = u64::from_str_radix(wanted.trim().trim_start_matches("0x"), 16)
                .expect("hex address");
            let (spec, instantiation) = match spec.trim().split_once('|') {
                Some((name, args)) => (
                    name,
                    Some(Instantiation::Args(
                        args.split(',').map(|a| a.trim().to_string()).collect(),
                    )),
                ),
                None => (spec.trim(), None),
            };
            let mut query = ManagedQuery::parse(spec).expect("name parses");
            query.instantiation = instantiation;
            let resolution = index
                .resolve(&query)
                .unwrap_or_else(|error| panic!("{spec}: {error}"));
            println!(
                "{spec} -> 0x{:x} ({}){}",
                resolution.vmaddr,
                resolution.how,
                resolution
                    .notes
                    .iter()
                    .map(|n| format!(
                        "
    note: {n}"
                    ))
                    .collect::<String>()
            );
            assert_eq!(
                resolution.vmaddr, wanted,
                "{spec} resolved to 0x{:x}, expected 0x{wanted:x}",
                resolution.vmaddr
            );
        }
    }

    #[test]
    fn matches_instantiations_with_wildcards() {
        assert!(instantiation_matches("System.Int32", &["Int32".into()]));
        assert!(instantiation_matches(
            "System.Byte, System.Object",
            &["*".into(), "System.Object".into()]
        ));
        assert!(!instantiation_matches(
            "System.Byte, System.Object",
            &["System.Object".into()]
        ));
    }
}
