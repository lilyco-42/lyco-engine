//! yuzu-cli — 柚子社(Yuzusoft)资源工具命令行。
//!
//! 支持 psb / scn / pimg / tlg 检视与提取,是 Project2 Web 引擎的前置工具链。

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};
use yuzu_pimg::{parse as parse_pimg, render_png};
use yuzu_psb::PsbFile;
use yuzu_scn::{Content, Line, Scn};
use yuzu_tlg::{decode as decode_tlg, image::ImageFormat, is_tlg};
use yuzu_xp3::{Simple, SimpleKind, Xp3Archive};

#[derive(Parser)]
#[command(
    name = "yuzu-cli",
    version,
    about = "柚子社(Yuzusoft)资源工具: psb/scn/pimg/tlg;傻瓜模式: yuzu <name.apk>"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
    /// 傻瓜模式:直接传 .apk 文件或游戏数据目录 → 一键提取 XP3 + 启动 Web + 打开浏览器
    #[arg(value_name = "FILE")]
    file: Option<String>,
}

#[derive(Subcommand)]
enum Cmd {
    /// PSB 检视与提取
    Psb {
        #[command(subcommand)]
        action: PsbAction,
    },
    /// SCN 剧本检视
    Scn {
        #[command(subcommand)]
        action: ScnAction,
    },
    /// PIMG 立绘合成
    Pimg {
        #[command(subcommand)]
        action: PimgAction,
    },
    /// 解码 KiriKiri 场景文本(.ks/.csv/.stand 等,移植 krkrz TextStream)
    Scenario {
        /// 输入文件
        file: String,
    },
    /// 原生引擎:在终端跑指定场景(非浏览器宿主)
    Play {
        /// 剧本文件(.scn),story 模式下为 XP3 归档
        file: String,
        /// 场景标签(缺省自动取首个含台词场景)
        scene: Option<String>,

    },
    /// TLG 纹理解码 → PNG
    Tlg {
        /// 输入文件
        input: String,
        /// 输出 PNG
        #[arg(short, long)]
        output: String,
    },
    /// XP3 归档检视与提取
    Xp3 {
        #[command(subcommand)]
        action: Xp3Action,
    },
    /// 傻瓜模式:从 APK 提取 XP3 归档到 yuzu-data/,启动 Web 并打开浏览器
    Apk {
        /// APK 文件路径
        file: String,
    },
    /// 启动 Web 播放器(内嵌资源),自动打开浏览器
    Web {
        /// 游戏数据目录(含 .xp3)
        #[arg(long, default_value = "yuzu-data")]
        data: String,
        /// 监听地址
        #[arg(long, default_value = "127.0.0.1:8080")]
        addr: String,
    },
}

#[derive(Subcommand)]
enum PsbAction {
    /// 打印 PSB 根 JSON
    Dump { file: String },
    /// 提取全部内嵌资源到目录
    Extract {
        file: String,
        #[arg(short, long)]
        output: String,
    },
}

#[derive(Subcommand)]
enum ScnAction {
    /// SCN 摘要
    Info { file: String },
    /// 按场景输出台词
    Dump {
        file: String,
        /// 优先语言(cn/jp/en/tw),缺省自动
        #[arg(long, default_value = "cn")]
        lang: String,
    },
}

#[derive(Subcommand)]
enum PimgAction {
    /// 画布/图层信息
    Info { file: String },
    /// 渲染合成到 PNG
    Render {
        file: String,
        #[arg(short, long)]
        output: String,
    },
}

