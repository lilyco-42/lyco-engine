//! 模板来源获取：GitHub 仓库（`gh api` tarball）与本地目录。

use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 检查 gh CLI 是否可用（仅对仓库型模板调用）。
pub fn ensure_gh() -> Result<()> {
    let out = Command::new("gh").arg("--version").output();
    if out.is_err() {
        bail!("未找到 gh CLI，请先安装并登录：gh auth login");
    }
    Ok(())
}

/// 通过 `gh api repos/{owner}/{repo}/tarball` 下载并解压到 `dest`（不含顶层 `<owner>-<repo>-<sha>/`）。
/// gh api / codeload 偶发网络抖动，最多重试 2 次。
pub fn fetch_repo(repo: &str, dest: &Path) -> Result<()> {
    ensure_gh()?;
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=2 {
        match try_download(repo, dest) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                if attempt < 2 {
                    eprintln!("  下载失败，1 秒后重试…");
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("下载模板失败（repo={repo}）")))
}

fn try_download(repo: &str, dest: &Path) -> Result<()> {
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    fs::create_dir_all(dest)?;

    let output = Command::new("gh")
        .args(["api", &format!("repos/{repo}/tarball")])
        .output()
        .with_context(|| format!("调用 gh api 下载模板失败（repo={repo}）"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("gh api 返回错误: {}", err.trim()));
    }
    if output.stdout.is_empty() {
        return Err(anyhow!("gh api 返回空数据（repo={repo}，仓库可能为空）"));
    }

    let gz = flate2::read::GzDecoder::new(&output.stdout[..]);
    let mut archive = tar::Archive::new(gz);
    let mut entries = archive.entries()?;
    while let Some(entry) = entries.next().transpose()? {
        let mut entry = entry;
        let path = entry.path()?.into_owned();
        // 剥掉顶层 `<owner>-<repo>-<sha>/`
        let rel = strip_top(&path);
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(&rel);
        let etype = entry.header().entry_type();
        if etype.is_dir() {
            fs::create_dir_all(&out)?;
        } else if etype.is_file() || etype.is_symlink() {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            // 符号链接在 Windows 上可能无法解包，失败时跳过而非中断
            if let Err(e) = entry.unpack(&out) {
                if etype.is_symlink() {
                    eprintln!("  ⚠ 跳过符号链接: {}", rel.display());
                    continue;
                }
                return Err(e).with_context(|| format!("解包失败: {}", rel.display()));
            }
        }
    }
    Ok(())
}

fn strip_top(path: &Path) -> PathBuf {
    path.components()
        .skip(1)
        .fold(PathBuf::new(), |mut acc, c| {
            acc.push(c.as_os_str());
            acc
        })
}
