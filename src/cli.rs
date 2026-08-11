//! 命令行入口：clap 定义与命令分发。

use crate::config::{self, Config, TemplateEntry, TemplateEntrySource};
use crate::fetch;
use crate::manifest::{self, TemplateManifest};
use crate::params;
use crate::render::{self as render_mod, RenderOptions};
use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

#[derive(Parser)]
#[command(
    name = "lyco",
    version,
    about = "统一多语言脚手架工具：gh api 驱动模板 + 声明式渲染"
)]
pub struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    /// 从模板一键生成新项目
    New {
        /// 项目名（同时是输出目录名，默认 ./<name>）
        name: String,
        /// 模板 id（见 `lyco template list`），默认 rust-android
        #[arg(short, long, default_value = "rust-android")]
        template: String,
        /// 覆盖参数，可多次：--param package=com.x.y
        #[arg(long = "param", value_name = "k=v")]
        params: Vec<String>,
        /// 全部使用默认值，跳过交互提问
        #[arg(short, long)]
        yes: bool,
        /// 重新下载模板（忽略缓存）
        #[arg(long)]
        refresh: bool,
        /// 输出目录（默认 ./<name>）
        #[arg(short, long)]
        output: Option<String>,
        /// 目标目录已存在时直接覆盖
        #[arg(long)]
        force: bool,
    },
    /// 管理模板
    Template {
        #[command(subcommand)]
        action: TemplateAction,
    },
    /// 查看配置与缓存路径
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum TemplateAction {
    /// 列出已注册模板
    List {
        /// 同时通过 gh api 展示 GitHub 上可用的模板候选仓库
        #[arg(long)]
        remote: bool,
    },
    /// 注册一个 GitHub 模板仓库：lyco template add <owner/repo> [--name <id>]
    Add {
        repo: String,
        /// 注册 id（默认取仓库名）
        #[arg(long)]
        name: Option<String>,
    },
    /// 注册一个本地目录模板：lyco template add-local <path> --name <id>
    AddLocal {
        path: String,
        #[arg(long)]
        name: String,
    },
    /// 注销模板并清理缓存
    Remove { name: String },
    /// 刷新仓库模板缓存
    Refresh {
        /// 不填则刷新全部仓库模板
        name: Option<String>,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// 打印配置与缓存路径
    Path,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        CliCommand::New {
            name,
            template,
            params,
            yes,
            refresh,
            output,
            force,
        } => cmd_new(name, template, params, yes, refresh, output, force),
        CliCommand::Template { action } => match action {
            TemplateAction::List { remote } => cmd_template_list(remote),
            TemplateAction::Add { repo, name } => cmd_template_add(repo, name),
            TemplateAction::AddLocal { path, name } => cmd_template_add_local(path, name),
            TemplateAction::Remove { name } => cmd_template_remove(name),
            TemplateAction::Refresh { name } => cmd_template_refresh(name),
        },
        CliCommand::Config {
            action: ConfigAction::Path,
        } => {
            println!("配置: {}", config::config_path().display());
            println!("缓存: {}", config::cache_root().display());
            Ok(())
        }
    }
}

// ---------- lyco new ----------

fn cmd_new(
    name: String,
    template: String,
    params: Vec<String>,
    yes: bool,
    refresh: bool,
    output: Option<String>,
    force: bool,
) -> Result<()> {
    let output_dir = PathBuf::from(output.unwrap_or_else(|| name.clone()));
    if output_dir.as_os_str().is_empty() {
        bail!("项目名不能为空");
    }

    let cfg = Config::load()?;
    let entry = cfg
        .templates
        .get(&template)
        .ok_or_else(|| anyhow!("未知模板 `{template}`（可用: lyco template list）"))?;

    let template_root = prepare_template_root(entry, &template, refresh)?;
    let manifest = resolve_manifest(entry, &template_root, &template)?;

    let overrides = parse_params(&params)?;
    let values = params::collect(&manifest, &name, &overrides, yes)?;

    if output_dir.exists() {
        if force {
            fs::remove_dir_all(&output_dir)?;
        } else {
            bail!("目录已存在: {}（使用 --force 覆盖）", output_dir.display());
        }
    }

    let created = render_mod::render(&RenderOptions {
        template_root: &template_root,
        output_dir: &output_dir,
        manifest: &manifest,
        values: &values,
    })?;
    render_mod::run_post(&manifest, &values, &output_dir)?;

    print_summary(&manifest, &values, &output_dir, &created);
    Ok(())
}

