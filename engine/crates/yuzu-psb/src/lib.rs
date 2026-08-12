//! # yuzu-psb
//!
//! M2 / EMT “Packaged Struct Binary”(PSB) 解析门面。
//!
//! 依赖 crates.io 的 [`emote-psb`](https://docs.rs/emote-psb)(MIT, storycraft 实现),
//! 将 PSB/MDF 二进制层解析为统一的 `serde_json::Value` 树,并把内嵌资源提取到独立切片,
//! 便于 WASM 线性内存直接访问。供 `yuzu-scn` / `yuzu-pimg` / `yuzu-engine` 使用。
//!
//! 千恋万花(柚子社 / KiriKiri Z)的 `.scn` 剧本、`.pimg` 立绘、`.psb` motion 均基于 PSB。
//! 资源引用在 JSON 中呈现为 `{"__PSB@RESOURCE": <索引>}` / `{"__PSB@EXTRA@RESOURCE": <索引>}`。

use std::io::{self, Cursor, Read};

/// PSB 解析错误(包装 IO / emote-psb 错误)。
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Open(String),
    Decode(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO: {e}"),
            Error::Open(e) => write!(f, "打开 PSB 失败: {e}"),
            Error::Decode(e) => write!(f, "解码 PSB 失败: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// 解析后的 PSB 文件:根值 + 内嵌资源。
pub struct PsbFile {
    version: u16,
    encrypted: bool,
    root: serde_json::Value,
    resources: Vec<Vec<u8>>,
    extra_resources: Vec<Vec<u8>>,
}

impl std::fmt::Debug for PsbFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PsbFile")
            .field("version", &self.version)
            .field("encrypted", &self.encrypted)
            .field("resources", &self.resources.len())
            .field("extra_resources", &self.extra_resources.len())
            .field("root", &self.root_type())
            .finish()
    }
}

impl PsbFile {
    /// 从完整文件字节解析 PSB(或 MDF 压缩容器)。
    pub fn parse(data: &[u8]) -> Result<Self> {
        let cursor = Cursor::new(data.to_vec());
        let mut inner = emote_psb::psb::read::PsbFile::open(cursor)
            .map_err(|e| Error::Open(e.to_string()))?;
        let root = inner
            .deserialize_root::<serde_json::Value>()
            .map_err(|e| Error::Decode(e.to_string()))?;

        let mut resources = Vec::with_capacity(inner.resources());
        for i in 0..inner.resources() {
            let mut buf = Vec::new();
            if let Some(mut s) = inner.open_resource(i).map_err(Error::Io)? {
                s.read_to_end(&mut buf)?;
            }
            resources.push(buf);
        }
        let mut extra = Vec::with_capacity(inner.extra_resources());
        for i in 0..inner.extra_resources() {
            let mut buf = Vec::new();
            if let Some(mut s) = inner.open_extra_resource(i).map_err(Error::Io)? {
                s.read_to_end(&mut buf)?;
            }
            extra.push(buf);
        }

        Ok(PsbFile {
            version: inner.version,
            encrypted: inner.encrypted,
            root,
            resources,
            extra_resources: extra,
        })
    }

    pub fn version(&self) -> u16 {
        self.version
    }

    pub fn encrypted(&self) -> bool {
        self.encrypted
    }

    /// 根值引用。
    pub fn root(&self) -> &serde_json::Value {
        &self.root
    }

    /// 资源索引 -> 二进制数据。
    pub fn resource(&self, id: u32) -> Option<&[u8]> {
        self.resources.get(id as usize).map(|v| v.as_slice())
    }

    /// extra(bstream)资源索引 -> 二进制数据。
    pub fn extra_resource(&self, id: u32) -> Option<&[u8]> {
        self.extra_resources.get(id as usize).map(|v| v.as_slice())
    }

    pub fn resources_len(&self) -> usize {
        self.resources.len()
    }

    pub fn extra_resources_len(&self) -> usize {
        self.extra_resources.len()
    }

    /// 资源引用标记解析:若值为 `{"__PSB@RESOURCE": idx}` 返回 Some(Resource(idx)),
    /// `{"__PSB@EXTRA@RESOURCE": idx}` 返回 Some(Extra(idx)),否则 None。
    pub fn resource_ref(v: &serde_json::Value) -> Option<ResourceRef> {
        if let serde_json::Value::Object(map) = v {
            if let Some(serde_json::Value::Number(n)) = map.get("__PSB@RESOURCE") {
                return Some(ResourceRef::Resource(n.as_u64().unwrap_or_default() as u32));
            }
            if let Some(serde_json::Value::Number(n)) = map.get("__PSB@EXTRA@RESOURCE") {
                return Some(ResourceRef::Extra(n.as_u64().unwrap_or_default() as u32));
            }
        }
        None
    }

    fn root_type(&self) -> &'static str {
        match self.root {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "bool",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        }
    }
}

/// 资源引用类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceRef {
    Resource(u32),
    Extra(u32),
}