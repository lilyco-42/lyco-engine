//! 用 FreeMote.Samples 真实样本验证 PSB 解析(基于 emote-psb)。
//! fixtures 目录:引擎根 `fixtures/`。

use std::path::PathBuf;
use yuzu_psb::PsbFile;

fn fixture(name: &str) -> Vec<u8> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../fixtures");
    p.push(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("读取样本 {} 失败: {e}", p.display()))
}

#[test]
fn parses_scn_fixture() {
    let data = fixture("c01c.txt.scn");
    let psb = PsbFile::parse(&data).expect("解析 c01c.txt.scn 失败");
    let root = psb.root();
    assert!(root.is_object(), "SCN 根应为对象,实际 {:?}", root);
    let root_keys: Vec<&str> = root.as_object().unwrap().keys().map(|s| s.as_str()).collect();
    eprintln!("scn root keys: {root_keys:?}");
    for k in ["scenes", "name"] {
        assert!(root.get(k).is_some(), "SCN 根缺少键 {k}");
    }
    let scenes = root.get("scenes").unwrap().as_array().unwrap();
    assert!(!scenes.is_empty());
    let first = &scenes[0];
    eprintln!("scn v{} enc={} scenes={} res={} extra={}", psb.version(), psb.encrypted(), scenes.len(), psb.resources_len(), psb.extra_resources_len());
    if let Some(o) = first.as_object() {
        eprintln!("scene[0] keys: {:?}", o.keys().collect::<Vec<_>>());
    }
}

#[test]
fn parses_pimg_fixture() {
    let data = fixture("title.pimg");
    let psb = PsbFile::parse(&data).expect("解析 title.pimg 失败");
    let root = psb.root();
    assert!(root.is_object());
    for k in ["layers", "width", "height"] {
        assert!(root.get(k).is_some(), "PIMG 根缺少键 {k}");
    }
    assert!(psb.resources_len() > 0, "pimg 应含资源块");
    let layers = root.get("layers").expect("layers");
    assert!(layers.is_array(), "layers 应为数组");
    eprintln!("pimg: v{} enc={} res={} extra={}", psb.version(), psb.encrypted(), psb.resources_len(), psb.extra_resources_len());
}

#[test]
fn json_dump_scn_is_valid() {
    let data = fixture("c01c.txt.scn");
    let psb = PsbFile::parse(&data).unwrap();
    let json = serde_json::to_string(psb.root()).unwrap();
    assert!(json.starts_with('{'));
    assert!(json.contains("scenes"));
    assert!(json.contains("texts"));
    eprintln!("scn json len = {}", json.len());
}