/// 取到模板源码根：仓库 → 缓存目录（必要时下载）；本地 → 源目录。
fn prepare_template_root(entry: &TemplateEntry, id: &str, refresh: bool) -> Result<PathBuf> {
    match &entry.source {
        TemplateEntrySource::Repo { repo } => {
            let src = config::cache_root().join(id).join("src");
            if refresh || !src.exists() {
                println!("{} 下载模板: gh api repos/{repo}/tarball", "[fetch]".cyan());
                fetch::fetch_repo(repo, &src)?;
            } else {
                println!("{} 使用缓存模板: {}", "[cache]".cyan(), src.display());
            }
            Ok(src)
        }
        TemplateEntrySource::Local { path } => {
            let p = PathBuf::from(path);
            if !p.is_dir() {
                bail!("本地模板目录不存在: {}", p.display());
            }
            Ok(p)
        }
    }
}

/// 清单优先级：配置内联 > 模板根 template.yaml > 自动纯拷贝。
fn resolve_manifest(
    entry: &TemplateEntry,
    template_root: &Path,
    id: &str,
) -> Result<TemplateManifest> {
    if let Some(m) = &entry.manifest {
        return Ok(m.clone());
    }
    let f = template_root.join("template.yaml");
    if f.exists() {
        return TemplateManifest::load_from_file(&f);
    }
    Ok(manifest::auto(id))
}

fn parse_params(items: &[String]) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for it in items {
        match it.split_once('=') {
            Some((k, v)) => out.push((k.trim().to_string(), v.to_string())),
            None => bail!("--param 格式应为 k=v，收到: {it}"),
        }
    }
    Ok(out)
}

fn print_summary(
    manifest: &TemplateManifest,
    values: &HashMap<String, String>,
    output_dir: &Path,
    created: &[PathBuf],
) {
    println!();
    println!("{} 项目已生成: {}", "✓".green(), output_dir.display());
    println!("  模板: {} · 语言: {}", manifest.id, manifest.language);
    for p in &manifest.params {
        let v = values.get(&p.key).cloned().unwrap_or_default();
        println!("  · {} = {}", p.key, v.cyan());
    }
    println!("  文件数: {}", created.len());
    if !manifest.package_managers.is_empty() {
        println!("  包管理器: {}", manifest.package_managers.join(", "));
    }
    if !manifest.next_steps.is_empty() {
        println!();
        println!("{} 下一步:", "→".yellow());
        for s in &manifest.next_steps {
            println!("  $ {}", params::resolve(s, values));
        }
    }
}

// ---------- lyco template ----------

