//! 模板注册表持久化：`~/.config/lyco/templates.yaml`。
//!
//! 每个注册项指向一个来源（GitHub 仓库或本地目录），并可携带内联清单覆盖
//! （优先级高于模板仓库自带的 template.yaml）。

use crate::manifest::{Param, RenameRule, ReplaceRule, TemplateManifest};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "lowercase")]
pub enum TemplateEntrySource {
    /// GitHub 仓库，通过 `gh api repos/{repo}/tarball` 获取
    Repo { repo: String },
    /// 本地目录
    Local { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateEntry {
    /// 来源（内部标签枚举，扁平化为 `source: repo|local` + 变体字段）
    #[serde(flatten)]
    pub source: TemplateEntrySource,
    /// 内联清单（本地覆盖，优先于模板仓库内的 template.yaml）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<TemplateManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub templates: BTreeMap<String, TemplateEntry>,
}

/// 配置文件路径：`{config_dir}/lyco/templates.yaml`；可用 `LYCO_CONFIG` 覆盖。
pub fn config_path() -> PathBuf {
    std::env::var("LYCO_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("lyco")
                .join("templates.yaml")
        })
}

/// 模板缓存根：`{cache_dir}/lyco/templates/<id>/src`；可用 `LYCO_CACHE` 覆盖。
pub fn cache_root() -> PathBuf {
    std::env::var("LYCO_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("lyco")
                .join("templates")
        })
}

impl Config {
    /// 载入配置；不存在时写入内置默认模板并返回。
    pub fn load() -> Result<Config> {
        let p = config_path();
        if !p.exists() {
            let cfg = Config::defaults();
            cfg.save()?;
            return Ok(cfg);
        }
        let s = fs::read_to_string(&p).with_context(|| format!("读取配置失败: {}", p.display()))?;
        let cfg: Config = serde_yaml_ng::from_str(&s)
            .with_context(|| format!("解析配置失败: {}", p.display()))?;
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        let p = config_path();
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir)?;
        }
        let s = serde_yaml_ng::to_string(self)?;
        // 原子写：先写临时文件再替换，避免进程中断导致配置半写损坏
        let tmp = p.with_extension("yaml.tmp");
        fs::write(&tmp, &s)?;
        if let Err(e) = fs::rename(&tmp, &p) {
            // Windows: rename 不允许覆盖已存在文件，先删再换
            let _ = fs::remove_file(&p);
            fs::rename(&tmp, &p).map_err(|_| e)?;
        }
        Ok(())
    }

    /// 内置默认模板（取自你的现有仓库；其仓库内暂无 template.yaml，故内联清单）。
    fn defaults() -> Config {
        let mut templates = BTreeMap::new();
        templates.insert(
            "rust-android".to_string(),
            TemplateEntry {
                source: TemplateEntrySource::Repo {
                    repo: "lilyco-42/rust-android-template".to_string(),
                },
                manifest: Some(default_rust_android()),
            },
        );
        templates.insert(
            "fabric-mod".to_string(),
            TemplateEntry {
                source: TemplateEntrySource::Repo {
                    repo: "lilyco-42/fabric-mod-template".to_string(),
                },
                manifest: Some(default_fabric_mod()),
            },
        );
        Config { templates }
    }
}

