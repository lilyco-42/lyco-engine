//! # yuzu-wasm — Web 端 WASM 绑定
//!
//! 用 wasm-bindgen 把引擎(yuzu-scn / yuzu-engine / yuzu-pimg / yuzu-tlg /
//! yuzu-xp3)暴露给浏览器:
//! - `parse_scn` / `compile_scn` — 剧本解析与编译为步骤流(JSON);
//! - `pimg_info` / `render_pimg_png` — 立绘信息与合成 PNG;
//! - `decode_tlg_png` — TLG 纹理转 PNG;
//! - `xp3_info` / `xp3_read` — 浏览器内 XP3 归档解包。
//!
//! 字节参数从 JS 以 `Uint8Array` 传入,返回的 `Vec<u8>` 亦映射为 `Uint8Array`。
//! 构建: `cargo build --target wasm32-unknown-unknown -p yuzu-wasm`。

use wasm_bindgen::prelude::*;

use yuzu_engine::{ScriptMachine, Step};
use yuzu_pimg::{self, Pimg};
use yuzu_scn::Scn;
use yuzu_tlg::{decode as decode_tlg, is_tlg};

/// 引擎版本(与 workspace 版本一致)。
#[wasm_bindgen]
pub fn version() -> String {
    format!("yuzu-wasm {}", env!("CARGO_PKG_VERSION"))
}

// ---------------------------------------------------------------------------
// SCN 剧本
// ---------------------------------------------------------------------------

/// 解析 SCN 剧本,返回结构摘要 JSON:
/// `{ name, hash, scenes: [{ label, title, first_line, sp_count, nexts }] }`。
#[wasm_bindgen]
pub fn parse_scn(data: &[u8]) -> Result<String, JsError> {
    parse_scn_impl(data).map_err(|e| JsError::new(&e))
}

fn parse_scn_impl(data: &[u8]) -> Result<String, String> {
    let scn = yuzu_scn::parse(data)?;
    serde_json::to_string(&scn_summary(&scn)).map_err(|e| e.to_string())
}

/// 编译 SCN 为引擎步骤流 JSON:
/// `{ name, scenes: [{ label, steps: [...] }] }`。
/// 台词文本按 `cn/zh → ja/jp` 顺序解析;渲染端按步消费即可驱动对话。
#[wasm_bindgen]
pub fn compile_scn(data: &[u8]) -> Result<String, JsError> {
    compile_scn_impl(data).map_err(|e| JsError::new(&e))
}