#[derive(Subcommand)]
enum Xp3Action {
    /// 列出归档内容
    List { file: String },
    /// 提取全部文件到目录
    Extract {
        file: String,
        #[arg(short, long)]
        output: String,
        /// 解密方案(xor 等;默认不解密)
        #[arg(long, value_name = "scheme")]
        scheme: Option<String>,
    },
    /// 提取单个文件(默认到 stdout,可用 -o 写文件)
    Read {
        file: String,
        /// 归档内路径
        name: String,
        #[arg(short, long)]
        output: Option<String>,
        #[arg(long, value_name = "scheme")]
        scheme: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Some(cmd) => match cmd {
        Cmd::Psb { action } => match action {
            PsbAction::Dump { file } => cmd_psb_dump(&file),
            PsbAction::Extract { file, output } => cmd_psb_extract(&file, &output),
        },
        Cmd::Scn { action } => match action {
            ScnAction::Info { file } => cmd_scn_info(&file),
            ScnAction::Dump { file, lang } => cmd_scn_dump(&file, &lang),
        },
        Cmd::Pimg { action } => match action {
            PimgAction::Info { file } => cmd_pimg_info(&file),
            PimgAction::Render { file, output } => cmd_pimg_render(&file, &output),
        },
        Cmd::Tlg { input, output } => cmd_tlg_decode(&input, &output),
        Cmd::Scenario { file } => cmd_scenario_decode(&file),
Cmd::Play { file, scene, .. } => cmd_play(&file, scene.as_deref()),
        Cmd::Xp3 { action } => match action {
            Xp3Action::List { file } => cmd_xp3_list(&file),
            Xp3Action::Extract {
                file,
                output,
                scheme,
            } => cmd_xp3_extract(&file, &output, scheme.as_deref()),
            Xp3Action::Read {
                file,
                name,
                output,
                scheme,
            } => cmd_xp3_read(&file, &name, output.as_deref(), scheme.as_deref()),
        },
        Cmd::Apk { file } => cmd_apk(&file),
        Cmd::Web { data, addr } => cmd_web(&data, &addr, true),
        },
        // 傻瓜模式:yuzu <name.apk> 或 yuzu <数据目录>
        None => match cli.file.as_deref() {
            Some(f) if f.to_lowercase().ends_with(".apk") => cmd_apk(f),
            Some(f) if Path::new(f).is_dir() => cmd_web(f, "127.0.0.1:8080", true),
            Some(f) => bail!("无法识别 {f}:需要 .apk 文件或游戏数据目录"),
            None => bail!("缺少参数: yuzu <name.apk> | yuzu web --data <dir> | yuzu <子命令>"),
        },
    }
}

// ---------- psb ----------

fn cmd_psb_dump(file: &str) -> Result<()> {
    let psb = load_psb(file)?;
    println!(
        "PSB v{} enc={} resources={} extra={}",
        psb.version(),
        psb.encrypted(),
        psb.resources_len(),
        psb.extra_resources_len()
    );
    println!("{}", serde_json::to_string_pretty(psb.root())?);
    Ok(())
}

fn cmd_psb_extract(file: &str, out: &str) -> Result<()> {
    let psb = load_psb(file)?;
    fs::create_dir_all(out).with_context(|| format!("创建目录失败: {out}"))?;
    let mut written = 0usize;
    for i in 0..psb.resources_len() {
        if let Some(b) = psb.resource(i as u32) {
            let name = format!("res_{i}{}", ext_for(b));
            fs::write(format!("{out}/{name}"), b)?;
            println!("  {name} ({} bytes)", b.len());
            written += 1;
        }
    }
    for i in 0..psb.extra_resources_len() {
        if let Some(b) = psb.extra_resource(i as u32) {
            let name = format!("extra_{i}{}", ext_for(b));
            fs::write(format!("{out}/{name}"), b)?;
            println!("  {name} ({} bytes)", b.len());
            written += 1;
        }
    }
    println!("✓ 提取 {} 个资源到 {out}", written);
    Ok(())
}

// ---------- scn ----------

fn cmd_scn_info(file: &str) -> Result<()> {
    let scn = load_scn(file)?;
    println!(
        "SCN: {} (hash={:?}, scenes={})",
        scn.name,
        scn.hash,
        scn.scenes.len()
    );
    for (i, s) in scn.scenes.iter().enumerate() {
        let title = s.title.first().map(|t| t.as_str()).unwrap_or("-");
        println!(
            "  scene[{i}] label={} title={title} lines={} texts={} nexts={}",
            s.label,
            s.lines.len(),
            s.texts.len(),
            s.nexts.len()
        );
    }
    Ok(())
}

fn cmd_scn_dump(file: &str, lang: &str) -> Result<()> {
    let scn = load_scn(file)?;
    for s in &scn.scenes {
        let title = s.title.first().map(|t| t.as_str()).unwrap_or(&s.label);
        println!("\n=== {title} (label={}) ===", s.label);
        for line in &s.lines {
            println!("  {}", fmt_line(line, s, lang));
        }
    }
    Ok(())
}

