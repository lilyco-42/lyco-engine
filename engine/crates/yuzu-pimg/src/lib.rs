//! # yuzu-pimg
//!
//! 柚子社 PIMG(PSB-based)分层立绘合成。
//!
//! PIMG PSB 根对象结构(FreeMote / arc_unpacker 交叉验证):
//!
//! ```json
//! {
//!   "width": 1280, "height": 720,
//!   "layers": [
//!     { "layer_id": 0, "left": 0, "top": 0, "width": .., "height": ..,
//!       "name": "...", "visible": 1, "opacity": 100, "group_layer_id": .. }
//!   ],
//!   "0.tlg": {"__PSB@RESOURCE": 0}, "1.tlg": {...}, ...
//! }
//! ```
//! 每个 layer 通过 `layer_id` 关联同名的资源贴图。

use image::{Rgba, RgbaImage};
use yuzu_psb::{PsbFile, ResourceRef};
use yuzu_tlg::{self, is_tlg};

#[derive(Debug, Clone, Copy, Default)]
pub struct Layer {
    pub id: i64,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub visible: bool,
    pub opacity: f32,
}

impl Layer {
    fn from_json(i: usize, v: &serde_json::Value) -> Layer {
        let n =
            |k: &str, d: i64| -> i64 { v.get(k).and_then(serde_json::Value::as_i64).unwrap_or(d) };
        Layer {
            id: n("layer_id", i as i64),
            x: n("left", 0) as i32,
            y: n("top", 0) as i32,
            width: n("width", 0) as i32,
            height: n("height", 0) as i32,
            visible: v
                .get("visible")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(1)
                != 0,
            // PSB 中 opacity 为 0-255 标度(255=不透明),归一化到 0-1
            opacity: v
                .get("opacity")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(255.0) as f32
                / 255.0,
        }
    }
}

/// 解析结果:画布尺寸、图层、按 layer_id 关联的资源字节。
pub struct Pimg {
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    /// (layer_id, 资源原始字节)
    pub textures: Vec<(i64, Vec<u8>)>,
}

#[derive(Debug)]
pub enum Error {
    Io(String),
    Tlg(yuzu_tlg::TlgError),
    Png(image::ImageError),
    Decode(String),
    Invalid(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(s) => write!(f, "IO: {s}"),
            Error::Tlg(e) => write!(f, "{e}"),
            Error::Png(e) => write!(f, "PNG: {e}"),
            Error::Decode(s) => write!(f, "解码失败: {s}"),
            Error::Invalid(s) => write!(f, "无效 PIMG: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<yuzu_tlg::TlgError> for Error {
    fn from(e: yuzu_tlg::TlgError) -> Self {
        Error::Tlg(e)
    }
}

impl From<image::ImageError> for Error {
    fn from(e: image::ImageError) -> Self {
        Error::Png(e)
    }
}

/// 解析 PIMG 文件字节。
pub fn parse(data: &[u8]) -> Result<Pimg, Error> {
    let psb = PsbFile::parse(data).map_err(|e| Error::Decode(e.to_string()))?;
    let root = psb.root();

    let width = root
        .get("width")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0) as u32;
    let height = root
        .get("height")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0) as u32;

    let mut layers = Vec::new();
    if let Some(arr) = root.get("layers").and_then(serde_json::Value::as_array) {
        for (i, item) in arr.iter().enumerate() {
            layers.push(Layer::from_json(i, item));
        }
    }

    // 收集根对象中的贴图资源:key 形如 "3.tlg" / "0.png"
    let mut textures: Vec<(i64, Vec<u8>)> = Vec::new();
    if let Some(obj) = root.as_object() {
        for (k, v) in obj {
            collect_texture(&psb, k, v, &mut textures);
        }
    }

    if width == 0 || height == 0 {
        return Err(Error::Invalid(format!(
            "画布尺寸缺失 ({width}x{height}), 资源 {} 个, layers {} 个",
            psb.resources_len(),
            layers.len()
        )));
    }
    Ok(Pimg {
        width,
        height,
        layers,
        textures,
    })
}

