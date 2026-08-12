//! # yuzu-scn
//!
//! 柚子社 SCN(`*.scn`,PSB 容器)剧本解析器。
//!
//! 解析结果(`model::Scn`)可直接驱动 Web 引擎的对话/选择 / 事件机制。

mod model;

pub use model::*;

use serde_json::Value;
use yuzu_psb::PsbFile;

/// 解析 SCN 文件字节。
pub fn parse(data: &[u8]) -> Result<Scn, String> {
    let psb = PsbFile::parse(data).map_err(|e| e.to_string())?;
    let root = psb.root();
    let obj = root.as_object().ok_or("SCN 根不是对象")?;

    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let hash = obj.get("hash").and_then(Value::as_i64);
    let mut scenes = Vec::new();
    if let Some(arr) = obj.get("scenes").and_then(Value::as_array) {
        for (i, sc) in arr.iter().enumerate() {
            scenes.push(parse_scene(sc, i));
        }
    }
    Ok(Scn { name, hash, scenes })
}

fn parse_scene(v: &Value, index: usize) -> Scene {
    let o = v.as_object().cloned().unwrap_or_default();
    let title = extract_title(o.get("title"));
    let lines = o
        .get("lines")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(parse_line).collect())
        .unwrap_or_default();
    let texts = o
        .get("texts")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(parse_text).collect())
        .unwrap_or_default();
    let nexts = o
        .get("nexts")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(parse_next).collect())
        .unwrap_or_default();
    Scene {
        label: o
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or(&format!("scene{index}"))
            .to_string(),
        title,
        first_line: o.get("firstLine").and_then(Value::as_i64).unwrap_or(0),
        sp_count: o.get("spCount").and_then(Value::as_i64).unwrap_or(0),
        lines,
        texts,
        nexts,
    }
}

fn extract_title(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn vstr(v: &Value, i: usize) -> Option<&str> {
    v.get(i).and_then(Value::as_str)
}
fn vi64(v: &Value, i: usize) -> Option<i64> {
    v.get(i).and_then(Value::as_i64)
}

/// 行解析:整数=文本索引、字符串=资源、数组=事件或快照点。
fn parse_line(v: &Value) -> Line {
    match v {
        Value::Number(n) => Line::Text(n.as_i64().unwrap_or(-1) as usize),
        Value::String(s) => Line::Resource(s.clone()),
        Value::Array(arr) => {
            // SnapshotPointLine: [index, a, b, flag, original_line(, voice, ua, ub)]
            if let Some(Value::Number(first)) = arr.first() {
                if let Some(idx) = first.as_i64() {
                    if (5..=8).contains(&arr.len()) {
                        // 快照点:原文件同索引文本行即是快照点
                        return Line::SnapshotPoint(SnapshotPoint {
                            index: idx,
                            text_index_a: vi64(v, 1),
                            text_index_b: vi64(v, 2),
                            flag: vi64(v, 3),
                            original_line: vi64(v, 4).unwrap_or(0),
                        });
                    }
                }
            }
            if let Some(Value::String(kind)) = arr.first() {
                let kind = kind.clone();
                let fields = arr[1..].to_vec();
                return Line::Event(Event { kind, fields });
            }
            Line::Raw(v.clone())
        }
        other => Line::Raw(other.clone()),
    }
}

/// 文本池条目(真实千恋万花 SCN 布局):
/// `[character, display?, dialogue(字符串|{lang}), voices[], value, data?]`
fn parse_text(v: &Value) -> Text {
    let arr = match v.as_array() {
        Some(a) => a,
        None => {
            return Text {
                character: None,
                dialogues: Vec::new(),
                voices: Vec::new(),
                value: None,
            }
        }
    };
    let character = vstr(v, 0).map(ToString::to_string);
    // 兼容两种布局: A) [name, dialogues[], voices[], value]  B) 真实布局(内容在 index 2)
    let dialogues = if let Some(arr1) = arr.get(1).and_then(Value::as_array) {
        arr1.iter().map(parse_dialogue).collect()
    } else {
        let content = match arr.get(2) {
            Some(Value::String(s)) => Content::Plain(s.clone()),
            Some(Value::Object(map)) => {
                let mut lm = LangMap::default();
                for (k, val) in map {
                    let Some(s) = val.as_str() else { continue };
                    match k.as_str() {
                        "jp" | "ja" => lm.jp = Some(s.to_string()),
                        "en" => lm.en = Some(s.to_string()),
                        "cn" | "zh" | "zh_cn" => lm.cn = Some(s.to_string()),
                        "tw" | "zh_tw" => lm.tw = Some(s.to_string()),
                        other => lm.extra.push((other.to_string(), s.to_string())),
                    }
                }
                Content::Lang(lm)
            }
            _ => Content::Plain(String::new()),
        };
        vec![Dialogue {
            name: character.clone(),
            content,
        }]
    };
    let voices = arr
        .get(3)
        .and_then(Value::as_array)
        .map(|vs| {
            vs.iter()
                .map(|vv| Voice {
                    name: vv
                        .get("name")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                    voice: vv
                        .get("voice")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                })
                .collect()
        })
        .unwrap_or_default();
    let value = vi64(v, 4);
    Text {
        character,
        dialogues,
        voices,
        value,
    }
}

/// 台词:[显示名, 内容(, 长度)]
/// 内容可能是字符串(单语言)或 {jp,en,cn,tw} 字典。
fn parse_dialogue(v: &Value) -> Dialogue {
    let arr = v.as_array().cloned().unwrap_or_default();
    let name = arr.first().and_then(Value::as_str).map(ToString::to_string);
    let content = match arr.get(1) {
        Some(Value::String(s)) => Content::Plain(s.clone()),
        Some(Value::Object(map)) => {
            let mut lm = LangMap::default();
            for (k, val) in map {
                let Some(s) = val.as_str() else {
                    continue;
                };
                match k.as_str() {
                    "jp" | "ja" => lm.jp = Some(s.to_string()),
                    "en" => lm.en = Some(s.to_string()),
                    "cn" | "zh" | "zh_cn" => lm.cn = Some(s.to_string()),
                    "tw" | "zh_tw" => lm.tw = Some(s.to_string()),
                    other => lm.extra.push((other.to_string(), s.to_string())),
                }
            }
            Content::Lang(lm)
        }
        _ => Content::Plain(String::new()),
    };
    Dialogue { name, content }
}

fn parse_next(v: &Value) -> Next {
    // 兼容两种布局:对象 { storage, target, type }(真实游戏) 与 数组 [storage, target, kind]
    if let Some(o) = v.as_object() {
        Next {
            storage: o.get("storage").and_then(Value::as_str).unwrap_or("").to_string(),
            target: o.get("target").and_then(Value::as_str).unwrap_or("").to_string(),
            kind: o
                .get("type")
                .and_then(Value::as_i64)
                .or_else(|| o.get("kind").and_then(Value::as_i64))
                .unwrap_or(0),
        }
    } else {
        Next {
            storage: vstr(v, 0).unwrap_or("").to_string(),
            target: vstr(v, 1).unwrap_or("").to_string(),
            kind: vi64(v, 2).unwrap_or(0),
        }
    }
}