fn fmt_line(line: &Line, scene: &yuzu_scn::Scene, lang: &str) -> String {
    match line {
        Line::Text(idx) => match scene.texts.get(*idx) {
            Some(t) => {
                let who = t.character.as_deref().unwrap_or("?");
                let text = t
                    .dialogues
                    .iter()
                    .map(|d| match &d.content {
                        Content::Plain(s) => s.clone(),
                        Content::Lang(lm) => lm.first(&[lang]).unwrap_or_default(),
                    })
                    .collect::<Vec<_>>()
                    .join(" / ");
                format!("{who}: {text}")
            }
            None => format!("<text {idx}>"),
        },
        Line::Resource(s) => format!("<{s}>"),
        Line::Event(e) => format!(
            "<{} {}>",
            e.kind,
            e.fields
                .iter()
                .map(serde_json::Value::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Line::SnapshotPoint(sp) => format!(
            "<snapshot #{}/{} flag={:?}>",
            sp.index, sp.original_line, sp.flag
        ),
        Line::Raw(v) => format!("<raw {v}>"),
    }
}

// ---------- pimg ----------

fn cmd_pimg_info(file: &str) -> Result<()> {
    let data = fs::read(file).with_context(|| format!("读取失败: {file}"))?;
    let pimg = parse_pimg(&data).map_err(|e| anyhow::anyhow!("解析失败: {e}"))?;
    println!(
        "{}x{} layers={} textures={}",
        pimg.width,
        pimg.height,
        pimg.layers.len(),
        pimg.textures.len()
    );
    for l in &pimg.layers {
        println!(
            "  layer id={} pos=({},{}) size={}x{} visible={} opacity={:.0}%",
            l.id,
            l.x,
            l.y,
            l.width,
            l.height,
            l.visible,
            l.opacity * 100.0
        );
    }
    Ok(())
}

fn cmd_pimg_render(file: &str, out: &str) -> Result<()> {
    let data = fs::read(file).with_context(|| format!("读取失败: {file}"))?;
    let png = render_png(&data).map_err(|e| anyhow::anyhow!("渲染失败: {e}"))?;
    fs::write(out, &png).with_context(|| format!("写入失败: {out}"))?;
    println!("✓ 已渲染 {file} → {out} ({} bytes)", png.len());
    Ok(())
}

// ---------- tlg ----------

fn cmd_tlg_decode(input: &str, out: &str) -> Result<()> {
    let data = fs::read(input).with_context(|| format!("读取失败: {input}"))?;
    if !is_tlg(&data) {
        bail!("不是 TLG 文件(魔数不符): {input}");
    }
    let img = decode_tlg(&data).map_err(|e| anyhow::anyhow!("TLG 解码失败: {e}"))?;
    img.write_to(&mut fs::File::create(out)?, ImageFormat::Png)
        .with_context(|| format!("写入失败: {out}"))?;
    println!(
        "✓ 已解码 {input} → {out} ({}x{})",
        img.width(),
        img.height()
    );
    Ok(())
}

// ---------- xp3 ----------

fn cmd_xp3_list(file: &str) -> Result<()> {
    let arc = load_xp3(file)?;
    println!("XP3 v{} 文件 {} 个", arc.version(), arc.len());
    for name in arc.file_names() {
        let f = arc.file(name).expect("名称来自归档");
        println!(
            "  {name}\t{} bytes (packed {})\thash={:08x}\tsegments={}",
            f.size,
            f.packed_size,
            f.hash,
            f.segments.len()
        );
    }
    Ok(())
}

fn cmd_xp3_extract(file: &str, out: &str, scheme: Option<&str>) -> Result<()> {
    let arc = load_xp3(file)?;
    let scheme = resolve_scheme(scheme)?;
    fs::create_dir_all(out).with_context(|| format!("创建目录失败: {out}"))?;
    let mut written = 0usize;
    let mut total = 0u64;
    for name in arc.file_names() {
        let data = read_with(&arc, name, scheme.as_ref())?;
        let path = safe_join(Path::new(out), name).context("归档路径非法")?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("创建目录失败: {}", parent.display()))?;
        }
        fs::write(&path, &data).with_context(|| format!("写入失败: {}", path.display()))?;
        written += 1;
        total += data.len() as u64;
    }
    println!("✓ 提取 {written} 个文件到 {out} (共 {total} 字节)");
    Ok(())
}