fn compile_scn_impl(data: &[u8]) -> Result<String, String> {
    let scn = yuzu_scn::parse(data)?;
    let mut machine = ScriptMachine::new();
    machine.compile(&scn);
    let mut scenes = Vec::new();
    for label in machine.scene_labels() {
        let run = machine.run(label, 0);
        let steps = run_progress(&run);
        scenes.push(serde_json::json!({ "label": label, "steps": steps }));
    }
    serde_json::to_string(&serde_json::json!({
        "name": scn.name,
        "scenes": scenes,
    }))
    .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// PIMG 立绘
// ---------------------------------------------------------------------------

/// PIMG 画布/图层信息 JSON:`{ width, height, layers: [...], textures }`。
#[wasm_bindgen]
pub fn pimg_info(data: &[u8]) -> Result<String, JsError> {
    pimg_info_impl(data).map_err(|e| JsError::new(&e))
}

fn pimg_info_impl(data: &[u8]) -> Result<String, String> {
    let pimg = yuzu_pimg::parse(data).map_err(|e| e.to_string())?;
    serde_json::to_string(&pimg_json(&pimg)).map_err(|e| e.to_string())
}

/// 渲染 PIMG 立绘(按图层 source-over 合成)并编码为 PNG。
#[wasm_bindgen]
pub fn render_pimg_png(data: &[u8]) -> Result<Vec<u8>, JsError> {
    render_pimg_png_impl(data).map_err(|e| JsError::new(&e))
}

fn render_pimg_png_impl(data: &[u8]) -> Result<Vec<u8>, String> {
    yuzu_pimg::render_png(data).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// TLG 纹理
// ---------------------------------------------------------------------------

/// 解码 TLG 纹理并编码为 PNG(非 TLG 输入报错)。
#[wasm_bindgen]
pub fn decode_tlg_png(data: &[u8]) -> Result<Vec<u8>, JsError> {
    decode_tlg_png_impl(data).map_err(|e| JsError::new(&e))
}

fn decode_tlg_png_impl(data: &[u8]) -> Result<Vec<u8>, String> {
    if !is_tlg(data) {
        return Err("不是 TLG 文件(魔数不符)".to_string());
    }
    let img = decode_tlg(data).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut out),
        yuzu_tlg::image::ImageFormat::Png,
    )
    .map_err(|e| e.to_string())?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// XP3 归档(浏览器内解包)
// ---------------------------------------------------------------------------

/// XP3 归档条目列表 JSON:
/// `{ version, count, files: [{ name, size, packed_size, hash, segments }] }`。
#[wasm_bindgen]
pub fn xp3_info(data: &[u8]) -> Result<String, JsError> {
    xp3_info_impl(data).map_err(|e| JsError::new(&e))
}

fn xp3_info_impl(data: &[u8]) -> Result<String, String> {
    let arc = yuzu_xp3::Xp3Archive::open(data).map_err(|e| e.to_string())?;
    let files: Vec<_> = arc
        .file_names()
        .map(|name| {
            let f = arc.file(name).expect("名称来自归档");
            serde_json::json!({
                "name": name,
                "size": f.size,
                "packed_size": f.packed_size,
                "hash": f.hash,
                "segments": f.segments.len(),
            })
        })
        .collect();
    serde_json::to_string(&serde_json::json!({
        "version": arc.version(),
        "count": arc.len(),
        "files": files,
    }))
    .map_err(|e| e.to_string())
}

/// 从 XP3 归档提取单个文件(解压后字节,未解密)。
/// 归档与文件名均从 JS 传入;归档字节以 `Uint8Array` 提供。
#[wasm_bindgen]
pub fn xp3_read(data: &[u8], name: &str) -> Result<Vec<u8>, JsError> {
    xp3_read_impl(data, name).map_err(|e| JsError::new(&e))
}

fn xp3_read_impl(data: &[u8], name: &str) -> Result<Vec<u8>, String> {
    let arc = yuzu_xp3::Xp3Archive::open(data).map_err(|e| e.to_string())?;
    arc.read(name)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("归档内不存在 {name}"))
}

// ---------------------------------------------------------------------------
// 场景文本解码(KiriKiri scenario encode,移植 krkrz TextStream)
// ---------------------------------------------------------------------------

/// 解码 KiriKiri 场景文本(`.ks`/`.csv`/`.stand`/`*.tjs` 数据等)。
/// 非场景编码输入报错。
#[wasm_bindgen]
pub fn decode_scenario(data: &[u8]) -> Result<String, JsError> {
    decode_scenario_impl(data).map_err(|e| JsError::new(&e))
}

fn decode_scenario_impl(data: &[u8]) -> Result<String, String> {
    yuzu_xp3::scenario::decode(data).ok_or_else(|| "非 KiriKiri 场景编码(无 fe fe 头)".to_string())
}

// ---------------------------------------------------------------------------
// KAG 引擎(执行场景 → 效果流)
// ---------------------------------------------------------------------------

/// 用 KAG 引擎执行指定场景,返回效果流 JSON:
/// `{ scene, effects: [{type: dialogue|layer|audio|chapter|wait, ...}] }`。
/// 引擎维护图层/音频/流程状态,渲染端按效果驱动 UI。
#[wasm_bindgen]
pub fn kag_run_scene(data: &[u8], scene_label: &str) -> Result<String, JsError> {
    kag_run_scene_impl(data, scene_label).map_err(|e| JsError::new(&e))
}

fn kag_run_scene_impl(data: &[u8], scene_label: &str) -> Result<String, String> {
    let scn = yuzu_scn::parse(data)?;
    let scene = scn
        .scene(scene_label)
        .ok_or_else(|| format!("场景不存在: {scene_label}"))?;
    let mut engine = yuzu_kag::KagEngine::new();
    let effects: Vec<_> = engine.run_scene(scene).iter().map(effect_json).collect();
    serde_json::to_string(&serde_json::json!({
        "scene": scene_label,
        "effects": effects,
    }))
    .map_err(|e| e.to_string())
}

