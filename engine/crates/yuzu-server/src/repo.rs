//! 归档仓储层:按需打开并缓存 XP3,按条目名读取(数据访问)。
//!
//! 懒加载原则:归档只在首次被请求时打开并缓存;单条资源只在被请求时
//! 从归档中解压,不整包下发。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Result};
use serde::Serialize;
use yuzu_xp3::Xp3Archive;

/// 归档内的一个条目(文件)元信息。
#[derive(Debug, Clone, Serialize)]
pub struct EntryInfo {
    pub name: String,
    pub size: u64,
    pub packed_size: u64,
    pub hash: u32,
    pub segments: usize,
}

/// 数据目录下的一个归档。
#[derive(Debug, Clone, Serialize)]
pub struct ArchiveInfo {
    pub name: String,
    pub file_size: u64,
    /// 打开后惰性填充的条目数
    pub count: Option<usize>,
}

pub struct ArchiveRepo {
    data_dir: PathBuf,
    cache: Mutex<HashMap<String, Arc<Xp3Archive>>>,
}

impl ArchiveRepo {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        ArchiveRepo {
            data_dir: data_dir.into(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// 列出数据目录中的 .xp3 归档(只读文件名,不打开索引)。
    pub fn list_archives(&self) -> Vec<ArchiveInfo> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.data_dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xp3")) {
                    let name = p.file_name().unwrap().to_string_lossy().into_owned();
                    let file_size = p.metadata().map(|m| m.len()).unwrap_or(0);
                    out.push(ArchiveInfo {
                        name,
                        file_size,
                        count: None,
                    });
                }
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// 打开(并缓存)指定归档。`name` 经净化后仅作为数据目录内的文件名查找。
    pub fn open(&self, name: &str) -> Result<Arc<Xp3Archive>> {
        {
            let cache = self.cache.lock().unwrap();
            if let Some(arc) = cache.get(name) {
                return Ok(arc.clone());
            }
        }
        let path = self.data_dir.join(safe_name(name));
        if !path.is_file() {
            bail!("归档不存在: {name}");
        }
        let arc = Arc::new(Xp3Archive::open_file(&path)?);
        self.cache
            .lock()
            .unwrap()
            .insert(name.to_string(), arc.clone());
        Ok(arc)
    }

    /// 归档内条目列表。
    pub fn entries(&self, name: &str) -> Result<Vec<EntryInfo>> {
        let arc = self.open(name)?;
        Ok(arc
            .file_names()
            .map(|n| {
                let f = arc.file(n).expect("名称来自归档");
                EntryInfo {
                    name: n.to_string(),
                    size: f.size,
                    packed_size: f.packed_size,
                    hash: f.hash,
                    segments: f.segments.len(),
                }
            })
            .collect())
    }

    /// 惰性解压单个条目(只在被请求时读取该段)。
    /// 精确匹配优先;未命中做大小写不敏感回退 —— KiriKiri 归档引用大小写不敏感,
    /// 如场景命令 `bgm_BGM01I` 对应归档内 `BGM01i.opus`。
    pub fn read(&self, name: &str, path: &str) -> Result<Vec<u8>> {
        let arc = self.open(name)?;
        if let Some(bytes) = arc.read(path)? {
            return Ok(bytes);
        }
        if let Some(hit) = arc.file_names().find(|n| n.eq_ignore_ascii_case(path)) {
            if let Some(bytes) = arc.read(hit)? {
                return Ok(bytes);
            }
        }
        bail!("归档 {name} 内不存在 {path}")
    }
}

/// 只保留纯文件名,丢弃路径段(防 `../` 等)。
fn safe_name(name: &str) -> &str {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    if base.contains("..") || base.is_empty() {
        "_"
    } else {
        base
    }
}

/// 归档清单项(版本/热更新用):文件大小 + 修改时间作为变更指纹。
#[derive(Debug, Clone, Serialize)]
pub struct ManifestEntry {
    pub name: String,
    pub file_size: u64,
    pub modified: u64,
}

impl ArchiveRepo {
    /// 生成资源清单(轻量指纹:大小 + mtime),供客户端版本比对/热更新。
    pub fn manifest(&self) -> Vec<ManifestEntry> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.data_dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xp3")) {
                    if let Ok(m) = p.metadata() {
                        out.push(ManifestEntry {
                            name: p.file_name().unwrap().to_string_lossy().into_owned(),
                            file_size: m.len(),
                            modified: m.modified().map(|t| t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)).unwrap_or(0),
                        });
                    }
                }
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
}