fn cmd_template_list(remote: bool) -> Result<()> {
    let cfg = Config::load()?;
    println!("{} 已注册模板 ({})", "本地".green(), cfg.templates.len());
    for (id, entry) in &cfg.templates {
        let source = match &entry.source {
            TemplateEntrySource::Repo { repo } => repo.clone(),
            TemplateEntrySource::Local { path } => format!("local:{path}"),
        };
        let desc = entry
            .manifest
            .as_ref()
            .map(|m| m.description.clone())
            .unwrap_or_default();
        println!("  {:<16} {:<40} {}", id.green(), source, desc);
    }
    if !remote {
        println!();
        println!(
            "提示: 用 {} 查看 GitHub 候选模板仓库。",
            "lyco template list --remote".cyan()
        );
        return Ok(());
    }
    println!();
    println!("{} GitHub 候选模板仓库 (gh api user/repos)", "远程".green());
    let out = Process::new("gh")
        .args([
            "api",
            "user/repos",
            "--paginate",
            "--jq",
            ".[] | [.full_name, (.description // \"\"), (.language // \"\")] | @tsv",
        ])
        .output()
        .context("调用 gh api 失败，请确认 gh 已登录")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("gh api 返回错误: {}", err.trim());
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    let mut shown = 0;
    for line in lines {
        let parts: Vec<&str> = line.split('\t').collect();
        let (full, desc, lang) = match parts.as_slice() {
            [full, desc, lang] => (*full, *desc, *lang),
            [full] => (*full, "", ""),
            _ => continue,
        };
        let is_tpl =
            full.to_lowercase().contains("template") || desc.to_lowercase().contains("template");
        if is_tpl {
            println!("  {:<40} {:<10} {}", full.green(), lang, desc);
            shown += 1;
        }
    }
    println!("  （共列出 {shown} 个含 template 的仓库；任意仓库都可通过 `lyco template add <owner>/<repo>` 注册）");
    Ok(())
}

fn cmd_template_add(repo: String, name: Option<String>) -> Result<()> {
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!("格式应为 <owner>/<repo>，收到: {repo}");
    }
    let out = Process::new("gh")
        .args(["api", &format!("repos/{repo}"), "--jq", ".full_name"])
        .output()
        .context("调用 gh api 失败，请确认 gh 已登录")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("仓库不存在或无权限 {repo}: {}", err.trim());
    }

    let id = name.unwrap_or_else(|| parts[1].to_string());
    let mut cfg = Config::load()?;
    cfg.templates.insert(
        id.clone(),
        TemplateEntry {
            source: TemplateEntrySource::Repo { repo },
            manifest: None,
        },
    );
    cfg.save()?;
    println!(
        "{} 已注册模板 `{id}`（若仓库根目录有 template.yaml 将自动采用；否则按纯拷贝渲染）",
        "✓".green()
    );
    println!(
        "  编辑配置以添加内联清单: {}",
        config::config_path().display()
    );
    Ok(())
}

fn cmd_template_add_local(path: String, name: String) -> Result<()> {
    let p = PathBuf::from(&path);
    if !p.is_dir() {
        bail!("本地模板目录不存在: {}", p.display());
    }
    let mut cfg = Config::load()?;
    cfg.templates.insert(
        name.clone(),
        TemplateEntry {
            source: TemplateEntrySource::Local { path },
            manifest: None,
        },
    );
    cfg.save()?;
    println!("{} 已注册本地模板 `{name}`", "✓".green());
    Ok(())
}

fn cmd_template_remove(name: String) -> Result<()> {
    let mut cfg = Config::load()?;
    if cfg.templates.remove(&name).is_none() {
        bail!("未注册的模板: {name}");
    }
    cfg.save()?;
    let cache = config::cache_root().join(&name);
    if cache.exists() {
        fs::remove_dir_all(&cache)?;
    }
    println!("{} 已注销模板 `{name}`", "✓".green());
    Ok(())
}

fn cmd_template_refresh(name: Option<String>) -> Result<()> {
    let cfg = Config::load()?;
    let targets: Vec<(String, String)> = cfg
        .templates
        .iter()
        .filter_map(|(id, entry)| match &entry.source {
            TemplateEntrySource::Repo { repo } => {
                if name.is_none() || name.as_deref() == Some(id.as_str()) {
                    Some((id.clone(), repo.clone()))
                } else {
                    None
                }
            }
            TemplateEntrySource::Local { .. } => None,
        })
        .collect();
    if targets.is_empty() {
        bail!("没有可刷新的仓库模板");
    }
    for (id, repo) in targets {
        let src = config::cache_root().join(&id).join("src");
        println!(
            "{} 刷新 {id} (gh api repos/{repo}/tarball)",
            "[fetch]".cyan()
        );
        fetch::fetch_repo(&repo, &src)?;
    }
    println!("{} 完成", "✓".green());
    Ok(())
}
