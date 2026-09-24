use crate::manifest::Manifest;
use anyhow::Context;
use image::{
    imageops::FilterType, DynamicImage, GenericImageView, ImageFormat, ImageReader, Rgb, RgbImage,
};
use plist::{Dictionary, Value};
use std::{
    fs::{self, File},
    io::Cursor,
    path::{Path, PathBuf},
};

const IPHONE_ICON_BASES: &[&str] = &[
    "IAOTAppIcon20x20",
    "IAOTAppIcon29x29",
    "IAOTAppIcon40x40",
    "IAOTAppIcon60x60",
];
const IPAD_ICON_BASES: &[&str] = &[
    "IAOTAppIcon20x20",
    "IAOTAppIcon29x29",
    "IAOTAppIcon40x40",
    "IAOTAppIcon76x76",
    "IAOTAppIcon83.5x83.5",
];
const ICON_OUTPUTS: &[(&str, u32)] = &[
    ("IAOTAppIcon20x20@2x.png", 40),
    ("IAOTAppIcon20x20@3x.png", 60),
    ("IAOTAppIcon29x29@2x.png", 58),
    ("IAOTAppIcon29x29@3x.png", 87),
    ("IAOTAppIcon40x40@2x.png", 80),
    ("IAOTAppIcon40x40@3x.png", 120),
    ("IAOTAppIcon60x60@2x.png", 120),
    ("IAOTAppIcon60x60@3x.png", 180),
    ("IAOTAppIcon20x20~ipad.png", 20),
    ("IAOTAppIcon20x20@2x~ipad.png", 40),
    ("IAOTAppIcon29x29~ipad.png", 29),
    ("IAOTAppIcon29x29@2x~ipad.png", 58),
    ("IAOTAppIcon40x40~ipad.png", 40),
    ("IAOTAppIcon40x40@2x~ipad.png", 80),
    ("IAOTAppIcon76x76~ipad.png", 76),
    ("IAOTAppIcon76x76@2x~ipad.png", 152),
    ("IAOTAppIcon83.5x83.5@2x~ipad.png", 167),
];

#[derive(Debug, Default)]
pub struct BrandingArtifacts {
    pub replacements: Vec<(String, PathBuf)>,
    pub additions: Vec<(String, PathBuf)>,
    pub display_name: Option<String>,
    pub icon_members: Vec<String>,
}

fn read_plist(path: &Path) -> anyhow::Result<Value> {
    let bytes =
        fs::read(path).with_context(|| format!("cannot read property list {}", path.display()))?;
    Value::from_reader(Cursor::new(bytes))
        .with_context(|| format!("cannot parse property list {}", path.display()))
}

fn write_binary_plist(value: &Value, path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    value
        .to_writer_binary(File::create(path)?)
        .with_context(|| format!("cannot write property list {}", path.display()))
}

fn dictionary(value: &mut Value) -> anyhow::Result<&mut Dictionary> {
    value
        .as_dictionary_mut()
        .ok_or_else(|| anyhow::anyhow!("property-list root is not a dictionary"))
}

fn nested_dictionary<'a>(dictionary: &'a mut Dictionary, key: &str) -> &'a mut Dictionary {
    if !matches!(dictionary.get(key), Some(Value::Dictionary(_))) {
        dictionary.insert(key.into(), Value::Dictionary(Dictionary::new()));
    }
    dictionary
        .get_mut(key)
        .and_then(Value::as_dictionary_mut)
        .expect("dictionary entry was inserted immediately above")
}

fn strings(values: &[&str]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|value| Value::String((*value).into()))
            .collect(),
    )
}

fn configure_primary_icon(root: &mut Dictionary, key: &str, files: &[&str]) {
    let icons = nested_dictionary(root, key);
    let primary = nested_dictionary(icons, "CFBundlePrimaryIcon");
    primary.insert("CFBundleIconFiles".into(), strings(files));
    // CFBundleIconName selects a compiled Assets.car catalog and would take
    // precedence over the generated standalone PNG set.
    primary.remove("CFBundleIconName");
}

fn configure_icon_plist(root: &mut Dictionary) {
    let mut legacy = IPHONE_ICON_BASES.to_vec();
    legacy.extend(IPAD_ICON_BASES.iter().copied());
    legacy.sort_unstable();
    legacy.dedup();
    root.insert("CFBundleIconFiles".into(), strings(&legacy));
    root.remove("CFBundleIconName");
    configure_primary_icon(root, "CFBundleIcons", IPHONE_ICON_BASES);
    configure_primary_icon(root, "CFBundleIcons~ipad", IPAD_ICON_BASES);
}

fn opaque_source(image: DynamicImage) -> DynamicImage {
    let rgba = image.to_rgba8();
    let output = RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let pixel = rgba.get_pixel(x, y).0;
        let alpha = u32::from(pixel[3]);
        let blend = |channel: u8| ((u32::from(channel) * alpha + 255 * (255 - alpha)) / 255) as u8;
        Rgb([blend(pixel[0]), blend(pixel[1]), blend(pixel[2])])
    });
    DynamicImage::ImageRgb8(output)
}

fn load_icon(path: &Path) -> anyhow::Result<DynamicImage> {
    let reader = ImageReader::open(path)
        .with_context(|| format!("cannot open custom app icon {}", path.display()))?
        .with_guessed_format()
        .with_context(|| format!("cannot detect custom app icon format {}", path.display()))?;
    if reader.format() != Some(ImageFormat::Png) {
        anyhow::bail!("custom app icon must be a PNG file")
    }
    let image = reader
        .decode()
        .with_context(|| format!("cannot decode custom app icon {}", path.display()))?;
    let (width, height) = image.dimensions();
    if width != height {
        anyhow::bail!(
            "custom app icon must be square; got {width}x{height} at {}",
            path.display()
        )
    }
    if width < 180 {
        anyhow::bail!("custom app icon must be at least 180x180 pixels; got {width}x{height}")
    }
    Ok(opaque_source(image))
}

