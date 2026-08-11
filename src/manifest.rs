//! 声明式模板清单 (template.yaml) 定义与解析。
//!
//! 一个模板由一个清单描述：参数、内容替换规则、路径重命名、删除、post 钩子。
//! 清单优先取 `~/.config/lyco/templates.yaml` 中内联的 `manifest:` 段，
//! 其次取模板仓库根目录的 `template.yaml`，最后回退为“纯拷贝”自动清单。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TemplateManifest {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// 主语言，例如 rust / java / kotlin / cpp
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// 模板使用的包管理器，例如 gradle / cargo / npm
    #[serde(default)]
    pub package_managers: Vec<String>,
    /// 生成时收集的参数（交互提示 / --param 覆盖 / 默认值）
    #[serde(default)]
    pub params: Vec<Param>,
    /// 内容占位符替换规则，按声明顺序依次应用
    #[serde(default)]
    pub replace: Vec<ReplaceRule>,
    /// 路径重命名规则（`from` 为原始相对路径，`to` 支持 {{模板}}）
    #[serde(default)]
    pub rename: Vec<RenameRule>,
    /// 从输出中删除的模板文件（相对路径）
    #[serde(default)]
    pub delete: Vec<String>,
    /// 生成后执行的钩子命令
    #[serde(default)]
    pub post: Vec<PostStep>,
    /// 生成完成后的“下一步”提示
    #[serde(default)]
    pub next_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub key: String,
    #[serde(default)]
    pub label: String,
    /// 默认值，支持 {{name}} / {{其他参数}} / 过滤器
    pub default: String,
    #[serde(default)]
    pub help: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaceRule {
    /// 原始占位符（字面字符串，非正则）
    pub find: String,
    /// 替换值，支持 {{param|filter}}
    pub with: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameRule {
    /// 原始相对路径（重命名前的路径）
    pub from: String,
    /// 目标相对路径，支持 {{param|filter}}
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostStep {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// 在生成目录下的子目录中执行
    #[serde(default)]
    pub cwd: Option<String>,
}

impl TemplateManifest {
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let s = std::fs::read_to_string(path)
            .with_context(|| format!("读取模板清单失败: {}", path.display()))?;
        let m: TemplateManifest = serde_yaml_ng::from_str(&s)
            .with_context(|| format!("解析模板清单失败: {}", path.display()))?;
        Ok(m)
    }
}

/// 回退用的“纯拷贝”自动清单：无参数、无替换，仅把模板目录拷到目标。
pub fn auto(id: &str) -> TemplateManifest {
    TemplateManifest {
        id: id.to_string(),
        name: id.to_string(),
        description: "自动生成的纯拷贝模板（编辑配置文件添加 replace/rename 规则）".to_string(),
        ..Default::default()
    }
}
