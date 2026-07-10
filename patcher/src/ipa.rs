use std::{fs::File, path::Path};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

fn safe_member(member: &str) -> bool {
    !member.contains("..") && !member.starts_with('/') && !member.ends_with('/')
}
pub fn extract_member(input: &Path, member: &str, output: &Path) -> anyhow::Result<()> {
    if !safe_member(member) {
        anyhow::bail!("unsafe IPA member path")
    }
    let mut zip = ZipArchive::new(File::open(input)?)?;
    let mut source = zip
        .by_name(member)
        .map_err(|_| anyhow::anyhow!("member not found: {member}"))?;
    let mut destination = File::create(output)?;
    std::io::copy(&mut source, &mut destination)?;
    Ok(())
}
pub fn repack(input: &Path, output: &Path, member: &str, replacement: &Path) -> anyhow::Result<()> {
    repack_with_addition(input, output, member, replacement, None)
}
pub fn repack_with_addition(
    input: &Path,
    output: &Path,
    member: &str,
    replacement: &Path,
    addition: Option<(&str, &Path)>,
) -> anyhow::Result<()> {
    if input == output {
        anyhow::bail!("input and output IPA paths must differ")
    }
    if !safe_member(member) || addition.is_some_and(|x| !safe_member(x.0)) {
        anyhow::bail!("unsafe IPA member path")
    }
    let mut zip = ZipArchive::new(File::open(input)?)?;
    if zip.by_name(member).is_err() {
        anyhow::bail!("member not found: {member}")
    }
    let mut writer = ZipWriter::new(File::create(output)?);
    let mut addition_seen = false;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let name = file.name().to_owned();
        let options = SimpleFileOptions::default()
            .compression_method(file.compression())
            .last_modified_time(file.last_modified().unwrap_or_default())
            .unix_permissions(file.unix_mode().unwrap_or(if file.is_dir() {
                0o755
            } else {
                0o644
            }));
        if file.is_dir() {
            writer.add_directory(name, options)?;
        } else {
            writer.start_file(&name, options)?;
            if name == member {
                let mut source = File::open(replacement)?;
                std::io::copy(&mut source, &mut writer)?;
            } else if addition.is_some_and(|x| x.0 == name) {
                let mut source = File::open(addition.unwrap().1)?;
                std::io::copy(&mut source, &mut writer)?;
                addition_seen = true;
            } else {
                std::io::copy(&mut file, &mut writer)?;
            }
        }
    }
    if let Some((name, path)) = addition {
        if !addition_seen {
            writer.start_file(
                name,
                SimpleFileOptions::default()
                    .compression_method(CompressionMethod::Deflated)
                    .unix_permissions(0o755),
            )?;
            let mut source = File::open(path)?;
            std::io::copy(&mut source, &mut writer)?;
        }
    }
    writer.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use tempfile::tempdir;
    fn fixture(path: &Path) {
        let mut w = ZipWriter::new(File::create(path).unwrap());
        w.start_file(
            "Payload/D.app/D",
            SimpleFileOptions::default().unix_permissions(0o755),
        )
        .unwrap();
        w.write_all(b"old").unwrap();
        w.start_file("keep", SimpleFileOptions::default()).unwrap();
        w.write_all(b"same").unwrap();
        w.finish().unwrap();
    }
    #[test]
    fn replaces_one() {
        let d = tempdir().unwrap();
        let input = d.path().join("a.zip");
        fixture(&input);
        let patched = d.path().join("p");
        std::fs::write(&patched, b"new").unwrap();
        let out = d.path().join("o.zip");
        repack(&input, &out, "Payload/D.app/D", &patched).unwrap();
        let mut z = ZipArchive::new(File::open(out).unwrap()).unwrap();
        let mut s = String::new();
        z.by_name("Payload/D.app/D")
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        assert_eq!(s, "new");
    }
    #[test]
    fn adds_dylib_and_extracts() {
        let d = tempdir().unwrap();
        let input = d.path().join("a.zip");
        fixture(&input);
        let patched = d.path().join("p");
        let dylib = d.path().join("x");
        std::fs::write(&patched, b"new").unwrap();
        std::fs::write(&dylib, b"dylib").unwrap();
        let out = d.path().join("o.zip");
        repack_with_addition(
            &input,
            &out,
            "Payload/D.app/D",
            &patched,
            Some(("Payload/D.app/Frameworks/X.dylib", &dylib)),
        )
        .unwrap();
        let extracted = d.path().join("e");
        extract_member(&out, "Payload/D.app/D", &extracted).unwrap();
        assert_eq!(std::fs::read(extracted).unwrap(), b"new");
        let mut z = ZipArchive::new(File::open(out).unwrap()).unwrap();
        let mut s = String::new();
        z.by_name("Payload/D.app/Frameworks/X.dylib")
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        assert_eq!(s, "dylib");
    }
}
