//! # yuzu-tlg
//!
//! Kirikiri TLG5/TLG6 纹理解码门面,基于 crates.io 的 [`tlg-rs`](https://crates.io/crates/tlg-rs)(MIT)。
//!
//! 千恋万花(柚子社)的立绘/背景以 `.tlg` 存储(通常嵌在 `.pimg` PSB 里),
//! 本 crate 提供统一解码入口,输出 `image::RgbaImage`。

pub use image;

use tlg::tlg_type::{PixelLayout, TlgDecoderTrait};

/// 解码 TLG 数据返回 RGBA 图像。
///
/// 自动识别 TLG5 / TLG6(以及带 SDS 头封装的形式)。
///
/// 注:直接使用 `tlg-rs` 的 `TlgReader::read` 会对非 SDS 的 TLG5/TLG6 报
/// `wrong magic`——其外层已消耗 11 字节完整魔数,内部解码器又从数据流起点
/// 再次校验魔数(TLG5_MAGIC 为 11 字节),必然错位。故这里绕过 reader,
/// 用 `from_data` 直接喂完整数据给对应版本解码器。
pub fn decode(data: &[u8]) -> Result<image::RgbaImage, TlgError> {
    let inner = unwrap_sds(data);
    let (pixels, info) = if inner.starts_with(b"TLG5.0\x00") {
        tlg::tlg5::Tlg5Decoder::from_data(inner.to_vec())
            .and_then(|d| d.decode())
            .map_err(|e| TlgError(format!("TLG5 解码失败: {e}")))?
    } else if inner.starts_with(b"TLG6.0\x00") {
        tlg::tlg6::Tlg6Decoder::from_data(inner.to_vec())
            .and_then(|d| d.decode())
            .map_err(|e| TlgError(format!("TLG6 解码失败: {e}")))?
    } else {
        return Err(TlgError("未知 TLG 版本(非 TLG5/TLG6)".to_string()));
    };

    let raw = match info.pixel_layout {
        PixelLayout::Rgba => pixels,
        PixelLayout::Rgb => rgb_to_rgba(&pixels),
        PixelLayout::Gray => gray_to_rgba(&pixels),
    };
    image::RgbaImage::from_raw(info.width, info.height, raw)
        .ok_or_else(|| TlgError("图像尺寸非法".to_string()))
}

/// 剥离 SDS 容器头(`TLG0.0\0sds\x1a` + u32 长度),返回内层 TLG 数据。
fn unwrap_sds(data: &[u8]) -> &[u8] {
    if data.starts_with(b"TLG0.0\x00sds\x1a") && data.len() >= 15 {
        let size = u32::from_le_bytes(data[11..15].try_into().unwrap()) as usize;
        let end = 15 + size;
        if end <= data.len() {
            return &data[15..end];
        }
    }
    data
}

fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
    rgb.chunks_exact(3)
        .flat_map(|c| [c[0], c[1], c[2], 255])
        .collect()
}

fn gray_to_rgba(gray: &[u8]) -> Vec<u8> {
    gray.iter().flat_map(|&v| [v, v, v, 255]).collect()
}

/// 判断数据是否为 TLG(通过魔数)。
pub fn is_tlg(data: &[u8]) -> bool {
    let magic: &[u8] = b"TLG0.0\x00";
    let magic2: &[u8] = b"TLG5.0\x00";
    let magic6: &[u8] = b"TLG6.0\x00";
    (data.starts_with(magic) && data.len() >= 11)
        || data.starts_with(magic2)
        || data.starts_with(magic6)
}

#[derive(Debug)]
pub struct TlgError(pub String);

impl std::fmt::Display for TlgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TLG 解码失败: {}", self.0)
    }
}

impl std::error::Error for TlgError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// 从真实 title.pimg 提取 TLG 资源逐一解码,断言至少一个成功。
    /// (个别资源可能在样本中截断——见 fixtures 说明——此时应清晰报错而非崩溃)
    #[test]
    fn decodes_embedded_tlg_from_pimg_fixture() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/title.pimg");
        let data = std::fs::read(&path).expect("读取 title.pimg");
        let psb = yuzu_psb::PsbFile::parse(&data).expect("解析 title.pimg");
        let root = psb.root();
        let mut decoded = 0usize;
        let mut tlg_count = 0usize;
        if let Some(obj) = root.as_object() {
            for (_k, v) in obj {
                if let Some(r) = yuzu_psb::PsbFile::resource_ref(v) {
                    let bytes = match r {
                        yuzu_psb::ResourceRef::Resource(i) => psb.resource(i),
                        yuzu_psb::ResourceRef::Extra(i) => psb.extra_resource(i),
                    };
                    if let Some(b) = bytes {
                        if is_tlg(b) {
                            tlg_count += 1;
                            match decode(b) {
                                Ok(img) => {
                                    assert!(img.width() > 0 && img.height() > 0);
                                    decoded += 1;
                                }
                                Err(e) => eprintln!("  (样本资源截断,跳过) {e}"),
                            }
                        }
                    }
                }
            }
        }
        assert!(decoded > 0, "title.pimg 中应有 TLG 资源被解码 (tlg={tlg_count}, ok={decoded})");
        eprintln!("title.pimg: TLG 资源 {tlg_count} 个, 成功解码 {decoded} 个");
    }
}
