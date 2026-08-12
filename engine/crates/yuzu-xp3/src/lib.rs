//! # yuzu-xp3
//!
//! KiriKiri XP3 归档读取器 + 柚子社(YuzuSoft)加密方案(SenrenCxCrypt 等)。
//!
//! ## XP3 归档
//!
//! 实现现代 XP3(version 1/2)格式:索引为 `File` / `hnfn` / `eliF` 条目,
//! 每个 `File` 条目由 `info` / `segm` / `adlr` / `time` 分块组成;
//! 索引表本身可 zlib 压缩;文件数据按段存放,段可 zlib 压缩。
//! 布局与 [arc_unpacker](https://github.com/vn-tools/arc_unpacker)
//! `dec/kirikiri/xp3_archive_decoder.cc` 对齐,并用其测试夹具交叉验证。
//!
//! ## 柚子社加密
//!
//! [`crypt`] 模块移植自 GARbro:
//! - `CxScheme` — SenrenCxCrypt 的 xcode 虚拟机(CxEncryption),输出按位 XOR;
//! - `Riddle` — RiddleCxCrypt:先对前 8 字节按 adlr hash 派生密钥 XOR,
//!   再走 CxScheme;
//! - `YuzDecryptor` / `NanaDecryptor` — 文件名列表解密(ARX / LCG);
//! - 若干简单 XOR 方案(`xor`, `xor-mix`, `dieselmine`, `fsn` 等)。
//!
//! 解密时机说明:arc_unpacker 在**解压之后**对整个文件数据应用解密
//! (`decrypt_func(data, adlr_key)`)。本 crate 的 `read_decrypted` 遵循该约定;
//! 若目标游戏在压缩流上先解密后解压(部分 GARbro 方案如此),请改用
//! `read` 取原始字节后自行编排。

mod crypt;
pub mod scenario;

pub use crypt::{CxScheme, NanaDecryptor, Riddle, Scheme, Simple, SimpleKind, YuzDecryptor};

use std::fs;
use std::io::Read;
use std::path::Path;

/// XP3 文件头签名(11 字节)。
const XP3_MAGIC: &[u8; 11] = b"XP3\r\n\x20\x0a\x1a\x8b\x67\x01";

/// 解析/读取错误。
#[derive(Debug)]
pub enum Error {
    /// 不是 XP3(魔数不符)
    NotXp3,
    /// 底层 IO 失败
    Io(std::io::Error),
    /// 索引结构非法(截断 / 越界 / 未知条目)
    BadIndex(String),
    /// 文件数据非法(段越界 / 解压尺寸不符)
    Corrupt(String),
    /// 不支持的索引表压缩 / 段压缩
    Unsupported(String),
    /// 目标文件不存在
    NotFound,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotXp3 => write!(f, "不是 XP3 文件(魔数不符)"),
            Error::Io(e) => write!(f, "IO: {e}"),
            Error::BadIndex(s) => write!(f, "XP3 索引非法: {s}"),
            Error::Corrupt(s) => write!(f, "XP3 数据损坏: {s}"),
            Error::Unsupported(s) => write!(f, "不支持的 XP3 特性: {s}"),
            Error::NotFound => write!(f, "归档中不存在该文件"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// 单个段(文件数据的一个连续块)。
#[derive(Debug, Clone, Copy)]
pub struct Segment {
    /// 段在归档中的绝对偏移
    pub offset: u64,
    /// 解压后(实际)字节数
    pub size: u64,
    /// 压缩后字节数(未压缩时 == size)
    pub packed_size: u64,
    /// 段标志:低 3 位非 0 表示压缩(zlib)
    pub flags: u32,
}

impl Segment {
    pub fn is_compressed(&self) -> bool {
        self.flags & 0x7 != 0
    }
}

/// 归档内一个文件条目。
#[derive(Debug, Clone)]
pub struct Xp3File {
    /// 归档内路径(来自 `info` 文件名,或 `hnfn`/`eliF` 哈希映射)
    pub name: String,
    /// 原始(解压后)总字节数
    pub size: u64,
    /// 压缩后总字节数
    pub packed_size: u64,
    /// `adlr` 校验键(柚子社加密的密钥输入)
    pub hash: u32,
    /// 组成该文件的段
    pub segments: Vec<Segment>,
}

/// 已解析的 XP3 归档。
pub struct Xp3Archive {
    data: Vec<u8>,
    version: u8,
    files: Vec<Xp3File>,
}

impl std::fmt::Debug for Xp3Archive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Xp3Archive")
            .field("version", &self.version)
            .field("files", &self.files.len())
            .finish()
    }
}

