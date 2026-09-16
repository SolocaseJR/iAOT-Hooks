use std::{collections::HashMap, fs::File, path::Path};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

fn safe_member(member: &str) -> bool {
    !member.contains("..") && !member.starts_with('/') && !member.ends_with('/')
}
pub fn app_bundle_root(member: &str) -> anyhow::Result<String> {
    let mut parts = Vec::new();
    for part in member.split('/') {
        parts.push(part);
        if part.ends_with(".app") {
            let root = parts.join("/");
            if root.starts_with("Payload/") && !root.contains("..") && !root.contains('\\') {
                return Ok(root);
            }
            break;
        }
    }
    anyhow::bail!("member is not inside a safe Payload/*.app bundle")
}
pub fn find_unity_framework_member(input: &Path) -> anyhow::Result<Option<String>> {
    let mut zip = ZipArchive::new(File::open(input)?)?;
    let mut matches = Vec::new();
    for i in 0..zip.len() {
        let file = zip.by_index(i)?;
        let name = file.name();
        if name.starts_with("Payload/")
            && name.ends_with("/Frameworks/UnityFramework.framework/UnityFramework")
        {
            matches.push(name.to_owned());
        }
    }
    matches.sort();
    matches.dedup();
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.pop()),
        count => {
            anyhow::bail!("IPA contains {count} UnityFramework candidates; use --member explicitly")
        }
    }
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
pub fn extract_all(input: &Path, output: &Path) -> anyhow::Result<()> {
    if output.exists() && output.read_dir()?.next().is_some() {
        anyhow::bail!("extraction directory must be empty: {}", output.display())
    }
    std::fs::create_dir_all(output)?;
    let mut zip = ZipArchive::new(File::open(input)?)?;
    for index in 0..zip.len() {
        let mut source = zip.by_index(index)?;
        let enclosed = source
            .enclosed_name()
            .ok_or_else(|| anyhow::anyhow!("unsafe IPA member path: {}", source.name()))?;
        let destination = output.join(enclosed);
        if source.is_dir() {
            std::fs::create_dir_all(&destination)?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(&destination)?;
        std::io::copy(&mut source, &mut file)?;
        #[cfg(unix)]
        if let Some(mode) = source.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(mode))?;
        }
    }
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
    let replacements = [(member, replacement)];
    let additions = addition.into_iter().collect::<Vec<_>>();
    repack_many(input, output, &replacements, &additions)
}