fn cmd_xp3_read(file: &str, name: &str, out: Option<&str>, scheme: Option<&str>) -> Result<()> {
    let arc = load_xp3(file)?;
    let scheme = resolve_scheme(scheme)?;
    let data = read_with(&arc, name, scheme.as_ref())?;
    match out {
        Some(o) => {
            fs::write(o, &data).with_context(|| format!("写入失败: {o}"))?;
            println!("✓ 已提取 {name} → {o} ({} bytes)", data.len());
        }
        None => {
            use std::io::Write;
            std::io::stdout().write_all(&data)?;
        }
    }
    Ok(())
}

fn load_xp3(file: &str) -> Result<Xp3Archive> {
    Xp3Archive::open_file(file).map_err(|e| anyhow::anyhow!("XP3 解析失败: {e}"))
}

fn read_with(arc: &Xp3Archive, name: &str, scheme: Option<&Simple>) -> Result<Vec<u8>> {
    match scheme {
        Some(s) => arc
            .read_decrypted(name, s)
            .map_err(anyhow::Error::msg)?
            .ok_or_else(|| anyhow::anyhow!("归档内不存在 {name}")),
        None => arc
            .read(name)
            .map_err(anyhow::Error::msg)?
            .ok_or_else(|| anyhow::anyhow!("归档内不存在 {name}")),
    }
}

fn resolve_scheme(scheme: Option<&str>) -> Result<Option<Simple>> {
    const SUPPORTED: &str =
        "xor, xor-p1-neg, xor-mix, dieselmine, moteyaba, kamiyaba, rebirth, fsn";
    match scheme {
        None => Ok(None),
        Some(name) => {
            let kind = match name {
                "xor" => SimpleKind::Xor,
                "xor-p1-neg" => SimpleKind::XorP1Neg,
                "xor-mix" => SimpleKind::XorMix,
                "dieselmine" => SimpleKind::Dieselmine,
                "moteyaba" => SimpleKind::Moteyaba,
                "kamiyaba" => SimpleKind::Kamiyaba,
                "rebirth" => SimpleKind::Rebirth,
                "fsn" => SimpleKind::Fsn,
                _ => bail!("未知加密方案 {name}(支持: {SUPPORTED})"),
            };
            Ok(Some(Simple(kind)))
        }
    }
}

/// 把归档内路径安全地映射到输出目录(丢弃空段/`.`/`..`/`:` 段,防穿越)。
fn safe_join(root: &Path, name: &str) -> Option<PathBuf> {
    let mut p = root.to_path_buf();
    for comp in name.split(['/', '\\']) {
        if comp.is_empty() || comp == "." || comp == ".." || comp.contains(':') {
            continue;
        }
        p.push(comp);
    }
    Some(p)
}

// ---------- scenario ----------

fn cmd_scenario_decode(file: &str) -> Result<()> {
    let data = fs::read(file).with_context(|| format!("读取失败: {file}"))?;
    match yuzu_xp3::scenario::decode(&data) {
        Some(text) => {
            use std::io::Write;
            std::io::stdout().write_all(text.as_bytes())?;
            Ok(())
        }
        None => bail!("非 KiriKiri 场景编码文件(无 fe fe 头)"),
    }
}

// ---------- helpers ----------

fn load_psb(file: &str) -> Result<PsbFile> {
    let data = fs::read(file).with_context(|| format!("读取失败: {file}"))?;
    PsbFile::parse(&data).map_err(|e| anyhow::anyhow!("PSB 解析失败: {e}"))
}

fn load_scn(file: &str) -> Result<Scn> {
    let data = fs::read(file).with_context(|| format!("读取失败: {file}"))?;
    yuzu_scn::parse(&data).map_err(anyhow::Error::msg)
}

/// 按魔数推断资源扩展名。
fn ext_for(data: &[u8]) -> &'static str {
    if is_tlg(data) {
        ".tlg"
    } else if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        ".png"
    } else if data.starts_with(b"GIF8") {
        ".gif"
    } else if data.starts_with(b"BM") {
        ".bmp"
    } else if data.starts_with(b"OggS") {
        ".ogg"
    } else if data.starts_with(b"RIFF") {
        ".wav"
    } else {
        ".bin"
    }
}