impl Xp3Archive {
    /// 从完整归档字节解析(校验魔数与索引,不解压文件数据)。
    pub fn open(data: &[u8]) -> Result<Self> {
        if !data.starts_with(XP3_MAGIC) {
            return Err(Error::NotXp3);
        }
        let version = detect_version(data)?;
        let table_offset = table_offset(data, version)?;
        let files = parse_table(data, table_offset)?;
        Ok(Xp3Archive {
            data: data.to_vec(),
            version,
            files,
        })
    }

    /// 从文件路径读取并解析。
    pub fn open_file(path: impl AsRef<Path>) -> Result<Self> {
        let data = fs::read(path)?;
        Self::open(&data)
    }

    pub fn version(&self) -> u8 {
        self.version
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn file_names(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(|f| f.name.as_str())
    }

    pub fn file(&self, name: &str) -> Option<&Xp3File> {
        self.files.iter().find(|f| f.name == name)
    }

    /// 解压并返回文件原始字节(不解密)。
    pub fn read(&self, name: &str) -> Result<Option<Vec<u8>>> {
        let file = match self.file(name) {
            Some(f) => f,
            None => return Ok(None),
        };
        self.read_file(file).map(Some)
    }

    /// 解压 + 解密(`scheme.decrypt(data, file.hash)`,解压之后应用)。
    pub fn read_decrypted(&self, name: &str, scheme: &dyn Scheme) -> Result<Option<Vec<u8>>> {
        let file = match self.file(name) {
            Some(f) => f,
            None => return Ok(None),
        };
        let mut data = self.read_file(file)?;
        scheme.decrypt(&mut data, file.hash);
        Ok(Some(data))
    }

    fn read_file(&self, file: &Xp3File) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(file.size as usize);
        for seg in &file.segments {
            if seg.is_compressed() {
                let comp = self
                    .slice(seg.offset, seg.packed_size)
                    .ok_or_else(|| Error::Corrupt(format!("段越界: {:?}", seg)))?;
                let mut dec = flate2::read::ZlibDecoder::new(comp);
                let mut buf = Vec::with_capacity(seg.size as usize);
                dec.read_to_end(&mut buf)
                    .map_err(|e| Error::Corrupt(format!("zlib 解压失败: {e}")))?;
                if buf.len() != seg.size as usize {
                    return Err(Error::Corrupt(format!(
                        "段解压尺寸不符: 期望 {} 实际 {}",
                        seg.size,
                        buf.len()
                    )));
                }
                out.extend_from_slice(&buf);
            } else {
                let raw = self
                    .slice(seg.offset, seg.size)
                    .ok_or_else(|| Error::Corrupt(format!("段越界: {:?}", seg)))?;
                out.extend_from_slice(raw);
            }
        }
        if out.len() != file.size as usize {
            return Err(Error::Corrupt(format!(
                "文件尺寸不符: 期望 {} 实际 {}",
                file.size,
                out.len()
            )));
        }
        Ok(out)
    }

    fn slice(&self, offset: u64, len: u64) -> Option<&[u8]> {
        let start = usize::try_from(offset).ok()?;
        let end = usize::try_from(offset.checked_add(len)?).ok()?;
        self.data.get(start..end)
    }
}

