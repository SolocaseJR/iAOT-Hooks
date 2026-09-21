//! Build-time name -> address/offset resolution from an IL2CPP metadata dump.
//!
//! Many shipping IL2CPP games strip the public `il2cpp_*` export API, so a
//! runtime resolver cannot look classes/methods/fields up by name on device.
//! iAOT sidesteps this by resolving the handful of names a project actually uses
//! *at build time*, from a metadata dump produced by an offline static-analysis
//! tool, and baking the resulting native RVAs / field offsets into the generated
//! offline tables (`generate.rs`). Nothing is hand-typed, and a game update that
//! moves an address surfaces as a loud "name not found" error at `iaot generate`
//! instead of a silently-stale number.
//!
//! # Supported dump formats (auto-detected)
//!
//! You only need a small slice of a tool's output - normally one or two files.
//! Point `runtime.metadata_package` in the manifest at the directory that holds
//! them (or directly at one of the files); the format is detected from the file
//! names present. `runtime.metadata_format` can force a format if detection is
//! ever ambiguous.
//!
//! * **Cpp2IL analysis package** (`--output-as=analysis-package`; the richest,
//!   and the format iAOT is primarily tested against):
//!     - `cpp2il_methods.jsonl` - one managed method per line: native address,
//!       assembly, declaring type, parameter types. Needed for methods / string
//!       factory / internal calls.
//!     - `cpp2il_types.jsonl`   - one managed type per line: per-field name,
//!       offset and size. Needed only if the project has `offline_fields`.
//!   The other files in the package (`cpp2il_calls.jsonl`, `*_pseudocode.jsonl`,
//!   the `.h` headers, the IDA/Ghidra `.py` scripts) are NOT read - you can ship
//!   just the two `.jsonl` files.
//!
//! * **Il2CppDumper** (Perfare's ubiquitous tool - "the IDA/Ghidra dump"):
//!     - `script.json` - the file its `ida.py` / `ghidra.py` consume. Method
//!       addresses keyed by `"Type$$Method"`. Covers methods and internal-call
//!       targets. Carries **no field offsets**.
//!     - `dump.cs`     - the single-file human dump. Methods (`// RVA: 0x..`) and
//!       instance field offsets (`// 0x..`). Use this when you need
//!       `offline_fields`; on its own it also covers methods.
//!   Give the directory containing either file (or a direct path). If both are
//!   present, `script.json` supplies methods and `dump.cs` supplies fields.
//!
//! # The RVA convention (read this before trusting any number)
//!
//! Every "rva" this module emits is `native_VA - 0x1_0000_0000`, i.e. relative
//! to the arm64 image base (the main executable's `__TEXT` vmaddr). The runtime
//! adds the real load slide to it. This is deliberately **not** the Cpp2IL `rva`
//! JSON field, which is relative to the tool's `primaryExecutableAddress` (e.g.
//! `0x1_0000_4000` for COD, i.e. past the Mach-O header) and would therefore be
//! short by the header size. We always recompute from the absolute `address`.
//! Il2CppDumper's `script.json` `Address` and `dump.cs` `RVA` are already
//! image-base-relative (see its `ida.py`: `addr = imagebase + Address`), so they
//! are used as-is.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

/// Common arm64 MH_EXECUTE image base. Some Mach-O IL2CPP images use vmaddr 0,
/// in which case Cpp2IL reports an already image-relative native address.
/// Matches `macho::image_base` (the main executable's `__TEXT` vmaddr).
const IMAGE_BASE: u64 = 0x1_0000_0000;

// Cpp2IL analysis-package file names.
const CPP2IL_METHODS_FILE: &str = "cpp2il_methods.jsonl";
const CPP2IL_TYPES_FILE: &str = "cpp2il_types.jsonl";
// Il2CppDumper file names.
const DUMPER_SCRIPT_FILE: &str = "script.json";
const DUMPER_DUMP_FILE: &str = "dump.cs";

// ---------------------------------------------------------------------------
// Shared, format-independent index
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct MethodHit {
    /// Managed assembly, when the source knows it ("" if not, e.g. script.json).
    assembly: String,
    param_count: usize,
    /// Managed parameter types, when the source knows them (Cpp2IL only).
    param_types: Vec<String>,
    rva: u64,
}

#[derive(Clone)]
struct FieldHit {
    assembly: String,
    offset: u64,
    size: u64,
}

/// A loaded, name-indexed view of a metadata dump, holding only the methods and
/// fields that were requested (bounded memory, not the whole dump). Look-ups are
/// by managed type/member name. All three dump parsers populate this same shape,
/// so the query methods below are format-independent.
pub struct MetadataIndex {
    // (declaringType, methodName) -> hits (across assemblies / overloads).
    // `declaringType` is stored exactly as the dump spells it, which may be the
    // fully-qualified "Namespace.Class" (Cpp2IL, most Il2CppDumper output) or a
    // bare "Class" (some Il2CppDumper output). Queries try both spellings.
    methods: HashMap<(String, String), Vec<MethodHit>>,
    // (typeName, fieldName) -> hits, same spelling rule as methods.
    fields: HashMap<(String, String), Vec<FieldHit>>,
}

/// One of the supported dump formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetadataFormat {
    Cpp2ilPackage,
    Il2CppDumper,
}

