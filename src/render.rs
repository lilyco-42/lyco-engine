//! 模板渲染：按清单应用内容替换、路径重命名、删除，并执行 post 钩子。

use crate::manifest::TemplateManifest;
use crate::params;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

pub struct RenderOptions<'a> {
    pub template_root: &'a Path,
    pub output_dir: &'a Path,
    pub manifest: &'a TemplateManifest,
    pub values: &'a HashMap<String, String>,
}

/// 把模板渲染到输出目录，返回生成的文件列表。
pub fn render(opts: &RenderOptions) -> Result<Vec<PathBuf>> {
    let values = opts.values;

    // 重命名规则：解析 from/to 并按 from 长度降序（最长前缀优先）
    let mut renames: Vec<(String, String)> = Vec::new();
    for r in &opts.manifest.rename {
        let from = params::resolve(&r.from, values);
        let to = params::resolve(&r.to, values);
        if from != to {
            renames.push((from, to));
        }
    }
    renames.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    let src = opts.template_root;
    let dst = opts.output_dir;
    fs::create_dir_all(dst)?;

    let mut created = Vec::new();

    for entry in WalkDir::new(src).min_depth(1) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src)?.to_path_buf();
        if is_skipped(&rel) {
            continue;
        }
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        // delete 规则：精确匹配文件，或以 `d/` 前缀匹配整个子树（目录级联删除）
        if is_deleted(&rel_str, &opts.manifest.delete) {
            continue;
        }

        let new_rel = apply_rename(&rel_str, &renames);
        let out = dst.join(new_rel);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }

        let data = fs::read(entry.path())?;
        if is_binary(&data) {
            fs::write(&out, &data)?;
        } else {
            let text = String::from_utf8_lossy(&data);
            let mut rendered = text.to_string();
            for rule in &opts.manifest.replace {
                let with = params::resolve(&rule.with, values);
                if with != rule.find {
                    rendered = rendered.replace(&rule.find, &with);
                }
            }
            fs::write(&out, rendered)?;
        }

        #[cfg(unix)]
        make_executable(&out, &rel_str);
        created.push(out);
    }

    prune_empty_dirs(dst);
    Ok(created)
}

/// 自底向上删除空目录（重命名后遗留的旧祖先目录；模板若需保留空目录应放置 .gitkeep）。
fn prune_empty_dirs(root: &Path) {
    let mut dirs: Vec<PathBuf> = WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
        .map(|e| e.into_path())
        .collect();
    dirs.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for d in dirs {
        let empty = std::fs::read_dir(&d)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false);
        if empty {
            let _ = std::fs::remove_dir(&d);
        }
    }
}

/// 生成后执行 post 钩子（在输出目录内）。
pub fn run_post(
    manifest: &TemplateManifest,
    values: &HashMap<String, String>,
    output_dir: &Path,
) -> Result<()> {
    for step in &manifest.post {
        let cmd = params::resolve(&step.command, values);
        let args: Vec<String> = step
            .args
            .iter()
            .map(|a| params::resolve(a, values))
            .collect();
        let cwd = match &step.cwd {
            Some(c) => output_dir.join(params::resolve(c, values)),
            None => output_dir.to_path_buf(),
        };
        if !cwd.is_dir() {
            bail!(
                "post 钩子 cwd 目录不存在: {}（空目录会被清理，需放 .gitkeep 保留）",
                cwd.display()
            );
        }
        let status = Command::new(&cmd)
            .args(&args)
            .current_dir(&cwd)
            .status()
            .with_context(|| format!("执行 post 钩子失败: {cmd}"))?;
        if !status.success() {
            bail!("post 钩子返回非零状态: {cmd} {args:?}");
        }
    }
    Ok(())
}

/// 跳过模板元数据：.git 与清单本身。
fn is_skipped(rel: &Path) -> bool {
    let first = rel
        .components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .unwrap_or_default();
    if first == ".git" {
        return true;
    }
    let s = rel.to_string_lossy().replace('\\', "/");
    s == "template.yaml" || s == "lyco.yaml"
}

/// delete 规则匹配：`d` 精确匹配文件，或以 `d/` 为前缀级联删除子树（容忍尾部斜杠）。
fn is_deleted(rel: &str, delete: &[String]) -> bool {
    delete.iter().any(|d| {
        let d = d.trim_end_matches('/');
        rel == d || rel.starts_with(&format!("{d}/"))
    })
}

/// 最长前缀匹配重命名（内部按 from 长度降序，保证最长前缀优先）。
fn apply_rename(rel: &str, renames: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = renames.iter().collect();
    sorted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    for (from, to) in sorted {
        if rel == from {
            return to.clone();
        }
        if let Some(rest) = rel.strip_prefix(&format!("{from}/")) {
            return format!("{to}/{rest}");
        }
    }
    rel.to_string()
}