// ---------- 头与版本 ----------

/// version 1/2 判别:偏移 19 处 u32 == 1 为 version 2。
fn detect_version(data: &[u8]) -> Result<u8> {
    if data.len() < 23 {
        return Err(Error::BadIndex("文件过短".into()));
    }
    Ok(if read_u32(data, 19) == 1 { 2 } else { 1 })
}

/// 索引表偏移:
/// - v1:文件头偏移 11 处直接为表偏移;
/// - v2:偏移 11 为附加头偏移,附加头内跳过 flags+表大小后为表偏移。
fn table_offset(data: &[u8], version: u8) -> Result<u64> {
    if version == 1 {
        return Ok(read_u64(data, 11));
    }
    let add = read_u64(data, 11);
    let minor = read_u32(data, 19);
    if minor != 1 {
        return Err(Error::BadIndex(format!("未知 XP3 次版本 {minor}")));
    }
    let pos = usize::try_from(add).map_err(|_| Error::BadIndex("附加头偏移越界".into()))?;
    if pos + 17 > data.len() {
        return Err(Error::BadIndex("附加头越界".into()));
    }
    Ok(read_u64(data, pos + 9)) // skip 1 (flags) + skip 8 (table size) → +9
}

// ---------- 索引表解析 ----------

/// 读取索引表(可压缩),产出文件条目列表。
fn parse_table(data: &[u8], table_offset: u64) -> Result<Vec<Xp3File>> {
    let mut pos =
        usize::try_from(table_offset).map_err(|_| Error::BadIndex("表偏移越界".into()))?;
    let is_compressed = *data
        .get(pos)
        .ok_or_else(|| Error::BadIndex("索引表截断".into()))?;
    pos += 1;
    let size_comp = read_u64_at(data, &mut pos)?;
    let size_orig = if is_compressed != 0 {
        read_u64_at(data, &mut pos)?
    } else {
        size_comp
    };

    let raw = data
        .get(
            pos..pos
                .checked_add(size_comp as usize)
                .ok_or_else(|| Error::BadIndex("表大小溢出".into()))?,
        )
        .ok_or_else(|| Error::BadIndex("索引表截断".into()))?;
    let table = if is_compressed != 0 {
        let mut dec = flate2::read::ZlibDecoder::new(raw);
        let mut buf = Vec::with_capacity(size_orig as usize);
        dec.read_to_end(&mut buf)
            .map_err(|e| Error::BadIndex(format!("索引表解压失败: {e}")))?;
        if buf.len() != size_orig as usize {
            return Err(Error::BadIndex(format!(
                "索引表解压尺寸不符: 期望 {size_orig} 实际 {}",
                buf.len()
            )));
        }
        buf
    } else {
        raw.to_vec()
    };

    parse_table_entries(&table)
}

/// 依次读取表内条目:`File`(文件)、`hnfn`/`eliF`(哈希→文件名映射)。
fn parse_table_entries(table: &[u8]) -> Result<Vec<Xp3File>> {
    let mut fn_map: std::collections::HashMap<u32, String> = Default::default();
    let mut files = Vec::new();
    let mut pos = 0usize;
    while pos < table.len() {
        let magic = table
            .get(pos..pos + 4)
            .ok_or_else(|| Error::BadIndex("条目魔数截断".into()))?;
        let entry_size = read_u64(table, pos + 4) as usize;
        pos += 12;
        let entry = table
            .get(pos..pos + entry_size)
            .ok_or_else(|| Error::BadIndex("条目数据越界".into()))?;
        pos += entry_size;
        match magic {
            b"File" => {
                let file = parse_file_entry(entry, &fn_map)?;
                files.push(file);
            }
            b"hnfn" | b"eliF" => {
                if entry.len() < 6 {
                    continue;
                }
                let hash = read_u32(entry, 0);
                let name_len = read_u16(entry, 4) as usize;
                if name_len * 2 + 6 <= entry.len() {
                    let name = utf16le_to_string(&entry[6..6 + name_len * 2]);
                    fn_map.insert(hash, name);
                }
            }
            other => {
                return Err(Error::Unsupported(format!(
                    "未知索引条目: {}",
                    String::from_utf8_lossy(other)
                )));
            }
        }
    }
    Ok(files)
}