impl MetadataFormat {
    /// Parse the optional `runtime.metadata_format` manifest override.
    /// `None`/`"auto"` means "detect from the files present".
    fn from_hint(hint: Option<&str>) -> Result<Option<Self>> {
        match hint.map(|h| h.trim().to_ascii_lowercase()).as_deref() {
            None | Some("") | Some("auto") => Ok(None),
            Some("cpp2il") | Some("cpp2il_package") | Some("analysis-package") => {
                Ok(Some(Self::Cpp2ilPackage))
            }
            Some("il2cpp_dumper") | Some("il2cppdumper") | Some("dumper") => {
                Ok(Some(Self::Il2CppDumper))
            }
            Some(other) => Err(anyhow!(
                "unknown runtime.metadata_format '{other}' (expected: auto, cpp2il, il2cpp_dumper)"
            )),
        }
    }
}

/// Concrete files a detected format will read.
enum Sources {
    Cpp2il {
        methods: PathBuf,
        types: PathBuf,
    },
    Il2CppDumper {
        script: Option<PathBuf>,
        dump_cs: Option<PathBuf>,
    },
}

fn join_type(namespace: &str, class_name: &str) -> String {
    if namespace.is_empty() {
        class_name.to_string()
    } else {
        format!("{namespace}.{class_name}")
    }
}

/// Bare class name (last `.`-separated segment) of a possibly-qualified type.
fn bare_class(type_full: &str) -> &str {
    type_full.rsplit('.').next().unwrap_or(type_full)
}

/// Given an internal-call name ("Namespace.Class::Method" or "...Method_Injected"),
/// return the full type name and the ordered list of managed method names to try
/// when resolving it. Unity's IL2CPP emits the injected native thunk under
/// inconsistent prefixes (`INTERNAL_CALL_<M>` for methods, `INTERNAL_<M>` for
/// property accessors), while a simple getter has no separate thunk and resolves
/// to itself. The ordering ensures a thunk variant wins over the plain managed
/// method (whose address is a different, wrong target for an injected call).
/// NB: the `INTERNAL_*` thunk names are a Cpp2IL concept; against Il2CppDumper
/// only the trailing plain-name candidates can match.
fn icall_candidates(name: &str) -> Result<(String, Vec<String>)> {
    let (type_full, method) = name
        .rsplit_once("::")
        .ok_or_else(|| anyhow!("internal call name '{name}' is not 'Type::Method'"))?;
    let mut names = Vec::new();
    if let Some(base) = method.strip_suffix("_Injected") {
        names.push(format!("INTERNAL_CALL_{base}"));
        names.push(format!("INTERNAL_{base}"));
        names.push(format!("{base}_Injected"));
        names.push(base.to_string());
    } else {
        names.push(method.to_string());
    }
    Ok((type_full.to_string(), names))
}

// ---------------------------------------------------------------------------
// Format detection
// ---------------------------------------------------------------------------

/// Resolve the configured path (a directory or a single file) plus an optional
/// format override into the concrete set of files to parse.
fn detect_sources(path: &Path, hint: Option<MetadataFormat>) -> Result<Sources> {
    // A direct file path pins the directory and, usually, the format.
    if path.is_file() {
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match file_name.as_str() {
            f if f == CPP2IL_METHODS_FILE || f == CPP2IL_TYPES_FILE => {
                return Ok(Sources::Cpp2il {
                    methods: dir.join(CPP2IL_METHODS_FILE),
                    types: dir.join(CPP2IL_TYPES_FILE),
                });
            }
            f if f == DUMPER_SCRIPT_FILE => {
                return Ok(Sources::Il2CppDumper {
                    script: Some(path.to_path_buf()),
                    dump_cs: exists_or_none(dir.join(DUMPER_DUMP_FILE)),
                });
            }
            f if f == DUMPER_DUMP_FILE || f.ends_with(".cs") => {
                return Ok(Sources::Il2CppDumper {
                    script: exists_or_none(dir.join(DUMPER_SCRIPT_FILE)),
                    dump_cs: Some(path.to_path_buf()),
                });
            }
            _ => {}
        }
        // Fall through to directory-based detection using the parent.
        return detect_in_dir(dir, hint, Some(path));
    }

    if path.is_dir() {
        return detect_in_dir(path, hint, None);
    }

    Err(anyhow!(
        "metadata_package path {} is neither a file nor a directory",
        path.display()
    ))
}

fn detect_in_dir(
    dir: &Path,
    hint: Option<MetadataFormat>,
    explicit_file: Option<&Path>,
) -> Result<Sources> {
    let has_cpp2il = dir.join(CPP2IL_METHODS_FILE).is_file();
    let script = exists_or_none(dir.join(DUMPER_SCRIPT_FILE));
    let dump_cs = exists_or_none(dir.join(DUMPER_DUMP_FILE));
    let has_dumper = script.is_some() || dump_cs.is_some();

    let format = match hint {
        Some(explicit) => explicit,
        None => {
            // Auto: Cpp2IL wins if present (richest); else Il2CppDumper.
            if has_cpp2il {
                MetadataFormat::Cpp2ilPackage
            } else if has_dumper {
                MetadataFormat::Il2CppDumper
            } else {
                return Err(anyhow!(
                    "no recognized metadata dump found in {}: expected one of \
                     {CPP2IL_METHODS_FILE}, {DUMPER_SCRIPT_FILE}, {DUMPER_DUMP_FILE}\
                     {}",
                    dir.display(),
                    explicit_file
                        .map(|f| format!(" (from path {})", f.display()))
                        .unwrap_or_default()
                ));
            }
        }
    };

    match format {
        MetadataFormat::Cpp2ilPackage => Ok(Sources::Cpp2il {
            methods: dir.join(CPP2IL_METHODS_FILE),
            types: dir.join(CPP2IL_TYPES_FILE),
        }),
        MetadataFormat::Il2CppDumper => {
            if !has_dumper {
                return Err(anyhow!(
                    "metadata_format=il2cpp_dumper but neither {DUMPER_SCRIPT_FILE} nor \
                     {DUMPER_DUMP_FILE} is present in {}",
                    dir.display()
                ));
            }
            Ok(Sources::Il2CppDumper { script, dump_cs })
        }
    }
}

