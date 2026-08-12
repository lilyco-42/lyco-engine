//! SCN 剧本模型(字段映射自 yuzuscn / FreeMote 反编译结果)。

use serde_json::Value;

/// 一部 SCN 文件。
#[derive(Debug, Clone)]
pub struct Scn {
    /// 文件名(storage)
    pub name: String,
    /// 哈希
    pub hash: Option<i64>,
    /// 场景列表
    pub scenes: Vec<Scene>,
}

impl Scn {
    pub fn is_empty(&self) -> bool {
        self.scenes.is_empty()
    }
    pub fn scene(&self, label: &str) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.label == label)
    }
}

/// 一个场景(label)。
#[derive(Debug, Clone)]
pub struct Scene {
    pub label: String,
    /// 标题(可能是语言数组)
    pub title: Vec<String>,
    /// 首个 .ks 行号
    pub first_line: i64,
    /// 快照点数量
    pub sp_count: i64,
    /// 剧本行
    pub lines: Vec<Line>,
    /// 文本池(lines 中的整数按此索引取文本)
    pub texts: Vec<Text>,
    /// 后续场景跳转
    pub nexts: Vec<Next>,
}

/// 剧本行。
#[derive(Debug, Clone)]
pub enum Line {
    /// 指向 `scene.texts[idx]` 的台词。
    Text(usize),
    /// 资源命令字符串(形如 "bg:xxx"、"chara:xxx")
    Resource(String),
    /// 引擎事件(playvoice / wait / envupdate / chapter …)
    Event(Event),
    /// 快照点行(SnapshotPointLine, 8 元组)
    SnapshotPoint(SnapshotPoint),
    /// 未知/保留
    Raw(Value),
}

/// 快照点行。
#[derive(Debug, Clone)]
pub struct SnapshotPoint {
    pub index: i64,
    pub text_index_a: Option<i64>,
    pub text_index_b: Option<i64>,
    pub flag: Option<i64>,
    pub original_line: i64,
}

/// 引擎事件。
#[derive(Debug, Clone)]
pub struct Event {
    /// 事件类型(str[0], 如 "playvoice"/"wait"/"chapter"/"envupdate")
    pub kind: String,
    /// 原始字段
    pub fields: Vec<Value>,
}

impl Event {
    pub fn get_str(&self, i: usize) -> Option<&str> {
        self.fields.get(i).and_then(Value::as_str)
    }
    pub fn get_i64(&self, i: usize) -> Option<i64> {
        self.fields.get(i).and_then(Value::as_i64)
    }
}

/// 文本池条目:角色 + 多语言台词。
#[derive(Debug, Clone)]
pub struct Text {
    pub character: Option<String>,
    pub dialogues: Vec<Dialogue>,
    pub voices: Vec<Voice>,
    pub value: Option<i64>,
}

/// 一句台词(可能多语言)。
#[derive(Debug, Clone)]
pub struct Dialogue {
    /// 显示名(如 "丛雨")
    pub name: Option<String>,
    /// 原文(content 可能为字符串或 {jp,en,cn,tw} 字典)
    pub content: Content,
}

/// 台词内容。
#[derive(Debug, Clone)]
pub enum Content {
    Plain(String),
    /// 多语言槽位
    Lang(LangMap),
}

/// 柚子社多语言槽位(常见顺序)。
#[derive(Debug, Clone, Default)]
pub struct LangMap {
    pub jp: Option<String>,
    pub en: Option<String>,
    pub cn: Option<String>,
    pub tw: Option<String>,
    /// 其它语言
    pub extra: Vec<(String, String)>,
}

impl LangMap {
    pub fn get(&self, lang: &str) -> Option<&str> {
        match lang {
            "jp" => self.jp.as_deref(),
            "en" => self.en.as_deref(),
            "cn" => self.cn.as_deref(),
            "tw" => self.tw.as_deref(),
            _ => self
                .extra
                .iter()
                .find(|(k, _)| k == lang)
                .map(|(_, v)| v.as_str()),
        }
    }
    /// 按优先级取任一可用文本(用于引擎展示)。
    pub fn first(&self, preferred: &[&str]) -> Option<String> {
        for lang in preferred {
            if let Some(s) = self.get(lang) {
                return Some(s.to_string());
            }
        }
        // 兜底
        if let Some(v) = [&self.jp, &self.en, &self.cn, &self.tw]
            .into_iter()
            .flatten()
            .next()
        {
            return Some(v.clone());
        }
        self.extra.first().map(|(_, v)| v.clone())
    }
}

/// 语音数据。
#[derive(Debug, Clone)]
pub struct Voice {
    pub name: Option<String>,
    pub voice: Option<String>,
}

/// 场景跳转(nexts)。
#[derive(Debug, Clone)]
pub struct Next {
    pub storage: String,
    pub target: String,
    pub kind: i64,
}