/// 解析单个 `File` 条目(内部为 `info`/`segm`/`adlr`/`time` 分块)。
fn parse_file_entry(
    entry: &[u8],
    fn_map: &std::collections::HashMap<u32, String>,
) -> Result<Xp3File> {
    let mut name: Option<String> = None;
    let mut size: u64 = 0;
    let mut packed_size: u64 = 0;
    let mut hash: u32 = 0;
    let mut segments: Vec<Segment> = Vec::new();

    let mut pos = 0usize;
    while pos < entry.len() {
        let chunk_magic = entry
            .get(pos..pos + 4)
            .ok_or_else(|| Error::BadIndex("分块魔数截断".into()))?;
        let chunk_size = read_u64(entry, pos + 4) as usize;
        pos += 12;
        let chunk = entry
            .get(pos..pos + chunk_size)
            .ok_or_else(|| Error::BadIndex("分块数据越界".into()))?;
        pos += chunk_size;
        match chunk_magic {
            b"info" => {
                if chunk.len() < 4 + 8 + 8 + 2 {
                    return Err(Error::BadIndex("info 分块过短".into()));
                }
                size = read_u64(chunk, 4);
                packed_size = read_u64(chunk, 12);
                let name_len = read_u16(chunk, 20) as usize;
                if 22 + name_len * 2 <= chunk.len() {
                    name = Some(utf16le_to_string(&chunk[22..22 + name_len * 2]));
                }
            }
            b"segm" => {
                let mut cpos = 0usize;
                while cpos + 28 <= chunk.len() {
                    let flags = read_u32(chunk, cpos);
                    let offset = read_u64(chunk, cpos + 4);
                    let size_orig = read_u64(chunk, cpos + 12);
                    let size_comp = read_u64(chunk, cpos + 20);
                    segments.push(Segment {
                        offset,
                        size: size_orig,
                        packed_size: size_comp,
                        flags,
                    });
                    cpos += 28;
                }
            }
            b"adlr" => {
                if chunk.len() >= 4 {
                    hash = read_u32(chunk, 0);
                }
            }
            b"time" => { /* 时间戳,读取时忽略 */ }
            _ => { /* 未知分块,跳过 */ }
        }
    }

    let name = match name {
        Some(n) => n,
        None => fn_map
            .get(&hash)
            .cloned()
            .ok_or_else(|| Error::BadIndex("File 条目缺少 info 文件名且无哈希映射".into()))?,
    };
    Ok(Xp3File {
        name,
        size,
        packed_size,
        hash,
        segments,
    })
}

// ---------- 小端读取 ----------

fn read_u16(b: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes([b[pos], b[pos + 1]])
}
fn read_u32(b: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes([b[pos], b[pos + 1], b[pos + 2], b[pos + 3]])
}
fn read_u64(b: &[u8], pos: usize) -> u64 {
    u64::from_le_bytes([
        b[pos],
        b[pos + 1],
        b[pos + 2],
        b[pos + 3],
        b[pos + 4],
        b[pos + 5],
        b[pos + 6],
        b[pos + 7],
    ])
}

/// 带边界检查的顺序读取。
fn read_u64_at(data: &[u8], pos: &mut usize) -> Result<u64> {
    let end = pos
        .checked_add(8)
        .filter(|&e| e <= data.len())
        .ok_or_else(|| Error::BadIndex("索引表截断".into()))?;
    let v = read_u64(data, *pos);
    *pos = end;
    Ok(v)
}

fn utf16le_to_string(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}
