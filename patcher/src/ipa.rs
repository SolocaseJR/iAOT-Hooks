use std::{fs::File, path::Path};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};
pub fn repack(input: &Path, output: &Path, member: &str, replacement: &Path) -> anyhow::Result<()> {
    if member.contains("..") || member.starts_with('/') {
        anyhow::bail!("unsafe IPA member path")
    }
    let src = File::open(input)?;
    let mut zip = ZipArchive::new(src)?;
    if zip.by_name(member).is_err() {
        anyhow::bail!("member not found: {member}")
    }
    let dst = File::create(output)?;
    let mut writer = ZipWriter::new(dst);
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
                let mut r = File::open(replacement)?;
                std::io::copy(&mut r, &mut writer)?;
            } else {
                std::io::copy(&mut file, &mut writer)?;
            }
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
    #[test]
    fn replaces_one() {
        let d = tempdir().unwrap();
        let a = d.path().join("a.zip");
        let mut w = ZipWriter::new(File::create(&a).unwrap());
        w.start_file(
            "Payload/D.app/D",
            SimpleFileOptions::default().unix_permissions(0o755),
        )
        .unwrap();
        w.write_all(b"old").unwrap();
        w.start_file("keep", SimpleFileOptions::default()).unwrap();
        w.write_all(b"same").unwrap();
        w.finish().unwrap();
        let p = d.path().join("p");
        std::fs::write(&p, b"new").unwrap();
        let out = d.path().join("o.zip");
        repack(&a, &out, "Payload/D.app/D", &p).unwrap();
        let mut z = ZipArchive::new(File::open(out).unwrap()).unwrap();
        let mut s = String::new();
        z.by_name("Payload/D.app/D")
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        assert_eq!(s, "new");
    }
}