/// 原生引擎:解析剧本,跑指定场景,把引擎效果流打印到终端(非浏览器宿主演示)。
fn cmd_play(file: &str, scene: Option<&str>) -> Result<()> {
    let data = fs::read(file).with_context(|| format!("读取失败: {file}"))?;
    let scn = yuzu_scn::parse(&data).map_err(anyhow::Error::msg)?;
    let label = match scene {
        Some(l) => l.to_string(),
        None => scn
            .scenes
            .iter()
            .find(|s| s.lines.iter().any(|l| matches!(l, yuzu_scn::Line::Text(_))))
            .map(|s| s.label.clone())
            .ok_or_else(|| anyhow::anyhow!("剧本无台词场景"))?,
    };
    let scene_obj = scn
        .scene(&label)
        .ok_or_else(|| anyhow::anyhow!("场景不存在: {label}"))?;
    let mut eng = yuzu_kag::KagEngine::new();
    let effects = eng.run_scene(scene_obj);
    println!("[引擎] {label} → {} 效果", effects.len());
    let (mut dlg, mut lay, mut aud, mut chp) = (0usize, 0usize, 0usize, 0usize);
    for e in &effects {
        match e {
            yuzu_kag::Effect::ShowDialogue {
                character, text, ..
            } => {
                dlg += 1;
                println!("  {character}: {text}");
            }
            yuzu_kag::Effect::LayerChange { layer, name, .. } => {
                lay += 1;
                println!("  [图层] {layer:?} {name}");
            }
            yuzu_kag::Effect::Audio { kind, name } => {
                aud += 1;
                println!("  [音频] {kind:?} {name}");
            }
            yuzu_kag::Effect::Chapter { title } => {
                chp += 1;
                println!("  [章节] {title}");
            }
            yuzu_kag::Effect::WaitForClick => {}
        }
    }
    println!("[统计] 台词 {dlg} | 图层 {lay} | 音频 {aud} | 章节 {chp}");
    Ok(())
}

// ---------- 傻瓜模式:APK → 提取 → Web ----------

/// 内嵌的 web 播放器资源(engine/web/),让独立安装的二进制无需外部文件即可运行。
/// `CARGO_MANIFEST_DIR` 经 rust-embed 的 interpolate-folder-path feature 展开,
/// 从任意工作目录都能定位源码内的 web/(发布 verify 亦同)。
#[derive(rust_embed::RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../web"]
struct WebAssets;

/// 把内嵌播放器落盘到 `out`(首次运行;已有则跳过)。
fn materialize_web(out: &Path) -> Result<()> {
    if out.join("index.html").is_file() {
        return Ok(());
    }
    fs::create_dir_all(out)?;
    let mut n = 0usize;
    for f in WebAssets::iter() {
        let name = f.as_ref();
        if name.contains("pkg-node") {
            continue; // Node 宿主绑定不需要,减小体积
        }
        if let Some(data) = WebAssets::get(name) {
            let path = out.join(name);
            if let Some(p) = path.parent() {
                fs::create_dir_all(p)?;
            }
            fs::write(&path, data.data.as_ref())?;
            n += 1;
        }
    }
    println!("✓ 播放器资源已落盘: {} ({n} 文件)", out.display());
    Ok(())
}

/// 启动内嵌 Web 播放器:API(懒加载 XP3)+ 播放器静态页,自动开浏览器。
fn cmd_web(data: &str, addr: &str, open: bool) -> Result<()> {
    let web_dir = if Path::new("web").is_dir() {
        "web".to_string() // 源码目录运行:直接用仓库 web/
    } else {
        let out = PathBuf::from(".yuzu-web");
        materialize_web(&out)?;
        out.to_string_lossy().into_owned()
    };
    let url = format!("http://{addr}/");
    println!("▶ 播放器: {url}  (data={data})");
    if open {
        open_browser(&url);
    }
    yuzu_server::serve(data, &web_dir, addr)
}