fn exists_or_none(path: PathBuf) -> Option<PathBuf> {
    if path.is_file() {
        Some(path)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Loading / dispatch
// ---------------------------------------------------------------------------

impl MetadataIndex {
    /// Detect the dump format at `path` and load only the requested entries.
    pub fn load(
        path: &Path,
        format_hint: Option<&str>,
        wanted_methods: &HashSet<(String, String)>,
        wanted_fields: &HashSet<(String, String)>,
    ) -> Result<Self> {
        let hint = MetadataFormat::from_hint(format_hint)?;
        let sources = detect_sources(path, hint)?;
        let mut index = Self {
            methods: HashMap::new(),
            fields: HashMap::new(),
        };
        match sources {
            Sources::Cpp2il { methods, types } => {
                index.load_cpp2il(&methods, &types, wanted_methods, wanted_fields)?;
            }
            Sources::Il2CppDumper { script, dump_cs } => {
                index.load_il2cpp_dumper(
                    script.as_deref(),
                    dump_cs.as_deref(),
                    wanted_methods,
                    wanted_fields,
                )?;
            }
        }
        Ok(index)
    }

    // ----- Cpp2IL analysis package -----

    fn load_cpp2il(
        &mut self,
        methods_path: &Path,
        types_path: &Path,
        wanted_methods: &HashSet<(String, String)>,
        wanted_fields: &HashSet<(String, String)>,
    ) -> Result<()> {
        if !wanted_methods.is_empty() {
            let file = File::open(methods_path).with_context(|| {
                format!("opening Cpp2IL methods file {}", methods_path.display())
            })?;
            for line in BufReader::with_capacity(1 << 20, file).lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let record: Cpp2ilMethod = match serde_json::from_str(&line) {
                    Ok(record) => record,
                    Err(_) => continue,
                };
                let key = (record.declaring_type.clone(), record.name.clone());
                if !wanted_methods.contains(&key) {
                    continue;
                }
                if record.address == 0 {
                    // No native body; not usable.
                    continue;
                }
                self.methods.entry(key).or_default().push(MethodHit {
                    assembly: record.assembly,
                    param_count: record.parameters.len(),
                    param_types: record.parameters.into_iter().map(|p| p.ty).collect(),
                    rva: if record.address >= IMAGE_BASE {
                        record.address - IMAGE_BASE
                    } else {
                        record.address
                    },
                });
            }
        }

        if !wanted_fields.is_empty() {
            let wanted_types: HashSet<&String> = wanted_fields.iter().map(|(t, _)| t).collect();
            let file = File::open(types_path)
                .with_context(|| format!("opening Cpp2IL types file {}", types_path.display()))?;
            for line in BufReader::with_capacity(1 << 20, file).lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let record: Cpp2ilType = match serde_json::from_str(&line) {
                    Ok(record) => record,
                    Err(_) => continue,
                };
                if !wanted_types.contains(&record.full_name) {
                    continue;
                }
                for field in record.fields {
                    let key = (record.full_name.clone(), field.name.clone());
                    if !wanted_fields.contains(&key) {
                        continue;
                    }
                    self.fields.entry(key).or_default().push(FieldHit {
                        assembly: record.assembly.clone(),
                        offset: field.offset,
                        size: field.size,
                    });
                }
            }
        }
        Ok(())
    }

    // ----- Il2CppDumper (script.json + dump.cs) -----

    fn load_il2cpp_dumper(
        &mut self,
        script: Option<&Path>,
        dump_cs: Option<&Path>,
        wanted_methods: &HashSet<(String, String)>,
        wanted_fields: &HashSet<(String, String)>,
    ) -> Result<()> {
        if let Some(script) = script {
            self.load_dumper_script(script, wanted_methods)?;
        }
        // dump.cs is the only Il2CppDumper source of field offsets, and can also
        // fill in method addresses when there is no script.json.
        let need_methods_from_dump = script.is_none() && !wanted_methods.is_empty();
        if let Some(dump_cs) = dump_cs {
            self.load_dumper_dump_cs(
                dump_cs,
                if need_methods_from_dump {
                    wanted_methods
                } else {
                    &EMPTY_SET
                },
                wanted_fields,
            )?;
        } else if !wanted_fields.is_empty() {
            return Err(anyhow!(
                "this project has offline_fields, but the Il2CppDumper dump has no {DUMPER_DUMP_FILE} \
                 (script.json carries no field offsets). Add dump.cs next to script.json, switch to a \
                 Cpp2IL package, or give each offline_field an explicit `offset:` in the manifest."
            ));
        }
        Ok(())
    }

    /// Parse Il2CppDumper `script.json`. `ScriptMethod[].Name` is `"Type$$Method"`
    /// and `Address` is already image-base-relative (used directly as the RVA).
    /// The whole file is read into memory (it has no line structure to stream);
    /// only wanted entries are retained in the index.
    fn load_dumper_script(
        &mut self,
        path: &Path,
        wanted_methods: &HashSet<(String, String)>,
    ) -> Result<()> {
        if wanted_methods.is_empty() {
            return Ok(());
        }
        let file = File::open(path)
            .with_context(|| format!("opening Il2CppDumper script {}", path.display()))?;
        let script: DumperScript = serde_json::from_reader(BufReader::with_capacity(1 << 20, file))
            .with_context(|| format!("parsing Il2CppDumper script {}", path.display()))?;
        for entry in script.script_method {
            let Some((type_part, method)) = entry.name.rsplit_once("$$") else {
                continue;
            };
            let type_part = type_part.trim();
            let method = method.trim();
            // Retain if either the qualified type or the bare class is wanted;
            // the wanted set carries both spellings (see `wanted_names`).
            let key_full = (type_part.to_string(), method.to_string());
            let key_bare = (bare_class(type_part).to_string(), method.to_string());
            if !wanted_methods.contains(&key_full) && !wanted_methods.contains(&key_bare) {
                continue;
            }
            self.methods.entry(key_full).or_default().push(MethodHit {
                assembly: String::new(),
                param_count: managed_param_count(&entry.signature).unwrap_or(0).max(0) as usize,
                param_types: Vec::new(),
                rva: entry.address,
            });
        }
        Ok(())
    }

    /// Parse an Il2CppDumper `dump.cs`. Instance field offsets come from the
    /// `// 0x..` trailing comment; method RVAs from the `// RVA: 0x..` comment
    /// that precedes each declaration. Best-effort line parser - documented as a
    /// fallback for when a full Cpp2IL package is not available.
    fn load_dumper_dump_cs(
        &mut self,
        path: &Path,
        wanted_methods: &HashSet<(String, String)>,
        wanted_fields: &HashSet<(String, String)>,
    ) -> Result<()> {
        let file = File::open(path)
            .with_context(|| format!("opening Il2CppDumper dump {}", path.display()))?;
        let reader = BufReader::with_capacity(1 << 20, file);

        let mut namespace = String::new();
        let mut type_full: Option<String> = None;
        let mut section = DumpSection::None;
        let mut pending_rva: Option<u64> = None;

        for line in reader.lines() {
            let raw = line?;
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(rest) = line.strip_prefix("// Namespace:") {
                namespace = rest.trim().to_string();
                type_full = None;
                section = DumpSection::None;
                pending_rva = None;
                continue;
            }
            if line == "// Fields" {
                section = DumpSection::Fields;
                continue;
            }
            if line == "// Properties" || line == "// Events" || line == "// Nested types" {
                section = DumpSection::Other;
                continue;
            }
            if line == "// Methods" {
                section = DumpSection::Methods;
                continue;
            }

            if let Some(name) = parse_type_decl(line) {
                type_full = Some(join_type(&namespace, name));
                section = DumpSection::None;
                pending_rva = None;
                continue;
            }

            let Some(current_type) = type_full.as_deref() else {
                continue;
            };

            match section {
                DumpSection::Fields if !wanted_fields.is_empty() => {
                    if let Some((field_name, offset)) = parse_field_line(line) {
                        insert_dumper_field(
                            &mut self.fields,
                            current_type,
                            field_name,
                            offset,
                            wanted_fields,
                        );
                    }
                }
                DumpSection::Methods if !wanted_methods.is_empty() => {
                    if let Some(rva) = parse_rva_comment(line) {
                        pending_rva = rva;
                    } else if line.starts_with("//") || line.starts_with('[') {
                        // Attribute or other comment between the RVA line and the
                        // declaration: keep the pending RVA and move on.
                    } else if let Some(rva) = pending_rva.take() {
                        if let Some((method_name, param_count)) = parse_method_decl(line) {
                            let key_full = (current_type.to_string(), method_name.to_string());
                            let key_bare = (
                                bare_class(current_type).to_string(),
                                method_name.to_string(),
                            );
                            if wanted_methods.contains(&key_full)
                                || wanted_methods.contains(&key_bare)
                            {
                                self.methods.entry(key_full).or_default().push(MethodHit {
                                    assembly: String::new(),
                                    param_count,
                                    param_types: Vec::new(),
                                    rva,
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    // ----- Queries (format-independent) -----

    fn methods_for(
        &self,
        type_full: &str,
        class_name: &str,
        name: &str,
    ) -> Option<&Vec<MethodHit>> {
        self.methods
            .get(&(type_full.to_string(), name.to_string()))
            .or_else(|| {
                if class_name != type_full {
                    self.methods
                        .get(&(class_name.to_string(), name.to_string()))
                } else {
                    None
                }
            })
            .filter(|hits| !hits.is_empty())
    }

    fn fields_for(&self, type_full: &str, class_name: &str, name: &str) -> Option<&Vec<FieldHit>> {
        self.fields
            .get(&(type_full.to_string(), name.to_string()))
            .or_else(|| {
                if class_name != type_full {
                    self.fields.get(&(class_name.to_string(), name.to_string()))
                } else {
                    None
                }
            })
            .filter(|hits| !hits.is_empty())
    }

    /// Resolve a managed method to its native RVA. `assembly` is matched when it
    /// names a real managed assembly (Cpp2IL only); `param_count` disambiguates
    /// overloads (pass a negative value to accept a unique match of any arity).
    /// `param_types`, when given, takes precedence (Cpp2IL only).
    pub fn method_rva(
        &self,
        assembly: &str,
        namespace: &str,
        class_name: &str,
        name: &str,
        param_count: i32,
        param_types: Option<&[String]>,
    ) -> Result<u64> {
        let type_full = join_type(namespace, class_name);
        let hits = self
            .methods_for(&type_full, class_name, name)
            .ok_or_else(|| anyhow!("method {type_full}::{name} not found in the metadata dump"))?;

        let mut candidates: Vec<&MethodHit> =
            hits.iter().filter(|hit| hit.assembly == assembly).collect();
        if candidates.is_empty() {
            // Assembly name did not line up (e.g. a manifest module alias, or a
            // source with no assembly info); match on type+name across assemblies.
            candidates = hits.iter().collect();
        }
        if let Some(types) = param_types {
            candidates.retain(|hit| hit.param_types.as_slice() == types);
        } else if param_count >= 0 {
            candidates.retain(|hit| hit.param_count == param_count as usize);
        }

        let mut rvas: Vec<u64> = candidates.iter().map(|hit| hit.rva).collect();
        rvas.sort_unstable();
        rvas.dedup();
        match rvas.as_slice() {
            [one] => Ok(*one),
            [] => Err(anyhow!(
                "method {type_full}::{name} (params={param_count}) matched no overload in the metadata dump"
            )),
            many => Err(anyhow!(
                "method {type_full}::{name} is ambiguous ({} distinct addresses); set parameter_count/parameter_types to disambiguate",
                many.len()
            )),
        }
    }

    /// Resolve an internal-call name ("Type::Method" / "...Method_Injected") to a
    /// native RVA, ignoring assembly. Tries the thunk-name variants in order and
    /// returns the first that resolves to a single address.
    pub fn icall_rva(&self, name: &str) -> Result<u64> {
        let (type_full, candidates) = icall_candidates(name)?;
        let class = bare_class(&type_full).to_string();
        for candidate in &candidates {
            let Some(hits) = self.methods_for(&type_full, &class, candidate) else {
                continue;
            };
            let mut rvas: Vec<u64> = hits.iter().map(|hit| hit.rva).collect();
            rvas.sort_unstable();
            rvas.dedup();
            if let [one] = rvas.as_slice() {
                return Ok(*one);
            }
            // Multiple distinct addresses under this candidate: skip and try the
            // next (more specific thunk names are listed first).
        }
        Err(anyhow!(
            "internal call {name} could not be resolved (tried {candidates:?})"
        ))
    }

    /// Resolve a field to (offset, size). `size` is 0 when the dump does not
    /// record one (e.g. Il2CppDumper); callers substitute a default width.
    pub fn field(
        &self,
        assembly: &str,
        namespace: &str,
        class_name: &str,
        name: &str,
    ) -> Result<(u64, u64)> {
        let type_full = join_type(namespace, class_name);
        let hits = self
            .fields_for(&type_full, class_name, name)
            .ok_or_else(|| anyhow!("field {type_full}.{name} not found in the metadata dump"))?;

        let mut candidates: Vec<&FieldHit> =
            hits.iter().filter(|hit| hit.assembly == assembly).collect();
        if candidates.is_empty() {
            candidates = hits.iter().collect();
        }
        let mut resolved: Vec<(u64, u64)> = candidates
            .iter()
            .map(|hit| (hit.offset, hit.size))
            .collect();
        resolved.sort_unstable();
        resolved.dedup();
        match resolved.as_slice() {
            [one] => Ok(*one),
            [] => Err(anyhow!("field {type_full}.{name} matched nothing")),
            _ => Err(anyhow!(
                "field {type_full}.{name} is ambiguous across assemblies"
            )),
        }
    }
}

// Empty wanted-set sentinel so dump.cs loading can suppress method parsing when
// script.json already covered methods, without allocating each call.
static EMPTY_SET: std::sync::LazyLock<HashSet<(String, String)>> =
    std::sync::LazyLock::new(HashSet::new);

#[derive(Clone, Copy, PartialEq)]
enum DumpSection {
    None,
    Fields,
    Methods,
    Other,
}

// ---------------------------------------------------------------------------
// Cpp2IL JSON records
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Cpp2ilMethod {
    assembly: String,
    #[serde(rename = "declaringType")]
    declaring_type: String,
    name: String,
    /// Native address. For the usual 0x100000000 Mach-O base, RVA is address
    /// minus IMAGE_BASE. For a vmaddr-zero image, address already is the RVA.
    /// The Cpp2IL `rva` JSON field is intentionally not used; see the module
    /// header for why it can point past the Mach-O header.
    address: u64,
    #[serde(default)]
    parameters: Vec<Cpp2ilParam>,
}

#[derive(Deserialize)]
struct Cpp2ilParam {
    #[serde(rename = "type")]
    ty: String,
}

#[derive(Deserialize)]
struct Cpp2ilType {
    assembly: String,
    #[serde(rename = "fullName")]
    full_name: String,
    #[serde(default)]
    fields: Vec<Cpp2ilField>,
}

#[derive(Deserialize)]
struct Cpp2ilField {
    name: String,
    offset: u64,
    #[serde(default)]
    size: u64,
}

// ---------------------------------------------------------------------------
// Il2CppDumper script.json records + helpers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DumperScript {
    #[serde(rename = "ScriptMethod", default)]
    script_method: Vec<DumperScriptMethod>,
}

#[derive(Deserialize)]
struct DumperScriptMethod {
    #[serde(rename = "Address")]
    address: u64,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Signature", default)]
    signature: String,
}

/// Count managed parameters in an Il2CppDumper C prototype, i.e. the comma-
/// separated args between the outermost parens, minus the trailing
/// `const MethodInfo* method` and the leading `__this` (instance methods).
/// Il2CppDumper flattens argument types to comma-free C types, so top-level
/// comma counting is safe.
fn managed_param_count(signature: &str) -> Option<i32> {
    let open = signature.find('(')?;
    let close = signature.rfind(')')?;
    if close <= open {
        return None;
    }
    let inside = signature[open + 1..close].trim();
    if inside.is_empty() {
        return Some(0);
    }
    let parts: Vec<&str> = inside.split(',').map(|s| s.trim()).collect();
    let mut count = parts.len() as i32;
    if parts.last().is_some_and(|p| p.contains("MethodInfo")) {
        count -= 1;
    }
    if parts.first().is_some_and(|p| p.contains("__this")) {
        count -= 1;
    }
    Some(count.max(0))
}

// ---------------------------------------------------------------------------
// Il2CppDumper dump.cs line parsing helpers
// ---------------------------------------------------------------------------

/// Detect a `class`/`struct`/`enum`/`interface` declaration line and return the
/// declared type name (stripped of generics/base list). Returns None otherwise.
fn parse_type_decl(line: &str) -> Option<&str> {
    if line.starts_with("//") || line.starts_with('[') {
        return None;
    }
    // Find a type keyword as a standalone word.
    let mut after_kw: Option<&str> = None;
    for kw in [" class ", " struct ", " interface ", " enum "] {
        if let Some(pos) = line.find(kw) {
            after_kw = Some(&line[pos + kw.len()..]);
            break;
        }
    }
    // Also allow the line to *start* with the keyword (no leading modifier).
    if after_kw.is_none() {
        for kw in ["class ", "struct ", "interface ", "enum "] {
            if let Some(rest) = line.strip_prefix(kw) {
                after_kw = Some(rest);
                break;
            }
        }
    }
    let rest = after_kw?.trim_start();
    // Name runs until whitespace, generic '<', base-list ':', or body '{'.
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '<' || c == ':' || c == '{')
        .unwrap_or(rest.len());
    let name = &rest[..end];
    if name.is_empty() || !name.chars().next().is_some_and(is_ident_start) {
        None
    } else {
        Some(name)
    }
}

/// Parse an instance field line `... <name>; // 0xNN`, returning (name, offset).
/// Static and const fields are skipped (their offset is into static storage, not
/// the instance layout).
fn parse_field_line(line: &str) -> Option<(&str, u64)> {
    let comment = line.find("//")?;
    let offset = parse_hex_after(&line[comment..], "0x")?;
    let decl = line[..comment].trim_end();
    let decl = decl.strip_suffix(';')?;
    // Split off the identifier: last whitespace-separated token.
    let name = decl.rsplit(|c: char| c.is_whitespace()).next()?.trim();
    if name.is_empty() || !name.chars().all(is_ident_char) {
        return None;
    }
    // Skip static/const declarations (instance offsets only).
    let modifiers = &decl[..decl.len() - name.len()];
    if word_present(modifiers, "static") || word_present(modifiers, "const") {
        return None;
    }
    Some((name, offset))
}

/// Parse a `// RVA: 0xNN Offset: ... VA: ...` comment. Returns Some(None) when
/// the RVA is `-1` (no native body), Some(Some(rva)) when present, None when the
/// line is not an RVA comment.
fn parse_rva_comment(line: &str) -> Option<Option<u64>> {
    let rest = line.strip_prefix("// RVA:")?.trim_start();
    if rest.starts_with("-1") {
        return Some(None);
    }
    let token = rest.split_whitespace().next()?;
    let value = token.strip_prefix("0x").unwrap_or(token);
    u64::from_str_radix(value, 16).ok().map(Some)
}

/// Parse a method declaration line, returning (method name, managed param count).
fn parse_method_decl(line: &str) -> Option<(&str, usize)> {
    let open = line.find('(')?;
    let close = matching_paren(line, open)?;
    let head = line[..open].trim_end();
    // Method name is the last identifier before '(' (handles `.ctor`, generics
    // are dropped: `Foo<T>` -> take up to '<').
    let name_start = head
        .rfind(|c: char| c.is_whitespace())
        .map(|i| i + 1)
        .unwrap_or(0);
    let mut name = &head[name_start..];
    if let Some(lt) = name.find('<') {
        name = &name[..lt];
    }
    if name.is_empty()
        || !name
            .chars()
            .next()
            .is_some_and(|c| is_ident_start(c) || c == '.')
    {
        return None;
    }
    let inside = line[open + 1..close].trim();
    let count = if inside.is_empty() {
        0
    } else {
        split_top_level_commas(inside)
    };
    Some((name, count))
}

fn insert_dumper_field(
    fields: &mut HashMap<(String, String), Vec<FieldHit>>,
    type_full: &str,
    name: &str,
    offset: u64,
    wanted_fields: &HashSet<(String, String)>,
) {
    let key_full = (type_full.to_string(), name.to_string());
    let key_bare = (bare_class(type_full).to_string(), name.to_string());
    if !wanted_fields.contains(&key_full) && !wanted_fields.contains(&key_bare) {
        return;
    }
    fields.entry(key_full).or_default().push(FieldHit {
        assembly: String::new(),
        offset,
        size: 0,
    });
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '<' || c == '>'
}
fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '<' || c == '>' || c == '`'
}

/// True if `needle` appears as a whole whitespace-delimited word in `haystack`.
fn word_present(haystack: &str, needle: &str) -> bool {
    haystack.split_whitespace().any(|w| w == needle)
}

/// Find `prefix` (e.g. "0x") in `text` and parse the hex number that follows.
fn parse_hex_after(text: &str, prefix: &str) -> Option<u64> {
    let start = text.find(prefix)? + prefix.len();
    let digits: String = text[start..]
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    u64::from_str_radix(&digits, 16).ok()
}

/// Index of the `)` matching the `(` at `open`.
fn matching_paren(line: &str, open: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Count comma-separated groups at angle/paren-bracket depth 0, i.e. the number
/// of parameters in a (non-empty) argument list.
fn split_top_level_commas(inside: &str) -> usize {
    let mut depth = 0i32;
    let mut count = 1usize;
    for c in inside.chars() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => count += 1,
            _ => {}
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Wanted-name collection (from the manifest)
// ---------------------------------------------------------------------------

/// Build the requested-name sets from a manifest's offline-metadata section so a
/// load can retain only what is used. Internal calls and the string factory
/// contribute method names; offline fields contribute field names. Both the
/// fully-qualified "Namespace.Class" and the bare "Class" spelling are inserted,
/// so a dump that omits namespaces (some Il2CppDumper output) still matches.
pub fn wanted_names(
    runtime: &crate::manifest::Runtime,
) -> (HashSet<(String, String)>, HashSet<(String, String)>) {
    let mut methods: HashSet<(String, String)> = HashSet::new();
    let mut fields: HashSet<(String, String)> = HashSet::new();

    let mut add_method = |namespace: &str, class_name: &str, name: &str| {
        methods.insert((join_type(namespace, class_name), name.to_string()));
        methods.insert((class_name.to_string(), name.to_string()));
    };

    for method in &runtime.offline_methods {
        add_method(&method.namespace, &method.class_name, &method.name);
    }
    if let Some(factory) = &runtime.string_factory {
        add_method(&factory.namespace, &factory.class_name, &factory.name);
    }
    for icall in &runtime.offline_internal_calls {
        if let Ok((type_full, candidates)) = icall_candidates(&icall.name) {
            let bare = bare_class(&type_full).to_string();
            for candidate in candidates {
                methods.insert((type_full.clone(), candidate.clone()));
                methods.insert((bare.clone(), candidate));
            }
        }
    }
    for field in &runtime.offline_fields {
        fields.insert((
            join_type(&field.namespace, &field.class_name),
            field.name.clone(),
        ));
        fields.insert((field.class_name.clone(), field.name.clone()));
    }
    (methods, fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn wanted(
        methods: &[(&str, &str)],
        fields: &[(&str, &str)],
    ) -> (HashSet<(String, String)>, HashSet<(String, String)>) {
        let m = methods
            .iter()
            .map(|(t, n)| (t.to_string(), n.to_string()))
            .collect();
        let f = fields
            .iter()
            .map(|(t, n)| (t.to_string(), n.to_string()))
            .collect();
        (m, f)
    }

    fn tmp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("iaot_md_test_{}", rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn rand_suffix() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }
    fn write(path: &Path, contents: &str) {
        let mut f = File::create(path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn cpp2il_resolves_method_field_and_icall() {
        let dir = tmp_dir();
        // Two overloads at different addresses; a field; an internal-call thunk.
        let methods = r#"{"assembly":"UnityEngine.CoreModule","declaringType":"UnityEngine.Camera","name":"WorldToScreenPoint","address":4295000000,"parameters":[{"type":"UnityEngine.Vector3"}]}
{"assembly":"UnityEngine.CoreModule","declaringType":"UnityEngine.Camera","name":"WorldToScreenPoint","address":4295000100,"parameters":[{"type":"UnityEngine.Vector3"},{"type":"UnityEngine.Camera.MonoOrStereoscopicEye"}]}
{"assembly":"UnityEngine.CoreModule","declaringType":"UnityEngine.Camera","name":"INTERNAL_CALL_WorldToScreenPoint","address":4295000200,"parameters":[]}
{"assembly":"Assembly-CSharp","declaringType":"Game.ZeroBased","name":"Tick","address":34081736,"parameters":[]}
{"assembly":"noise","declaringType":"UnityEngine.Camera","name":"NoBody","address":0,"parameters":[]}
"#;
        let types = r#"{"assembly":"Assembly-CSharp","fullName":"GameBase.Pawn","fields":[{"name":"m_HeadBone","offset":352,"size":8},{"name":"m_IsAlive","offset":64,"size":1}]}
"#;
        write(&dir.join(CPP2IL_METHODS_FILE), methods);
        write(&dir.join(CPP2IL_TYPES_FILE), types);

        let (wm, wf) = wanted(
            &[
                ("UnityEngine.Camera", "WorldToScreenPoint"),
                ("UnityEngine.Camera", "INTERNAL_CALL_WorldToScreenPoint"),
                ("Game.ZeroBased", "Tick"),
            ],
            &[
                ("GameBase.Pawn", "m_HeadBone"),
                ("GameBase.Pawn", "m_IsAlive"),
            ],
        );
        let idx = MetadataIndex::load(&dir, None, &wm, &wf).unwrap();

        // Overload disambiguation by arity.
        assert_eq!(
            idx.method_rva(
                "UnityEngine.CoreModule",
                "UnityEngine",
                "Camera",
                "WorldToScreenPoint",
                1,
                None
            )
            .unwrap(),
            4295000000 - IMAGE_BASE
        );
        assert_eq!(
            idx.method_rva(
                "UnityEngine.CoreModule",
                "UnityEngine",
                "Camera",
                "WorldToScreenPoint",
                2,
                None
            )
            .unwrap(),
            4295000100 - IMAGE_BASE
        );
        // Ambiguous without a discriminator.
        assert!(idx
            .method_rva(
                "UnityEngine.CoreModule",
                "UnityEngine",
                "Camera",
                "WorldToScreenPoint",
                -1,
                None
            )
            .is_err());
        // Mach-O images with vmaddr 0 already report an RVA in `address`.
        assert_eq!(
            idx.method_rva("Assembly-CSharp", "Game", "ZeroBased", "Tick", 0, None)
                .unwrap(),
            34081736
        );
        // Field offset + size.
        assert_eq!(
            idx.field("Assembly-CSharp", "GameBase", "Pawn", "m_HeadBone")
                .unwrap(),
            (352, 8)
        );
        assert_eq!(
            idx.field("Assembly-CSharp", "GameBase", "Pawn", "m_IsAlive")
                .unwrap(),
            (64, 1)
        );
        // Internal-call thunk resolution.
        assert_eq!(
            idx.icall_rva("UnityEngine.Camera::WorldToScreenPoint_Injected")
                .unwrap(),
            4295000200 - IMAGE_BASE
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn il2cpp_dumper_script_resolves_methods() {
        let dir = tmp_dir();
        let script = r#"{
  "ScriptMethod": [
    { "Address": 5000000, "Name": "GameBase.Pawn$$IsLocalPlayer", "Signature": "bool GameBase_Pawn_IsLocalPlayer (Pawn_o* __this, const MethodInfo* method);", "TypeSignature": "iii" },
    { "Address": 5000400, "Name": "UnityEngine.Camera$$WorldToScreenPoint", "Signature": "Vector3 UnityEngine_Camera_WorldToScreenPoint (Camera_o* __this, Vector3 position, const MethodInfo* method);", "TypeSignature": "iiii" }
  ],
  "ScriptString": []
}"#;
        write(&dir.join(DUMPER_SCRIPT_FILE), script);

        let (wm, wf) = wanted(
            &[
                ("GameBase.Pawn", "IsLocalPlayer"),
                ("UnityEngine.Camera", "WorldToScreenPoint"),
            ],
            &[],
        );
        let idx = MetadataIndex::load(&dir, None, &wm, &wf).unwrap();
        assert_eq!(
            idx.method_rva("", "GameBase", "Pawn", "IsLocalPlayer", 0, None)
                .unwrap(),
            5000000
        );
        // One managed param (position); __this and method are excluded.
        assert_eq!(
            idx.method_rva("", "UnityEngine", "Camera", "WorldToScreenPoint", 1, None)
                .unwrap(),
            5000400
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn il2cpp_dumper_script_without_dump_cs_rejects_fields() {
        let dir = tmp_dir();
        write(&dir.join(DUMPER_SCRIPT_FILE), r#"{"ScriptMethod":[]}"#);
        let (wm, wf) = wanted(&[], &[("GameBase.Pawn", "m_HeadBone")]);
        let err = match MetadataIndex::load(&dir, None, &wm, &wf) {
            Ok(_) => panic!("expected an error for offline_fields without dump.cs"),
            Err(e) => e.to_string(),
        };
        assert!(err.contains("offline_fields"), "unexpected error: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn il2cpp_dumper_dump_cs_resolves_methods_and_fields() {
        let dir = tmp_dir();
        let dump = r#"
// Namespace: GameBase
public class Pawn : MonoBehaviour // TypeDefIndex: 4321
{
	// Fields
	private Transform m_HeadBone; // 0x160
	public bool m_IsAlive; // 0x40
	private static int s_Count; // 0x0

	// Methods

	// RVA: 0x4C4DB0 Offset: 0x4C4DB0 VA: 0x1004C4DB0
	public bool IsLocalPlayer() { }

	// RVA: -1 Offset: -1 VA: 0x0
	public extern Transform get_Root();
}

// Namespace: UnityEngine
public sealed class Camera : Behaviour // TypeDefIndex: 55
{
	// Methods

	// RVA: 0x8BB077C Offset: 0x8BB077C VA: 0x108BB077C Slot: 4
	public Vector3 WorldToScreenPoint(Vector3 position) { }
}
"#;
        write(&dir.join(DUMPER_DUMP_FILE), dump);
        let (wm, wf) = wanted(
            &[
                ("GameBase.Pawn", "IsLocalPlayer"),
                ("UnityEngine.Camera", "WorldToScreenPoint"),
            ],
            &[
                ("GameBase.Pawn", "m_HeadBone"),
                ("GameBase.Pawn", "m_IsAlive"),
            ],
        );
        let idx = MetadataIndex::load(&dir, None, &wm, &wf).unwrap();
        assert_eq!(
            idx.method_rva("", "GameBase", "Pawn", "IsLocalPlayer", 0, None)
                .unwrap(),
            0x4C4DB0
        );
        assert_eq!(
            idx.method_rva("", "UnityEngine", "Camera", "WorldToScreenPoint", 1, None)
                .unwrap(),
            0x8BB077C
        );
        // Instance fields resolve; static field is ignored even if requested.
        assert_eq!(
            idx.field("", "GameBase", "Pawn", "m_HeadBone").unwrap().0,
            0x160
        );
        assert_eq!(
            idx.field("", "GameBase", "Pawn", "m_IsAlive").unwrap().0,
            0x40
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detects_by_direct_file_path() {
        let dir = tmp_dir();
        write(
            &dir.join(DUMPER_SCRIPT_FILE),
            r#"{"ScriptMethod":[{"Address":42,"Name":"A.B$$C","Signature":"void A_B_C (B_o* __this, const MethodInfo* method);"}]}"#,
        );
        let (wm, wf) = wanted(&[("A.B", "C")], &[]);
        // Point directly at the file, not the directory.
        let idx = MetadataIndex::load(&dir.join(DUMPER_SCRIPT_FILE), None, &wm, &wf).unwrap();
        assert_eq!(idx.method_rva("", "A", "B", "C", 0, None).unwrap(), 42);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_format_hint_errors() {
        assert!(MetadataFormat::from_hint(Some("nonsense")).is_err());
        assert_eq!(MetadataFormat::from_hint(Some("auto")).unwrap(), None);
        assert_eq!(MetadataFormat::from_hint(None).unwrap(), None);
        assert_eq!(
            MetadataFormat::from_hint(Some("cpp2il")).unwrap(),
            Some(MetadataFormat::Cpp2ilPackage)
        );
        assert_eq!(
            MetadataFormat::from_hint(Some("il2cpp_dumper")).unwrap(),
            Some(MetadataFormat::Il2CppDumper)
        );
    }
}