fn default_rust_android() -> TemplateManifest {
    TemplateManifest {
        id: "rust-android".into(),
        name: "Rust + Kotlin Android (Compose)".into(),
        description: "Rust + Kotlin Android 项目 · JNI 桥接 · Jetpack Compose · 开箱即用".into(),
        language: "rust".into(),
        tags: vec!["android".into(), "compose".into(), "jni".into()],
        package_managers: vec!["gradle".into(), "cargo".into()],
        params: vec![
            Param {
                key: "package".into(),
                label: "包名".into(),
                default: "com.example.app".into(),
                help: "例如 com.mycompany.myapp".into(),
                required: true,
            },
            Param {
                key: "appname".into(),
                label: "应用名".into(),
                default: "{{name|pascal}}".into(),
                help: "桌面显示的应用名".into(),
                required: false,
            },
        ],
        replace: vec![
            ReplaceRule {
                find: "com.example.app".into(),
                with: "{{package}}".into(),
            },
            ReplaceRule {
                find: "com_example_app".into(),
                with: "{{package|rust}}".into(),
            },
            ReplaceRule {
                find: "com/example/app".into(),
                with: "{{package|path}}".into(),
            },
            ReplaceRule {
                find: "Rust App".into(),
                with: "{{appname}}".into(),
            },
        ],
        rename: vec![RenameRule {
            from: "app/src/main/java/com/example/app".into(),
            to: "app/src/main/java/{{package|path}}".into(),
        }],
        delete: vec!["README.md".into()],
        post: vec![],
        next_steps: vec![
            "cd {{name}}/native_lib && cargo ndk --target aarch64-linux-android --platform 24 -- build --release".into(),
            "cd {{name}} && ./gradlew assembleRelease".into(),
        ],
    }
}

fn default_fabric_mod() -> TemplateManifest {
    TemplateManifest {
        id: "fabric-mod".into(),
        name: "Fabric 1.21.4 Mod".into(),
        description: "Minecraft Fabric 客户端 Mod · Mixin · 双语 lang".into(),
        language: "java".into(),
        tags: vec!["minecraft".into(), "mod".into(), "fabric".into()],
        package_managers: vec!["gradle".into()],
        params: vec![
            Param {
                key: "mod_id".into(),
                label: "Mod ID".into(),
                default: "{{name|kebab}}".into(),
                help: "小写，例如 auto-sprint".into(),
                required: true,
            },
            Param {
                key: "mod_name".into(),
                label: "Mod 名称".into(),
                default: "{{name|space}}".into(),
                help: "例如 Auto Sprint".into(),
                required: false,
            },
            Param {
                key: "package".into(),
                label: "Java 包名".into(),
                default: "com.example.{{name|snake}}".into(),
                help: "例如 com.example.autosprint".into(),
                required: true,
            },
            Param {
                key: "class_name".into(),
                label: "主类名".into(),
                default: "{{name|pascal}}Mod".into(),
                help: "例如 AutoSprintMod".into(),
                required: false,
            },
        ],
        replace: vec![
            // 顺序敏感：com.example.template 必须先于 com.example
            ReplaceRule {
                find: "template-mod".into(),
                with: "{{mod_id}}".into(),
            },
            ReplaceRule {
                find: "Template Mod".into(),
                with: "{{mod_name}}".into(),
            },
            ReplaceRule {
                find: "com.example.template".into(),
                with: "{{package}}".into(),
            },
            ReplaceRule {
                find: "com.example".into(),
                with: "{{package|parent}}".into(),
            },
            ReplaceRule {
                find: "TemplateMod".into(),
                with: "{{class_name}}".into(),
            },
        ],
        rename: vec![
            RenameRule {
                from: "src/main/java/com/example/template/TemplateMod.java".into(),
                to: "src/main/java/{{package|path}}/{{class_name}}.java".into(),
            },
            RenameRule {
                from: "src/main/java/com/example/template".into(),
                to: "src/main/java/{{package|path}}".into(),
            },
            RenameRule {
                from: "src/main/resources/assets/template-mod".into(),
                to: "src/main/resources/assets/{{mod_id}}".into(),
            },
            RenameRule {
                from: "src/main/resources/template-mod.mixins.json".into(),
                to: "src/main/resources/{{mod_id}}.mixins.json".into(),
            },
        ],
        delete: vec!["README.md".into()],
        post: vec![],
        next_steps: vec![
            "cd {{name}} && ./gradlew build".into(),
            "cd {{name}} && ./gradlew runClient".into(),
        ],
    }
}
