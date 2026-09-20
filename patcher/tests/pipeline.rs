use image::{Rgb, RgbImage};
use std::{
    fs::File,
    io::{Cursor, Read, Write},
    process::Command,
};
use tempfile::tempdir;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

fn put32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn macho(filetype: u32) -> Vec<u8> {
    let mut data = vec![0u8; 0x5000];
    put32(&mut data, 0, 0xfeedfacf);
    put32(&mut data, 4, 0x0100000c);
    put32(&mut data, 12, filetype);
    put32(&mut data, 16, 3);
    put32(&mut data, 20, 240);
    let p = 32;
    put32(&mut data, p, 0x19);
    put32(&mut data, p + 4, 152);
    data[p + 8..p + 14].copy_from_slice(b"__TEXT");
    put64(&mut data, p + 24, 0x100000000);
    put64(&mut data, p + 32, 0x4000);
    put64(&mut data, p + 48, 0x4000);
    put32(&mut data, p + 56, 7);
    put32(&mut data, p + 60, 5);
    put32(&mut data, p + 64, 1);
    let s = p + 72;
    data[s..s + 6].copy_from_slice(b"__text");
    data[s + 16..s + 22].copy_from_slice(b"__TEXT");
    put64(&mut data, s + 32, 0x100000400);
    put64(&mut data, s + 40, 0x100);
    put32(&mut data, s + 48, 0x400);
    let linkedit = p + 152;
    put32(&mut data, linkedit, 0x19);
    put32(&mut data, linkedit + 4, 72);
    data[linkedit + 8..linkedit + 18].copy_from_slice(b"__LINKEDIT");
    put64(&mut data, linkedit + 24, 0x1_0000_4000);
    put64(&mut data, linkedit + 32, 0x4000);
    put64(&mut data, linkedit + 40, 0x4000);
    put64(&mut data, linkedit + 48, 0x1000);
    put32(&mut data, linkedit + 56, 1);
    put32(&mut data, linkedit + 60, 1);
    let signature = linkedit + 72;
    put32(&mut data, signature, 0x1d);
    put32(&mut data, signature + 4, 16);
    put32(&mut data, signature + 8, 0x4800);
    put32(&mut data, signature + 12, 0x800);
    data[0x4800..0x5000].fill(0xaa);
    put32(&mut data, 0x400, 0xd503201f);
    data
}

#[test]
fn complete_pipeline_analyzes_injects_repackages_and_validates() {
    let directory = tempdir().unwrap();
    let input = directory.path().join("input.ipa");
    let output = directory.path().join("output.ipa");
    let manifest = directory.path().join("hooks.yaml");
    let dylib = directory.path().join("Demo.dylib");
    let icon = directory.path().join("icon.png");
    let reports = directory.path().join("reports");
    let mut zip = ZipWriter::new(File::create(&input).unwrap());
    zip.start_file("Payload/Demo.app/Info.plist", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(br#"<?xml version="1.0"?><plist><dict><key>CFBundleExecutable</key><string>Demo</string><key>CFBundleIdentifier</key><string>com.example.demo</string></dict></plist>"#).unwrap();
    zip.start_file(
        "Payload/Demo.app/Demo",
        SimpleFileOptions::default().unix_permissions(0o755),
    )
    .unwrap();
    zip.write_all(&macho(2)).unwrap();
    zip.finish().unwrap();
    std::fs::write(&dylib, macho(6)).unwrap();
    RgbImage::from_pixel(256, 256, Rgb([17, 34, 51]))
        .save(&icon)
        .unwrap();
    std::fs::write(&manifest,"project: { name: Demo, bundle_id: com.example.demo, platform: ios, arch: arm64, min_ios: '14.0' }\nruntime: { config_segment: __IAOTCFG, cave_segment: __IAOTCAVE, page_size: 16384, dylib_name: Demo.dylib }\napp: { display_name: Branded Demo, icon_path: icon.png }\nanalysis: { runtime_il2cpp: false }\nconfig: { flags: [], strings: [], callbacks: [] }\nfunctions: []\ngenerated_hooks: []\npatches: []\n").unwrap();
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_iaot"))
        .canonicalize()
        .unwrap();
    let prepared = Command::new(&executable)
        .args([
            "prepare",
            "--manifest",
            manifest.to_str().unwrap(),
            "--in",
            input.to_str().unwrap(),
            "--reports",
            reports.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        prepared.status.success(),
        "prepare stdout={}\nprepare stderr={}",
        String::from_utf8_lossy(&prepared.stdout),
        String::from_utf8_lossy(&prepared.stderr)
    );
    assert!(reports.join("hook-preflight.yaml").is_file());
    let result = Command::new(&executable)
        .args([
            "pipeline",
            "--manifest",
            manifest.to_str().unwrap(),
            "--in",
            input.to_str().unwrap(),
            "--out",
            output.to_str().unwrap(),
            "--dylib",
            dylib.to_str().unwrap(),
            "--reports",
            reports.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(reports.join("ipa-analysis.yaml").is_file());
    assert!(reports.join("final-validation.yaml").is_file());
    let mut archive = ZipArchive::new(File::open(output).unwrap()).unwrap();
    assert!(archive
        .by_name("Payload/Demo.app/Frameworks/Demo.dylib")
        .is_ok());
    let mut plist_bytes = Vec::new();
    archive
        .by_name("Payload/Demo.app/Info.plist")
        .unwrap()
        .read_to_end(&mut plist_bytes)
        .unwrap();
    let plist = plist::Value::from_reader(Cursor::new(plist_bytes)).unwrap();
    assert_eq!(
        plist
            .as_dictionary()
            .unwrap()
            .get("CFBundleDisplayName")
            .and_then(plist::Value::as_string),
        Some("Branded Demo")
    );
    let mut icon_bytes = Vec::new();
    archive
        .by_name("Payload/Demo.app/IAOTAppIcon60x60@3x.png")
        .unwrap()
        .read_to_end(&mut icon_bytes)
        .unwrap();
    let generated_icon = image::load_from_memory(&icon_bytes).unwrap();
    assert_eq!(generated_icon.width(), 180);
    assert_eq!(generated_icon.height(), 180);
    let validation = std::fs::read_to_string(reports.join("final-validation.yaml")).unwrap();
    assert!(validation.contains("display_name: Branded Demo"));
    assert!(validation.contains("IAOTAppIcon60x60@3x.png"));
}