/// 简单二进制检测：前 8KB 含 NUL 字节视为二进制（跳过替换）。
fn is_binary(data: &[u8]) -> bool {
    let n = data.len().min(8192);
    data[..n].contains(&0)
}

/// 为脚本类文件补可执行位（Unix）。
#[cfg(unix)]
fn make_executable(path: &Path, rel: &str) {
    use std::os::unix::fs::PermissionsExt;
    let name = Path::new(rel)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if rel.ends_with(".sh") || name == "gradlew" {
        if let Ok(meta) = fs::metadata(path) {
            let mut perms = meta.permissions();
            let mode = perms.mode();
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode | 0o755));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{RenameRule, ReplaceRule, TemplateManifest};

    fn fixture(root: &Path) {
        fs::create_dir_all(root.join("src/com/example/app")).unwrap();
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(
            root.join("src/com/example/app/Main.kt"),
            "package com.example.app\n",
        )
        .unwrap();
        fs::write(root.join("assets/data.bin"), &[0u8, 1, 2, 0]).unwrap();
        fs::write(root.join("README.md"), "# template\n").unwrap();
        fs::write(root.join(".git/config"), "x").unwrap();
        fs::write(root.join("template.yaml"), "id: x\n").unwrap();
    }

    #[test]
    fn render_replaces_renames_deletes_and_skips() {
        let tmp = tempfile::tempdir().unwrap();
        let tpl = tmp.path().join("tpl");
        let out = tmp.path().join("out");
        fixture(&tpl);

        let manifest = TemplateManifest {
            id: "t".into(),
            replace: vec![ReplaceRule {
                find: "com.example.app".into(),
                with: "{{package}}".into(),
            }],
            rename: vec![RenameRule {
                from: "src/com/example/app".into(),
                to: "src/{{package|path}}".into(),
            }],
            delete: vec!["README.md".into()],
            ..Default::default()
        };
        let mut values = HashMap::new();
        values.insert("package".into(), "com.foo.bar".into());

        let created = render(&RenderOptions {
            template_root: &tpl,
            output_dir: &out,
            manifest: &manifest,
            values: &values,
        })
        .unwrap();

        assert_eq!(
            fs::read_to_string(out.join("src/com/foo/bar/Main.kt")).unwrap(),
            "package com.foo.bar\n"
        );
        assert_eq!(
            fs::read(out.join("assets/data.bin")).unwrap(),
            [0u8, 1, 2, 0]
        );
        assert!(
            !out.join("README.md").exists(),
            "delete 规则应移除 README.md"
        );
        assert!(!out.join(".git").exists(), "应跳过 .git");
        assert!(!out.join("template.yaml").exists(), "应跳过模板清单");
        assert!(
            !out.join("src/com/example").exists(),
            "重命名后遗留的空目录应被清理"
        );
        assert!(created.iter().any(|p| p.ends_with("Main.kt")));
    }

    #[test]
    fn delete_directory_cascades() {
        let tmp = tempfile::tempdir().unwrap();
        let tpl = tmp.path().join("tpl");
        let out = tmp.path().join("out");
        fs::create_dir_all(tpl.join("docs/nested")).unwrap();
        fs::write(tpl.join("keep.txt"), "k").unwrap();
        fs::write(tpl.join("docs/guide.md"), "g").unwrap();
        fs::write(tpl.join("docs/nested/deep.txt"), "d").unwrap();

        let manifest = TemplateManifest {
            id: "t".into(),
            delete: vec!["docs/".into()],
            ..Default::default()
        };
        let values = HashMap::new();
        render(&RenderOptions {
            template_root: &tpl,
            output_dir: &out,
            manifest: &manifest,
            values: &values,
        })
        .unwrap();

        assert!(out.join("keep.txt").exists());
        assert!(!out.join("docs").exists(), "delete 目录规则应级联删除整个子树");
    }

    #[test]
    fn longest_prefix_rename_wins() {
        let renames = vec![
            ("a/b".to_string(), "x".to_string()),
            ("a/b/c".to_string(), "y".to_string()),
        ];
        assert_eq!(apply_rename("a/b/c/d.txt", &renames), "y/d.txt");
        assert_eq!(apply_rename("a/b/z.txt", &renames), "x/z.txt");
    }

    #[test]
    fn binary_detection() {
        assert!(is_binary(&[1, 2, 0, 3]));
        assert!(!is_binary(b"hello world"));
    }
}