fn effect_json(e: &yuzu_kag::Effect) -> serde_json::Value {
    use yuzu_kag::{AudioKind, Effect};
    match e {
        Effect::ShowDialogue { character, text, voice } => serde_json::json!({
            "type": "dialogue", "character": character, "text": text, "voice": voice,
        }),
        Effect::LayerChange { layer, name, image } => serde_json::json!({
            "type": "layer", "layer": format!("{layer:?}"), "name": name,
            "visible": image.as_ref().map(|i| i.visible),
            "file": image.as_ref().and_then(|i| i.file.clone()),
            "face": image.as_ref().and_then(|i| i.face.clone()),
            "dress": image.as_ref().and_then(|i| i.dress.clone()),
        }),
        Effect::Audio { kind, name } => serde_json::json!({
            "type": "audio", "kind": match kind {
                AudioKind::Bgm => "bgm", AudioKind::Se => "se", AudioKind::Voice => "voice",
            }, "name": name,
        }),
        Effect::Chapter { title } => serde_json::json!({ "type": "chapter", "title": title }),
        Effect::WaitForClick => serde_json::json!({ "type": "wait" }),
    }
}

// ---------------------------------------------------------------------------
// JSON 辅助
// ---------------------------------------------------------------------------

/// SCN 结构摘要。
fn scn_summary(scn: &Scn) -> serde_json::Value {
    let scenes: Vec<_> = scn
        .scenes
        .iter()
        .map(|s| {
            serde_json::json!({
                "label": s.label,
                "title": s.title,
                "first_line": s.first_line,
                "sp_count": s.sp_count,
                "line_count": s.lines.len(),
                "text_count": s.texts.len(),
                "nexts": s.nexts.iter().map(|n| serde_json::json!({
                    "storage": n.storage,
                    "target": n.target,
                    "kind": n.kind,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::json!({
        "name": scn.name,
        "hash": scn.hash,
        "scenes": scenes,
    })
}

/// 编译后的步骤流(ScriptMachine 的 Run 快照)。
fn run_progress(run: &yuzu_engine::Run) -> Vec<serde_json::Value> {
    let mut steps = Vec::new();
    let mut run = run.clone();
    while let Some(step) = run.advance() {
        steps.push(step_json(step));
    }
    steps
}

fn step_json(step: &Step) -> serde_json::Value {
    match step {
        Step::Dialogue {
            character,
            text,
            voice,
        } => serde_json::json!({
            "type": "dialogue",
            "character": character,
            "text": text,
            "voice": voice,
        }),
        Step::Command(value) => serde_json::json!({ "type": "command", "value": value }),
        Step::Event { kind, fields } => {
            serde_json::json!({ "type": "event", "kind": kind, "fields": fields })
        }
        Step::Label(value) => serde_json::json!({ "type": "label", "value": value }),
        Step::Jump { storage, target } => {
            serde_json::json!({ "type": "jump", "storage": storage, "target": target })
        }
        Step::End => serde_json::json!({ "type": "end" }),
    }
}

fn pimg_json(pimg: &Pimg) -> serde_json::Value {
    let layers: Vec<_> = pimg
        .layers
        .iter()
        .map(|l| {
            serde_json::json!({
                "id": l.id,
                "x": l.x,
                "y": l.y,
                "width": l.width,
                "height": l.height,
                "visible": l.visible,
                "opacity": l.opacity,
            })
        })
        .collect();
    serde_json::json!({
        "width": pimg.width,
        "height": pimg.height,
        "layers": layers,
        "textures": pimg.textures.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name);
        std::fs::read(&path).unwrap_or_else(|e| panic!("读取夹具 {name}: {e}"))
    }

    fn as_obj(s: &str) -> serde_json::Value {
        serde_json::from_str(s).expect("合法 JSON")
    }

    #[test]
    fn version_string() {
        assert!(version().starts_with("yuzu-wasm "));
    }

    #[test]
    fn parse_scn_fixture() {
        let data = fixture("c01c.txt.scn");
        let j = as_obj(&parse_scn_impl(&data).expect("parse_scn"));
        assert_eq!(j["name"], "c01c.txt");
        let scenes = j["scenes"].as_array().expect("scenes 数组");
        assert!(scenes.len() >= 2);
        assert!(scenes[0]["label"].as_str().is_some());
        assert!(scenes[0]["nexts"].is_array());
    }

    #[test]
    fn compile_scn_fixture_has_dialogues() {
        let data = fixture("c01c.txt.scn");
        let j = as_obj(&compile_scn_impl(&data).expect("compile_scn"));
        let scenes = j["scenes"].as_array().expect("scenes");
        let mut dialogues = 0;
        let mut chars = 0;
        for sc in scenes {
            for step in sc["steps"].as_array().expect("steps") {
                if step["type"] == "dialogue" {
                    dialogues += 1;
                    if !step["character"].as_str().unwrap_or("").is_empty() {
                        chars += 1;
                    }
                    assert!(!step["text"].as_str().unwrap_or("").is_empty(), "台词非空");
                }
            }
        }
        assert!(dialogues >= 10, "应有台词,实际 {dialogues}");
        assert!(chars >= 10, "台词应含角色名,实际 {chars}");
    }

    #[test]
    fn pimg_info_and_render() {
        let data = fixture("title.pimg");
        let j = as_obj(&pimg_info_impl(&data).expect("pimg_info"));
        assert_eq!(j["width"], 1280);
        assert_eq!(j["height"], 720);
        assert!(j["layers"]
            .as_array()
            .map(|a| a.len() >= 2)
            .unwrap_or(false));
        assert!(j["textures"].as_u64().map(|n| n >= 2).unwrap_or(false));

        let png = render_pimg_png_impl(&data).expect("render_pimg_png");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"), "应为 PNG");
    }

    #[test]
    fn decode_tlg_from_pimg_fixture() {
        let data = fixture("title.pimg");
        let psb = yuzu_psb::PsbFile::parse(&data).expect("解析 title.pimg");
        let mut tlg_found = false;
        if let Some(obj) = psb.root().as_object() {
            for v in obj.values() {
                if let Some(r) = yuzu_psb::PsbFile::resource_ref(v) {
                    let bytes = match r {
                        yuzu_psb::ResourceRef::Resource(i) => psb.resource(i),
                        yuzu_psb::ResourceRef::Extra(i) => psb.extra_resource(i),
                    };
                    if let Some(b) = bytes {
                        if is_tlg(b) {
                            tlg_found = true;
                            let png = decode_tlg_png_impl(b).expect("decode_tlg_png");
                            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
                            break;
                        }
                    }
                }
            }
        }
        assert!(tlg_found, "title.pimg 应含 TLG 资源");
    }

    #[test]
    fn rejects_bad_inputs() {
        assert!(parse_scn_impl(b"garbage").is_err());
        assert!(compile_scn_impl(b"garbage").is_err());
        assert!(decode_tlg_png_impl(b"not tlg").is_err());
        assert!(render_pimg_png_impl(b"garbage").is_err());
        assert!(pimg_info_impl(b"garbage").is_err());
        assert!(xp3_info_impl(b"garbage").is_err());
        assert!(xp3_read_impl(b"garbage", "x").is_err());
    }

    #[test]
    fn xp3_unpack_fixture() {
        // 复用 yuzu-xp3 的 arc_unpacker 夹具
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../yuzu-xp3/tests/fixtures/xp3-compressed-files.xp3");
        let data = std::fs::read(&path).expect("读取夹具");
        let j = as_obj(&xp3_info_impl(&data).expect("xp3_info"));
        assert_eq!(j["count"], 2);
        let files = j["files"].as_array().expect("files");
        assert_eq!(files[0]["name"], "123.txt");
        assert_eq!(files[0]["size"], 10);

        let out = xp3_read_impl(&data, "123.txt").expect("xp3_read");
        assert_eq!(out, b"1234567890");
        assert!(xp3_read_impl(&data, "不存在").is_err());
    }

    #[test]
    fn decode_scenario_real_file() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../realgame/mizuha.stand");
        let data = std::fs::read(&path).expect("读取 mizuha.stand");
        let text = decode_scenario_impl(&data).expect("解码");
        assert!(text.contains("みづはa"), "应引用纹理集: {text:?}");
    }

    #[test]
    fn kag_engine_runs_real_scene() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../realgame/scn001.scn");
        let data = std::fs::read(&path).expect("scn001");
        let j = as_obj(&kag_run_scene_impl(&data, "*001_01com").expect("kag_run_scene"));
        let effects = j["effects"].as_array().expect("effects");
        let dlg = effects.iter().filter(|e| e["type"] == "dialogue").count();
        assert!(dlg > 100, "引擎应产出大量台词,实际 {dlg}");

    }
}
