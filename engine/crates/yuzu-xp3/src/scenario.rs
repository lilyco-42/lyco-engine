//! KiriKiri 场景文本编码(移植自 krkrz `base/TextStream.cpp`)。
//!
//! 游戏文本文件(`.ks` / `.csv` / `.stand` 等)头部:
//! ```text
//! fe fe | <mode> | ff fe | <UTF-16LE 文本>
//! ```
//! - `fe fe`  : 加密签名
//! - `<mode>` : 0 = 简单 XOR,1 = 相邻位交换,2 = zlib 压缩
//! - `ff fe`  : UTF-16LE BOM
//!
//! 文本按模式逐字符还原(字符 ≥ 0x20 才变换)。

use std::io::Read;

/// 解码 KiriKiri 场景文本。无 `fe fe` 头的普通文件返回 None。
pub fn decode(data: &[u8]) -> Option<String> {
    if data.len() < 5 || data[0] != 0xfe || data[1] != 0xfe {
        return None;
    }
    let mode = data[2];
    let text: Vec<u8> = match mode {
        0 | 1 => data[5..].to_vec(),
        2 => {
            // fe fe 02 ff fe + u64 compressed + u64 uncompressed + zlib 数据
            if data.len() < 5 + 8 + 8 {
                return None;
            }
            let comp = u64::from_le_bytes(data[5..13].try_into().ok()?);
            let uncomp = u64::from_le_bytes(data[13..21].try_into().ok()?);
            let comp = usize::try_from(comp).ok()?;
            let uncomp = usize::try_from(uncomp).ok()?;
            let raw = data.get(21..21 + comp)?;
            let mut dec = flate2::read::ZlibDecoder::new(raw);
            let mut buf = Vec::with_capacity(uncomp);
            dec.read_to_end(&mut buf).ok()?;
            if buf.len() != uncomp {
                return None;
            }
            buf
        }
        _ => return None,
    };

    let units: Vec<u16> = text
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let decoded: Vec<u16> = units
        .iter()
        .map(|&ch| match mode {
            0 => {
                if ch >= 0x20 {
                    ch ^ (((ch & 0xfe) << 8) ^ 1)
                } else {
                    ch
                }
            }
            _ => ((ch & 0xaaaa) >> 1) | ((ch & 0x5555) << 1),
        })
        .collect();
    Some(String::from_utf16_lossy(&decoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用真实文件解码验证(千恋万花 scenelist.csv,模式 1)。
    #[test]
    fn decodes_real_scenelist() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../realgame/scns/main/scenelist.csv");
        let data = std::fs::read(&path).expect("读取 scenelist.csv");
        let text = decode(&data).expect("应可解码");
        assert!(text.contains("回想モード一覧"), "应含真实标题: {text:?}");
        assert!(text.contains("thum_EV111"), "应含场景条目");
    }

    #[test]
    fn decodes_real_stand() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../realgame/mizuha.stand");
        let data = std::fs::read(&path).expect("读取 mizuha.stand");
        let text = decode(&data).expect("应可解码");
        assert!(text.contains("みづはa"), "应引用纹理集: {text:?}");
        assert!(text.contains("leveloffset"), "应含层级");
    }

    #[test]
    fn plain_text_returns_none() {
        assert!(decode(b"plain text no header").is_none());
        assert!(decode(&[]).is_none());
    }
}