fn add_localized_names(
    extracted: &Path,
    output: &Path,
    app_bundle: &str,
    display_name: &str,
    replacements: &mut Vec<(String, PathBuf)>,
) -> anyhow::Result<()> {
    let app_root = extracted.join(Path::new(app_bundle));
    for entry in fs::read_dir(&app_root)
        .with_context(|| format!("cannot enumerate app bundle {}", app_root.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir()
            || !entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(".lproj")
        {
            continue;
        }
        let source = entry.path().join("InfoPlist.strings");
        if !source.is_file() {
            continue;
        }
        let mut value = read_plist(&source)?;
        dictionary(&mut value)?.insert(
            "CFBundleDisplayName".into(),
            Value::String(display_name.into()),
        );
        let relative = Path::new("localized")
            .join(entry.file_name())
            .join("InfoPlist.strings");
        let destination = output.join(&relative);
        write_binary_plist(&value, &destination)?;
        replacements.push((
            format!(
                "{app_bundle}/{}/InfoPlist.strings",
                entry.file_name().to_string_lossy()
            ),
            destination,
        ));
    }
    Ok(())
}

pub fn prepare(
    manifest: &Manifest,
    app_bundle: &str,
    info_plist_member: &str,
    extracted: &Path,
    output: &Path,
) -> anyhow::Result<BrandingArtifacts> {
    let display_name = manifest
        .app
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let icon_path = manifest.app.icon_path.as_ref().map(|configured| {
        if configured.is_absolute() {
            configured.clone()
        } else {
            manifest.source_dir.join(configured)
        }
    });
    if display_name.is_none() && icon_path.is_none() && !manifest.app.remove_supported_devices {
        return Ok(BrandingArtifacts::default());
    }

    fs::create_dir_all(output)?;
    let info_source = extracted.join(Path::new(info_plist_member));
    let mut info = read_plist(&info_source)?;
    {
        let root = dictionary(&mut info)?;
        if manifest.app.remove_supported_devices {
            root.remove("UISupportedDevices");
        }
        if let Some(name) = &display_name {
            root.insert("CFBundleDisplayName".into(), Value::String(name.clone()));
        }
        if icon_path.is_some() {
            configure_icon_plist(root);
        }
    }
    let info_output = output.join("Info.plist");
    write_binary_plist(&info, &info_output)?;

    let mut artifacts = BrandingArtifacts {
        replacements: vec![(info_plist_member.into(), info_output)],
        additions: Vec::new(),
        display_name: display_name.clone(),
        icon_members: Vec::new(),
    };
    if let Some(name) = &display_name {
        add_localized_names(
            extracted,
            output,
            app_bundle,
            name,
            &mut artifacts.replacements,
        )?;
    }
    if let Some(path) = icon_path {
        let source = load_icon(&path)?;
        let icons_dir = output.join("icons");
        fs::create_dir_all(&icons_dir)?;
        for &(filename, pixels) in ICON_OUTPUTS {
            let destination = icons_dir.join(filename);
            source
                .resize_exact(pixels, pixels, FilterType::Lanczos3)
                .to_rgb8()
                .save_with_format(&destination, ImageFormat::Png)
                .with_context(|| {
                    format!("cannot write generated app icon {}", destination.display())
                })?;
            let member = format!("{app_bundle}/{filename}");
            artifacts.icon_members.push(member.clone());
            artifacts.additions.push((member, destination));
        }
    }
    Ok(artifacts)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn device_allowlist_is_removed_only_when_enabled_without_branding() {
        for enabled in [None, Some(false), Some(true)] {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("input");
            fs::create_dir_all(&source).unwrap();
            let mut root = Dictionary::new();
            root.insert("UISupportedDevices".into(), Value::Array(vec![Value::String("iPad1,1".into())]));
            root.insert("UIDeviceFamily".into(), Value::Array(vec![Value::Integer(1.into()), Value::Integer(2.into())]));
            write_binary_plist(&Value::Dictionary(root), &source.join("Info.plist")).unwrap();
            let option = enabled.map(|v| format!("remove_supported_devices: {v}")).unwrap_or_default();
            let manifest: Manifest = serde_yaml::from_str(&format!(
                "project: {{ name: Test, bundle_id: test, platform: ios, arch: arm64, min_ios: '14.0' }}\nruntime: {{ config_segment: __CFG, cave_segment: __CAVE, page_size: 16384, dylib_name: Test.dylib }}\napp: {{ {option} }}\n"
            )).unwrap();
            let out = dir.path().join("out");
            let artifacts = prepare(&manifest, "Payload/Test.app", "Info.plist", &source, &out).unwrap();
            let plist = if enabled == Some(true) {
                assert_eq!(artifacts.replacements.len(), 1);
                read_plist(&out.join("Info.plist")).unwrap()
            } else {
                assert!(artifacts.replacements.is_empty());
                read_plist(&source.join("Info.plist")).unwrap()
            };
            let dict = plist.as_dictionary().unwrap();
            assert_eq!(dict.contains_key("UISupportedDevices"), enabled != Some(true));
            assert!(dict.contains_key("UIDeviceFamily"));
        }
    }
}
