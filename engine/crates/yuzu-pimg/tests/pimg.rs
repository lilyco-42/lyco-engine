//! yuzu-pimg 渲染验证:title.pimg(真实 FreeMote 样本)应能解析并合成。

use std::path::PathBuf;
use yuzu_pimg::{parse, render_png};

fn fixture(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("读取样本 {} 失败: {e}", p.display()))
}

#[test]
fn parses_title_pimg() {
    let data = fixture("title.pimg");
    let pimg = parse(&data).expect("解析 title.pimg 失败");
    assert!(pimg.width > 0 && pimg.height > 0);
    assert!(!pimg.layers.is_empty(), "应有图层");
    assert!(!pimg.textures.is_empty(), "应有贴图资源");
    eprintln!(
        "title.pimg: {}x{}, layers={}, textures={}",
        pimg.width,
        pimg.height,
        pimg.layers.len(),
        pimg.textures.len()
    );
}

#[test]
fn renders_title_pimg_to_png() {
    let data = fixture("title.pimg");
    let png = render_png(&data).expect("render 失败");
    assert!(png.len() > 1000, "PNG 输出过小: {} bytes", png.len());
    // 保存到 fixtures 供 web demo 使用
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/assets/title_demo.png");
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(&out, &png).unwrap();
    eprintln!("title_demo.png written: {} bytes", png.len());
}