/// 跨平台打开默认浏览器。
fn open_browser(url: &str) {
    let spawned = if cfg!(windows) {
        std::process::Command::new("cmd").args(["/c", "start", "", url]).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if let Err(e) = spawned {
        println!("[提示] 自动打开浏览器失败: {e} (手动访问 {url})");
    }
}

/// 傻瓜入口:从 APK(zip)提取全部 XP3 归档(扩展名或魔数)到 yuzu-data/,然后启动 Web。
fn cmd_apk(path: &str) -> Result<()> {
    use std::io::Read;
    let f = std::fs::File::open(path).with_context(|| format!("打开 APK 失败: {path}"))?;
    let mut zip = zip::ZipArchive::new(f).with_context(|| format!("解包失败(非 zip): {path}"))?;
    let base = Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "game".into());
    let out = PathBuf::from("yuzu-data").join(&base);
    fs::create_dir_all(&out)?;

    // 第一遍:识别 XP3 条目(扩展名 .xp3,或魔数 "XP3\r")
    let mut hits: Vec<(usize, String, u64)> = Vec::new();
    for i in 0..zip.len() {
        let (name, size, is_dir) = {
            let e = zip.by_index(i)?;
            (e.name().to_string(), e.size(), e.is_dir())
        };
        if is_dir {
            continue;
        }
        if name.to_lowercase().ends_with(".xp3") {
            hits.push((i, name, size));
            continue;
        }
        // 魔数嗅探:改名/无扩展名的归档(跳过超大条目)
        if size <= 64 * 1024 * 1024 {
            let mut head = [0u8; 4];
            let mut fe = zip.by_index(i)?;
            if fe.read_exact(&mut head).is_ok() && &head == b"XP3\r" {
                hits.push((i, name, size));
            }
        }
    }
    if hits.is_empty() {
        bail!("APK 中未找到 XP3 归档(data.xp3 等)。可能数据在外部存储/OBB,见 README「从 APK 提取」");
    }
    for (i, name, size) in &hits {
        let mut e = zip.by_index(*i)?;
        let safe = name.rsplit(['/', '\\']).next().unwrap_or(name).to_string();
        let mut of = fs::File::create(out.join(&safe))?;
        std::io::copy(&mut e, &mut of)?;
        println!("  {safe} ({} bytes)", size);
    }
    println!("✓ 提取 {} 个 XP3 到 {}", hits.len(), out.display());
    cmd_web(out.to_string_lossy().as_ref(), "127.0.0.1:8080", true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use yuzu_xp3::Scheme;

    /// 复用 yuzu-xp3 的 arc_unpacker 夹具做端到端验证。
    fn fixture(name: &str) -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../yuzu-xp3/tests/fixtures")
            .join(name)
            .to_str()
            .expect("夹具路径")
            .to_string()
    }

    #[test]
    fn xp3_list_and_extract() {
        let f = fixture("xp3-compressed-files.xp3");
        cmd_xp3_list(&f).expect("list");

        let out = std::env::temp_dir().join(format!("yuzu-cli-xp3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        cmd_xp3_extract(&f, out.to_str().expect("临时目录"), None).expect("extract");
        assert_eq!(
            std::fs::read_to_string(out.join("123.txt")).expect("123.txt"),
            "1234567890"
        );
        assert_eq!(
            std::fs::read_to_string(out.join("abc.xyz")).expect("abc.xyz"),
            "abcdefghijklmnopqrstuvwxyz"
        );
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn xp3_read_single_with_and_without_scheme() {
        let arc = load_xp3(&fixture("xp3-v1.xp3")).expect("打开归档");
        assert_eq!(
            read_with(&arc, "123.txt", None).expect("读取"),
            b"1234567890"
        );
        assert!(read_with(&arc, "nope.txt", None).is_err());

        // scheme 应用后应改变内容(证明加密路径接通)
        let plain = read_with(&arc, "123.txt", None).expect("明文");
        let garbled = read_with(&arc, "123.txt", Some(&Simple(SimpleKind::Xor))).expect("解密");
        assert_ne!(plain, garbled);
        // xor 自逆:对密文原位再解应还原(使用条目真实 adlr 哈希)
        let hash = arc.file("123.txt").expect("条目").hash;
        let mut roundtrip = garbled.clone();
        Simple(SimpleKind::Xor).decrypt(&mut roundtrip, hash);
        assert_eq!(roundtrip, plain);
    }

    #[test]
    fn unknown_scheme_rejected() {
        assert!(resolve_scheme(Some("bogus")).is_err());
        assert!(resolve_scheme(Some("xor")).is_ok());
        assert!(resolve_scheme(None).is_ok());
    }
}