fn collect_texture(psb: &PsbFile, k: &str, v: &serde_json::Value, out: &mut Vec<(i64, Vec<u8>)>) {
    if let Some(r) = PsbFile::resource_ref(v) {
        let bytes = match r {
            ResourceRef::Resource(idx) => psb.resource(idx),
            ResourceRef::Extra(idx) => psb.extra_resource(idx),
        };
        if let Some(b) = bytes {
            out.push((extract_layer_id(k), b.to_vec()));
        }
    }
}

/// 从 "3.tlg" / "3.png" / "3" 提取 layer_id。
fn extract_layer_id(key: &str) -> i64 {
    let prefix = key.split('.').next().unwrap_or(key);
    prefix.parse::<i64>().unwrap_or(-1)
}

/// 解码单张贴图为 RGBA(TLG 用 tlg-rs,其余走 PNG 解码)。
pub fn decode_texture(bytes: &[u8]) -> Result<RgbaImage, Error> {
    if is_tlg(bytes) {
        Ok(yuzu_tlg::decode(bytes)?)
    } else {
        let img = image::load_from_memory(bytes)?;
        Ok(img.to_rgba8())
    }
}

/// source-over 混合(支持图层透明度)。画布为 RGBA。
// 参数为像素拷贝所需的全部几何量;内层热循环,保持值传递以利于优化。
#[allow(clippy::too_many_arguments)]
fn blend_over(
    dst: &mut [u8],
    src: &[u8],
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    cw: u32,
    ch: u32,
    opacity: f32,
) {
    for row in 0..h {
        let dy = y + row as i32;
        if dy < 0 || dy as u32 >= ch {
            continue;
        }
        for col in 0..w {
            let dx = x + col as i32;
            if dx < 0 || dx as u32 >= cw {
                continue;
            }
            let si = ((row as usize) * (w as usize) + col as usize) * 4;
            let di = ((dy as usize) * (cw as usize) + dx as usize) * 4;
            let a = ((src[si + 3] as f32) * opacity).clamp(0.0, 255.0);
            let da = dst[di + 3] as f32;
            let out_a = a + da * (1.0 - a / 255.0);
            if out_a <= 0.0 {
                continue;
            }
            for c in 0..3 {
                let sc = src[si + c] as f32;
                let dc = dst[di + c] as f32;
                dst[di + c] = ((sc * a / 255.0 + dc * da / 255.0 * (1.0 - a / 255.0))
                    / (out_a / 255.0)) as u8;
            }
            dst[di + 3] = out_a.clamp(0.0, 255.0) as u8;
        }
    }
}

/// 合成 PIMG 到画布(按 layers 顺序 source-over)。
pub fn compose(pimg: &Pimg) -> Result<RgbaImage, Error> {
    let w = pimg.width;
    let h = pimg.height;
    let mut canvas = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 0]));

    // 预解码:layer_id -> image
    let mut decoded: Vec<(i64, RgbaImage)> = Vec::new();
    for (id, bytes) in &pimg.textures {
        if let Ok(img) = decode_texture(bytes) {
            decoded.push((*id, img));
        }
    }

    for layer in &pimg.layers {
        if !layer.visible {
            continue;
        }
        if let Some((_, img)) = decoded.iter().find(|(id, _)| *id == layer.id) {
            blend_over(
                canvas.as_mut(),
                img.as_raw(),
                layer.x,
                layer.y,
                img.width(),
                img.height(),
                w,
                h,
                layer.opacity,
            );
        }
    }
    Ok(canvas)
}

/// 一步到位:解析 + 合成。
pub fn render(data: &[u8]) -> Result<(Pimg, RgbaImage), Error> {
    let p = parse(data)?;
    let img = compose(&p)?;
    Ok((p, img))
}

/// 渲染并编码为 PNG 字节。
pub fn render_png(data: &[u8]) -> Result<Vec<u8>, Error> {
    let (_, img) = render(data)?;
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)?;
    Ok(out)
}
