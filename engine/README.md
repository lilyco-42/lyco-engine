# yuzu — WASM 跨平台视觉小说引擎

柚子社(YuzuSoft)/KiriKiri 系 Galgame 的 **WASM 跨平台运行时**:Rust 引擎 + 服务端资源懒加载 + Web/Node 播放器。

- **客户端**:WASM 引擎(脚本解析 / 图层 / 音频 / 剧情状态)+ Web 播放器 / Node 宿主
- **服务端**:XP3 归档懒加载 / 反编译 / 图像解码 / 版本管理与热更新
- **流式加载**:剧本/图像/音频两级缓存(内存 LRU + IndexedDB 持久),跨会话秒开
- **对齐原版**:场景图选择点 / 章节流程 / 立绘多层合成 / 表情随台词切换 / 存档读档 / 回看 / 设置

> 引擎支持任意 KiriKiri 系游戏(PSB 剧本 + XP3 归档 + TLG 纹理);本仓库以《千恋万花》验证。
> 交接与实现细节见 [`ENGINE_STATUS.md`](./ENGINE_STATUS.md)。

---

## 目录

- [0. 一键安装与傻瓜玩法](#0-一键安装与傻瓜玩法)
- [1. 我有 APK,怎么玩](#1-我有-apk怎么玩)
- [2. 快速开始(已有解包数据)](#2-快速开始已有解包数据)
- [3. 架构](#3-架构)
- [4. 命令行工具](#4-命令行工具)
- [5. 从源码构建](#5-从源码构建)
- [6. 常见问题](#6-常见问题)

---

## 0. 一键安装与傻瓜玩法

```bash
# 1) 安装:下载 GitHub Release 预编译二进制,或源码安装
#    (GitHub Actions 自动构建,仓库公开,无需登录)
cargo install --path crates/yuzu-cli      # 或从 Releases 下载 yuzu-cli 预编译版

# 2) 傻瓜玩法:一个 APK,一条命令,自动开浏览器
yuzu-cli 千恋万花.apk
# → 从 APK 提取 XP3 归档到 yuzu-data/ → 启动内嵌 web 播放器 → 自动打开浏览器

# 已解包的数据目录同理
yuzu-cli 你的数据目录/      # 或 yuzu-cli web --data 你的数据目录/
```

> **发布流程(仓库维护者)**:打纯版本号 tag(如 `0.1.2`)触发 GitHub Actions 自动构建并发布
> [GitHub Release](https://github.com/lilyco-42/lyco-engine/releases),资产含各平台的 `yuzu-cli` /
> `yuzu-server`(Windows 为 `.exe`)及 `lyco-<target>`。二进制内嵌播放器资源,单文件可运行,无需 `web/` 目录。
> `cargo binstall lyco` 亦可直接安装(见仓库根 README)。

---

## 1. 我有 APK,怎么玩

KiriKiri 系游戏的 Android 移植(如 Kirikiroid2)把游戏数据打包在 APK 里。核心数据是 **XP3 归档**(`data.xp3` / `fgimage.xp3` / `bgimage.xp3` / `voice.xp3` / `bgm.xp3` …),引擎直接吃这些。

### 步骤 ① 从 APK 提取 XP3 归档

**方法 A:解包 APK(推荐先试)**
```bash
# APK 本质是 zip;解压后找 .xp3
unzip game.apk -d apk_out
find apk_out -iname "*.xp3"
```
找不到 `.xp3`?可能是被改名/混淆(常见改为 `.bin` / `.dat` / 无扩展名)。用魔数识别:
```bash
# KiriKiri XP3 头: XP3\r\n \n\x1a\x8b\x67\x01
find apk_out -type f -exec sh -c 'head -c 4 "$1" | grep -q "XP3" && echo "$1"' _ {} \;
```

**方法 B:从应用数据目录拉取**

很多移植在首次运行时才释放数据到存储:
```bash
# 先找到包名
aapt dump badging game.apk | grep package      # 或用 apktool
# 外置数据(无需 root,最常见)
adb shell ls /sdcard/Android/data/<包名>/       # 数据通常在此
adb pull /sdcard/Android/data/<包名>/ data_out
# Kirikiroid2 常见目录
adb shell ls /sdcard/KIRIKIRI/ 2>/dev/null
# 内置数据(需要 root 或 adb backup)
adb shell su -c "ls /data/data/<包名>/" 2>/dev/null
```
> Windows 下 adb shell 的路径会被 Git Bash 改写,用 `MSYS_NO_PATHCONV=1 adb shell …` 或双斜杠。

**方法 C:OBB 文件**
游戏数据也可能在 `/sdcard/Android/obb/<包名>/*.obb`(本身是 zip,或直接是归档)。

### 步骤 ② 放置数据目录

把提取到的 `.xp3` 放进一个目录(例如 `realgame/`):
```bash
mkdir realgame && mv data.xp3 fgimage.xp3 bgimage.xp3 voice.xp3 bgm.xp3 realgame/
```

### 步骤 ③ 启动服务端 → 浏览器游玩
```bash
cd engine
cargo run -p yuzu-server -- --data realgame --web web
# 浏览器打开 http://127.0.0.1:8080/
```
开始游戏 → 标题屏 → **开始游戏** → 章节流程 + 选择分支 + 立绘表情切换。

### 步骤 ④(仅加密归档需要)指定解密方案
若 `./yuzu-cli xp3 list realgame/data.xp3` 报错或内容乱码,归档可能加密,用 `--scheme`:
```bash
yuzu-cli xp3 list realgame/data.xp3
yuzu-cli xp3 read realgame/data.xp3 "scn\\xxx.ks.scn" --scheme xor
```
支持方案:`xor` / `xor-p1-neg` / `xor-mix` / `dieselmine` / `moteyaba` / `kamiyaba` / `rebirth` / `fsn`(另有 Riddle / YuzDecryptor / NanaDecryptor 自动探测)。

---

## 2. 快速开始(已有解包数据)

```bash
cd engine
# 后端:懒加载资源 + 静态 web 播放器
yuzu-server --data realgame --web web --addr 127.0.0.1:8080
# 浏览器 → http://127.0.0.1:8080/
```
没有后端时,播放器也可在浏览器内直接解包本地 `.xp3`(点顶栏「打开 .xp3 文件」)。

Node 宿主(非浏览器,同一 WASM 引擎跑剧本):
```bash
cd web && node run.mjs --scene '*com_part_1' --steps 30
```

---

## 3. 架构

```
yuzu-scn(.scn PSB 解析) → yuzu-kag(KAG 引擎:图层/音频/流程状态机)
                              ↓ 效果流(dialogue/layer/audio/chapter/wait)
yuzu-wasm(kag_run_scene) ← 播放器(web canvas / Node 宿主)
yuzu-xp3(归档/压缩/加密/场景文本解码) | yuzu-psb/pimg/tlg(资源)
yuzu-server(懒加载 / 反编译 / 解码 / /api/version 热更新)
```

| crate | 职责 |
|---|---|
| `yuzu-scn` | 解析 `.scn`(PSB 格式)剧本 |
| `yuzu-kag` | KiriKiri KAG 引擎:图层/音频/流程/存档 |
| `yuzu-xp3` | XP3 归档读写、压缩、加密方案、场景文本解码 |
| `yuzu-psb` / `yuzu-pimg` / `yuzu-tlg` | PSB / PIMG 合成 / TLG 纹理 |
| `yuzu-wasm` | wasm-bindgen 导出(compile_scn / kag_run_scene / 解码) |
| `yuzu-server` | axum 后端:懒加载归档、反编译、图像解码、版本清单 |
| `yuzu-cli` | 命令行:psb / scn / pimg / tlg / xp3 / scenario / play |

---

## 4. 命令行工具

```bash
cargo run -p yuzu-cli -- --help
# PSB 剧本检视/提取
yuzu-cli psb dump    file.psb
yuzu-cli psb extract file.psb -o out/
# SCN 剧本
yuzu-cli scn info    file.scn
yuzu-cli scn dump    file.scn --lang cn
# 立绘 PIMG 合成 → PNG
yuzu-cli pimg render file.pimg -o out.png
# TLG 纹理解码 → PNG
yuzu-cli tlg  input.tlg -o out.png
# XP3 归档
yuzu-cli xp3 list    file.xp3
yuzu-cli xp3 extract file.xp3 -o out/ --scheme xor
yuzu-cli xp3 read    file.xp3 "scn\\x.ks.scn" -o x.scn
# KiriKiri 场景文本解码(.ks/.csv/.stand)
yuzu-cli scenario    file.ks
# 原生引擎跑场景(终端)
yuzu-cli play        file.scn '*scene_label'
```

---

## 5. 从源码构建

前置:Rust(wasm32-unknown-unknown target)+ wasm-bindgen-cli 0.2.127
```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.127
cd engine && ./web/build.sh      # 重建 wasm + 后端 + 冒烟测试
cargo test --workspace
```

---

## 6. 常见问题

**Q: 开始游戏没反应?**
硬刷新(Ctrl+Shift+R)。旧版 HTML/CSS/JS 缓存错配已通过 `Cache-Control: no-cache` 根治。

**Q: 有背景没立绘,或立绘没有脸?**
需要 `fgimage*.xp3` 与 `bgimage*.xp3`。立绘是「身体 + 表情脸」多层合成(与 APK 一致),缺失任一归档会回退到简单模式。

**Q: 剧本(SCN)反编译报错?**
确认 `.scn` 是 PSB 格式(部分 `.scn` 是文本);`yuzu-cli scn dump` 可定位。

**Q: 语音/音乐不响?**
需要 `voice.xp3`(M4A)与 `bgm.xp3`(opus);归档引用大小写不敏感,服务端已处理 `BGM01I` → `BGM01i.opus`。

**Q: 数据目录结构?**
服务端扫描 `--data` 目录下所有 `*.xp3`,无需预先解包;播放器按需懒加载。
