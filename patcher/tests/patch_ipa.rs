use std::{
    fs::File,
    io::{Read, Write},
    process::Command,
};
use tempfile::tempdir;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

fn put32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes())
}
fn put64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes())
}
fn macho_fixture(filetype: u32) -> Vec<u8> {
    let mut d = vec![0u8; 0x1000];
    put32(&mut d, 0, 0xfeedfacf);
    put32(&mut d, 4, 0x0100000c);
    put32(&mut d, 12, filetype);
    put32(&mut d, 16, 2);
    put32(&mut d, 20, 168);
    let p = 32;
    put32(&mut d, p, 0x19);
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
    put64(&mut d, s + 32, 0x1_0000_0400);
    put64(&mut d, s + 40, 0x100);
    put32(&mut d, s + 48, 0x400);
    let c = p + 152;
    put32(&mut d, c, 0x1d);
    put32(&mut d, c + 4, 16);
    put32(&mut d, c + 8, 0x800);
    put32(&mut d, c + 12, 16);
    put32(&mut d, 0x400, 0xd503201f);
    d
}

#[test]
fn patches_complete_ipa_and_stages_dylib() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("input.ipa");
    let output = dir.path().join("output.ipa");
    let manifest = dir.path().join("hooks.yaml");
    let dylib = dir.path().join("Demo.dylib");
    let mut zip = ZipWriter::new(File::create(&input).unwrap());
    zip.start_file(
        "Payload/Demo.app/Demo",
        SimpleFileOptions::default().unix_permissions(0o755),
    )
    .unwrap();
    zip.write_all(b"main-executable").unwrap();
    zip.start_file(
        "Payload/Demo.app/Frameworks/UnityFramework.framework/UnityFramework",
        SimpleFileOptions::default().unix_permissions(0o755),
    )
    .unwrap();
    zip.write_all(&macho_fixture(6)).unwrap();
    zip.start_file("Payload/Demo.app/Info.plist", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"plist").unwrap();
    zip.finish().unwrap();
    std::fs::write(&dylib, b"signed-demo-dylib").unwrap();
    std::fs::write(&manifest,"project: { name: Demo, bundle_id: com.example.demo, platform: ios, arch: arm64, min_ios: '14.0' }\nruntime: { config_segment: __IAOTCFG, cave_segment: __IAOTCAVE, page_size: 16384, dylib_name: Demo.dylib }\napp: { executable_member: Payload/Demo.app/Demo, authorized_only: true, prefer_unity_framework: true }\nconfig:\n  flags: [{ name: enabled, index: 0, default: true }]\n  strings: []\n  callbacks: []\nfunctions: []\ngenerated_hooks: []\npatches:\n  - name: auto_bytes\n    enabled: true\n    description: synthetic owned fixture\n    locator: { type: va, value: 0x100000400 }\n    patch: { type: branch_to_cave, action: { type: ret_if_flag, flag: enabled } }\n    expect: auto\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_iaot"))
        .args([
            "patch-ipa",
            "--manifest",
            manifest.to_str().unwrap(),
            "--in",
            input.to_str().unwrap(),
            "--out",
            output.to_str().unwrap(),
            "--dylib",
            dylib.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("resolved automatically"));
    assert!(stdout.contains("UnityFramework.framework/UnityFramework"));
    let mut archive = ZipArchive::new(File::open(output).unwrap()).unwrap();
    let mut executable = Vec::new();
    archive
        .by_name("Payload/Demo.app/Frameworks/UnityFramework.framework/UnityFramework")
        .unwrap()
        .read_to_end(&mut executable)
        .unwrap();
    assert!(executable.windows(8).any(|x| x == b"IAOTCFG1"));
    assert!(executable.windows(10).any(|x| x == b"__IAOTCAVE"));
    assert!(executable
        .windows(38)
        .any(|x| x == b"@executable_path/Frameworks/Demo.dylib"));
    let mut embedded = String::new();
    archive
        .by_name("Payload/Demo.app/Frameworks/Demo.dylib")
        .unwrap()
        .read_to_string(&mut embedded)
        .unwrap();
    assert_eq!(embedded, "signed-demo-dylib");
}
