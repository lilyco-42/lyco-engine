//! 对 arc_unpacker 提供的真实 XP3 夹具做交叉验证。
//!
//! 每个夹具均包含两个文件(期望内容见
//! `arc_unpacker tests/dec/kirikiri/xp3_archive_decoder_test.cc`):
//! - `123.txt` = "1234567890"
//! - `abc.xyz` = "abcdefghijklmnopqrstuvwxyz"

use std::path::PathBuf;
use yuzu_xp3::Xp3Archive;

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("读取夹具 {name} 失败: {e}"))
}

fn check(name: &str) {
    let archive = Xp3Archive::open(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(archive.len(), 2, "{name}: 应含 2 个文件");
    assert_eq!(
        archive.read("123.txt").unwrap().as_deref(),
        Some(b"1234567890".as_slice()),
        "{name}: 123.txt"
    );
    assert_eq!(
        archive.read("abc.xyz").unwrap().as_deref(),
        Some(b"abcdefghijklmnopqrstuvwxyz".as_slice()),
        "{name}: abc.xyz"
    );
    assert!(archive.file("不存在.txt").is_none());
    assert_eq!(archive.read("不存在.txt").unwrap(), None);
}

#[test]
fn version_1() {
    check("xp3-v1.xp3");
    assert_eq!(
        Xp3Archive::open(&fixture("xp3-v1.xp3")).unwrap().version(),
        1
    );
}

#[test]
fn version_2() {
    check("xp3-v2.xp3");
    assert_eq!(
        Xp3Archive::open(&fixture("xp3-v2.xp3")).unwrap().version(),
        2
    );
}

#[test]
fn compressed_table() {
    check("xp3-compressed-table.xp3");
}

#[test]
fn compressed_files() {
    check("xp3-compressed-files.xp3");
}

#[test]
fn multiple_segments() {
    let archive = Xp3Archive::open(&fixture("xp3-multiple-segm.xp3")).unwrap();
    let f = archive.file("123.txt").unwrap();
    assert!(
        f.segments.len() >= 2,
        "应含多个段,实际 {}",
        f.segments.len()
    );
    check("xp3-multiple-segm.xp3");
}

#[test]
fn shuffled_chunks() {
    check("xp3-shuffled.xp3");
}

#[test]
fn time_chunk() {
    check("xp3-time.xp3");
}

#[test]
fn not_xp3_rejected() {
    assert!(matches!(
        Xp3Archive::open(b"not an archive at all........."),
        Err(yuzu_xp3::Error::NotXp3)
    ));
    assert!(matches!(
        Xp3Archive::open(&[]),
        Err(yuzu_xp3::Error::NotXp3)
    ));
}

#[test]
fn metadata_exposed() {
    let archive = Xp3Archive::open(&fixture("xp3-v1.xp3")).unwrap();
    let f = archive.file("abc.xyz").unwrap();
    assert_eq!(f.size, 26);
    assert_eq!(f.hash, 0x1234_5678); // 夹具 adlr 校验键
    assert_eq!(f.packed_size, 26);
    let names: Vec<&str> = archive.file_names().collect();
    assert_eq!(names, ["123.txt", "abc.xyz"]);
}