pub fn repack_many(
    input: &Path,
    output: &Path,
    replacements: &[(&str, &Path)],
    additions: &[(&str, &Path)],
) -> anyhow::Result<()> {
    if input == output {
        anyhow::bail!("input and output IPA paths must differ")
    }
    if replacements.is_empty() {
        anyhow::bail!("at least one IPA replacement is required")
    }
    if replacements.iter().any(|(name, _)| !safe_member(name))
        || additions.iter().any(|(name, _)| !safe_member(name))
    {
        anyhow::bail!("unsafe IPA member path")
    }
    let replacement_map: HashMap<_, _> = replacements.iter().copied().collect();
    let addition_map: HashMap<_, _> = additions.iter().copied().collect();
    if replacement_map.len() != replacements.len() || addition_map.len() != additions.len() {
        anyhow::bail!("duplicate IPA replacement or addition member")
    }
    for (_, path) in replacements.iter().chain(additions) {
        if !path.is_file() {
            anyhow::bail!("IPA input artifact does not exist: {}", path.display())
        }
    }
    let mut zip = ZipArchive::new(File::open(input)?)?;
    for (member, _) in replacements {
        if zip.by_name(member).is_err() {
            anyhow::bail!("member not found: {member}")
        }
    }
    let parent = output.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut writer = ZipWriter::new(temporary.as_file_mut());
    let mut replacement_seen = std::collections::HashSet::new();
    let mut addition_seen = std::collections::HashSet::new();
    for i in 0..zip.len() {
        let file = zip.by_index(i)?;
        let name = file.name().to_owned();
        if file.is_dir() {
            let options = SimpleFileOptions::default()
                .last_modified_time(file.last_modified().unwrap_or_default())
                .unix_permissions(file.unix_mode().unwrap_or(0o755));
            writer.add_directory(name, options)?;
            continue;
        }
        if let Some(replacement) = replacement_map.get(name.as_str()) {
            let options = SimpleFileOptions::default()
                .compression_method(file.compression())
                .last_modified_time(file.last_modified().unwrap_or_default())
                .unix_permissions(file.unix_mode().unwrap_or(0o644));
            writer.start_file(&name, options)?;
            let mut source = File::open(replacement)?;
            std::io::copy(&mut source, &mut writer)?;
            replacement_seen.insert(name.clone());
        } else if let Some(addition) = addition_map.get(name.as_str()) {
            let options = SimpleFileOptions::default()
                .compression_method(file.compression())
                .last_modified_time(file.last_modified().unwrap_or_default())
                .unix_permissions(file.unix_mode().unwrap_or(0o644));
            writer.start_file(&name, options)?;
            let mut source = File::open(addition)?;
            std::io::copy(&mut source, &mut writer)?;
            addition_seen.insert(name.clone());
        } else {
            // Copy the already-compressed bytes directly instead of
            // decompressing then recompressing every untouched entry. Most
            // of an IPA (Unity asset bundles, embedded frameworks, etc.) is
            // not part of a patch pass, so decompress+recompress of every
            // one of those multi-gigabyte entries was the dominant cost of
            // repackaging; raw_copy_file skips both codec passes for
            // anything that isn't actually being replaced.
            writer.raw_copy_file(file)?;
        }
    }
    if replacement_seen.len() != replacement_map.len() {
        anyhow::bail!("not every requested IPA member was replaced")
    }
    for (name, path) in additions {
        if !addition_seen.contains(*name) {
            writer.start_file(
                *name,
                SimpleFileOptions::default()
                    .compression_method(CompressionMethod::Deflated)
                    .unix_permissions(0o755),
            )?;
            let mut source = File::open(path)?;
            std::io::copy(&mut source, &mut writer)?;
        }
    }
    writer.finish()?;
    temporary.as_file_mut().sync_all()?;
    temporary.persist(output).map_err(|error| error.error)?;
    Ok(())
}

/// Publishes a fully validated artifact without exposing a partially copied file.
/// Existing output is replaced only after the staged copy has been flushed.
pub fn publish_file(source: &Path, output: &Path) -> anyhow::Result<()> {
    if source == output {
        anyhow::bail!("publish source and destination must differ")
    }
    if !source.is_file() {
        anyhow::bail!("artifact to publish does not exist: {}", source.display())
    }
    let parent = output.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    // The staged artifact and the output directory are normally on the same
    // volume (both under the project tree), so a rename is an atomic,
    // near-instant directory-entry update instead of copying every byte of a
    // multi-gigabyte IPA a second time. Only fall back to the copy-then-
    // persist path (still atomic, via a same-directory temp file) when the
    // rename can't cross filesystems (e.g. a work_dir on another drive).
    if std::fs::rename(source, output).is_ok() {
        return Ok(());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut input = File::open(source)?;
    std::io::copy(&mut input, temporary.as_file_mut())?;
    temporary.as_file_mut().sync_all()?;
    temporary.persist(output).map_err(|error| error.error)?;
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
    #[test]
    fn detects_unity_member_and_app_root() {
        let directory = tempdir().unwrap();
        let input = directory.path().join("unity.ipa");
        let mut writer = ZipWriter::new(File::create(&input).unwrap());
        writer
            .start_file(
                "Payload/D.app/Frameworks/UnityFramework.framework/UnityFramework",
                SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"framework").unwrap();
        writer.finish().unwrap();
        let member = find_unity_framework_member(&input).unwrap().unwrap();
        assert_eq!(
            member,
            "Payload/D.app/Frameworks/UnityFramework.framework/UnityFramework"
        );
        assert_eq!(app_bundle_root(&member).unwrap(), "Payload/D.app");
    }

    #[test]
    fn publishes_only_a_complete_file() {
        let directory = tempdir().unwrap();
        let source = directory.path().join("staged.ipa");
        let output = directory.path().join("final.ipa");
        std::fs::write(&source, b"complete").unwrap();
        std::fs::write(&output, b"old").unwrap();
        publish_file(&source, &output).unwrap();
        assert_eq!(std::fs::read(output).unwrap(), b"complete");
    }
}
