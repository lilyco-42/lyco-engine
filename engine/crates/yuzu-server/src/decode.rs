//! 解码/反编译服务层:剧本 → 可读文本,图像 → 可显示字节。

use anyhow::{bail, Result};

/// 反编译 SCN 剧本为可读文本(对白 + 事件),供审查/翻译/校对。
pub fn decompile_scn(data: &[u8]) -> Result<String> {
    let scn = yuzu_scn::parse(data).map_err(|e| anyhow::anyhow!(e))?;
    let mut out = String::new();
    out.push_str(&format!(
        "SCN: {} (scenes={})\n\n",
        scn.name,
        scn.scenes.len()
    ));
    for s in &scn.scenes {
        let title = s.title.first().map(|t| t.as_str()).unwrap_or("-");
        out.push_str(&format!("=== {title} (label={}) ===\n", s.label));
        for line in &s.lines {
            match line {
                yuzu_scn::Line::Text(idx) => {
                    if let Some(t) = s.texts.get(*idx) {
                        let who = t.character.as_deref().unwrap_or("?");
                        let text = t
                            .dialogues
                            .iter()
                            .map(|d| match &d.content {
                                yuzu_scn::Content::Plain(x) => x.clone(),
                                yuzu_scn::Content::Lang(lm) => {
                                    lm.first(&["jp", "ja", "cn", "zh"]).unwrap_or_default()
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(" / ");
                        if !text.is_empty() {
                            out.push_str(&format!("{who}: {text}\n"));
                        }
                    }
                }
                yuzu_scn::Line::Resource(r) => out.push_str(&format!("<{r}>\n")),
                yuzu_scn::Line::Event(e) => {
                    out.push_str(&format!(
                        "<{} {}\n",
                        e.kind,
                        serde_json::to_string(&e.fields).unwrap_or_default()
                    ));
                }
                yuzu_scn::Line::SnapshotPoint(_) | yuzu_scn::Line::Raw(_) => {}
            }
        }
        out.push('\n');
    }
    Ok(out)
}

/// 解码图像字节 → `(bytes, mime)`。TLG / PSB(pimg)解码为 PNG;
/// WebP / PNG / JPEG / GIF 原样透传(浏览器原生渲染)。
pub fn decode_image(data: &[u8]) -> Result<(Vec<u8>, &'static str)> {
    if yuzu_tlg::is_tlg(data) {
        let img = yuzu_tlg::decode(data)?;
        let mut out = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut out),
            yuzu_tlg::image::ImageFormat::Png,
        )?;
        Ok((out, "image/png"))
    } else if is_psb(data) {
        let png = yuzu_pimg::render_png(data)?;
        Ok((png, "image/png"))
    } else if is_webp(data) {
        Ok((data.to_vec(), "image/webp"))
    } else if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok((data.to_vec(), "image/png"))
    } else if data.starts_with(b"\xFF\xD8\xFF") {
        Ok((data.to_vec(), "image/jpeg"))
    } else if data.starts_with(b"GIF8") {
        Ok((data.to_vec(), "image/gif"))
    } else {
        let h = [
            data.first().copied().unwrap_or(0),
            data.get(1).copied().unwrap_or(0),
        ];
        bail!("未知图像格式(魔数 0x{:02X}{:02X})", h[0], h[1]);
    }
}

fn is_psb(d: &[u8]) -> bool {
    d.starts_with(b"PSB\x00")
}
fn is_webp(d: &[u8]) -> bool {
    d.len() >= 12 && &d[0..4] == b"RIFF" && &d[8..12] == b"WEBP"
}

/// 按扩展名推断 MIME(原始提取端点用)。
pub fn mime_for(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".ogg") {
        "audio/ogg"
    } else if lower.ends_with(".mp3") {
        "audio/mpeg"
    } else if lower.ends_with(".wav") {
        "audio/wav"
    } else if lower.ends_with(".tjs")
        || lower.ends_with(".txt")
        || lower.ends_with(".csv")
        || lower.ends_with(".json")
        || lower.ends_with(".ini")
    {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

/// 按内容魔数嗅探 MIME(覆盖命名不一致:游戏内 `.ogg` 实为 M4A 等)。
pub fn content_type(name: &str, data: &[u8]) -> &'static str {
    if data.len() >= 12 && &data[4..8] == b"ftyp" {
        return "audio/mp4";
    }
    if data.starts_with(b"OggS") {
        return "audio/ogg";
    }
    if data.starts_with(b"RIFF") && data.len() >= 12 && &data[8..12] == b"WAVE" {
        return "audio/wav";
    }
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return "image/png";
    }
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return "image/webp";
    }
    if data.starts_with(b"\xFF\xD8\xFF") {
        return "image/jpeg";
    }
    if data.starts_with(b"GIF8") {
        return "image/gif";
    }
    mime_for(name)
}
