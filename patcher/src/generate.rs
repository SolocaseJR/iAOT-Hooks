use crate::manifest::*;
use std::{fmt::Write, fs, path::Path};

fn ident(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn c_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

pub fn generate(m: &Manifest, manifest_path: &Path) -> anyhow::Result<()> {
    let dir = manifest_path.parent().unwrap_or(Path::new("."));
    let out = dir.join("generated");
    fs::create_dir_all(&out)?;

    let mut c = String::from("#pragma once\n#include <IAOT/IAOT.hpp>\n#include <cstdint>\n\n");
    writeln!(c, "enum class IAOTFlag : std::uint32_t {{")?;
    for x in &m.config.flags {
        writeln!(c, "    {} = {},", ident(&x.name), x.index)?;
    }
    writeln!(c, "}};")?;
    writeln!(c, "enum class IAOTString : std::uint32_t {{")?;
    for x in &m.config.strings {
        writeln!(c, "    {} = {},", ident(&x.name), x.index)?;
    }
    writeln!(c, "}};")?;
    writeln!(c, "enum class IAOTCallback : std::uint32_t {{")?;
    for x in &m.config.callbacks {
        writeln!(c, "    {} = {},", ident(&x.name), x.index)?;
    }
    writeln!(c, "}};")?;
    for x in &m.config.flags {
        writeln!(
            c,
            "inline constexpr bool IAOTDefaultFlag_{} = {};",
            ident(&x.name),
            x.default
        )?;
    }
    for x in &m.config.strings {
        writeln!(
            c,
            "inline constexpr const char* IAOTDefaultString_{} = OBF(\"{}\");",
            ident(&x.name),
            c_escape(&x.default)
        )?;
    }
    for x in &m.config.callbacks {
        writeln!(
            c,
            "inline constexpr const char* IAOTCallbackSymbol_{} = OBF(\"{}\");",
            ident(&x.name),
            c_escape(&x.symbol)
        )?;
    }
    c.push_str("inline bool IAOTGetFlag(IAOTFlag v){return iaot::GetBool(static_cast<std::uint32_t>(v));}\ninline void IAOTSetFlag(IAOTFlag v,bool x){iaot::SetBool(static_cast<std::uint32_t>(v),x);}\ninline const char* IAOTGetString(IAOTString v){return iaot::GetString(static_cast<std::uint32_t>(v));}\ninline void IAOTSetString(IAOTString v,const char* x){iaot::SetString(static_cast<std::uint32_t>(v),x);}\ninline std::uintptr_t IAOTGetCallback(IAOTCallback v){return iaot::GetCallback(static_cast<std::uint32_t>(v));}\ninline void IAOTSetCallback(IAOTCallback v,std::uintptr_t x){iaot::SetCallback(static_cast<std::uint32_t>(v),x);}\n");
    fs::write(out.join("IAOTGeneratedConfig.hpp"), c)?;

    let mut f = String::from(
        "#pragma once\n#include <IAOT/IAOT.hpp>\n#include <string>\nnamespace iaot::generated {\n",
    );
    for x in &m.functions {
        if x.kind.as_deref() == Some("objc") {
            continue;
        }
        let sig = x.signature.as_ref().unwrap();
        let a = x.address.as_ref().unwrap();
        match a.kind.as_str() {
            "vmaddr" => writeln!(
                f,
                "inline auto {}(){{return iaot::AppFunction<{}>::FromVmAddr(0x{:X}ULL);}}",
                x.name,
                sig,
                a.value.unwrap_or(0)
            )?,
            "rva" => writeln!(
                f,
                "inline auto {}(){{return iaot::AppFunction<{}>::FromRva(0x{:X}ULL);}}",
                x.name,
                sig,
                a.value.unwrap_or(0)
            )?,
            "runtime" => writeln!(
                f,
                "inline auto {}(){{return iaot::AppFunction<{}>::FromRuntime(0x{:X}ULL);}}",
                x.name,
                sig,
                a.value.unwrap_or(0)
            )?,
            "symbol" => writeln!(
                f,
                "inline auto {}(){{return iaot::AppFunction<{}>::FromSymbol(OBF(\"{}\"));}}",
                x.name,
                sig,
                c_escape(a.symbol.as_deref().unwrap_or(""))
            )?,
            _ => anyhow::bail!("unsupported function address type {}", a.kind),
        }
    }
    writeln!(f, "}}")?;
    fs::write(out.join("IAOTGeneratedFunctions.hpp"), f)?;

    let mut h = String::from("#include <IAOT/IAOT.hpp>\n#include \"IAOTGeneratedConfig.hpp\"\n#include \"IAOTGeneratedFunctions.hpp\"\n");
    for x in &m.generated_hooks {
        writeln!(
            h,
            "extern \"C\" bool {}(const iaot::HookContext& ctx);",
            x.symbol
        )?;
    }
    writeln!(h, "namespace iaot {{ void RegisterGeneratedHooks() {{")?;
    for x in &m.generated_hooks {
        writeln!(
            h,
            "    RegisterHook({{ OBF(\"{}\"), {} }});",
            c_escape(&x.name),
            x.symbol
        )?;
    }
    writeln!(h, "}} }}")?;
    fs::write(out.join("IAOTGeneratedHooks.mm"), h)?;
    Ok(())
}
