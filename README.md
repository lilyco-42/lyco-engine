# lyco — 统一多语言脚手架工具

[![crates.io](https://img.shields.io/crates/v/lyco.svg)](https://crates.io/crates/lyco)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

> `gh api` 驱动模板 + 声明式渲染。一条命令从 GitHub 模板仓库（或本地目录）生成可用的多语言项目骨架。

覆盖 Android (Rust + Kotlin/Compose)、Minecraft Fabric Mod 等模板；支持自定义参数、占位符替换、
目录重命名、post 钩子；模板完全由声明式 `template.yaml` 描述，无需编写脚本。**支持 Windows / Linux / macOS / Termux（Android）**。

需求背景见 [`REQUIREMENTS.md`](./REQUIREMENTS.md)。

---

## 目录

- [特性](#特性)
- [安装](#安装)
- [快速开始](#快速开始)
- [命令参考](#命令参考)
- [模板系统](#模板系统)
- [`template.yaml` 详解](#templateyaml-详解)
- [内置变量与过滤器](#内置变量与过滤器)
- [内置模板](#内置模板)
- [自定义模板](#自定义模板)
- [配置文件参考](#配置文件参考)
- [环境变量](#环境变量)
- [约定与注意事项](#约定与注意事项)
- [开发与测试](#开发与测试)
- [常见问题](#常见问题)
- [License](#license)

---

## 特性

| 能力 | 说明 |
|------|------|
| 🔌 **gh api 驱动** | 模板经 `gh api repos/{owner}/{repo}/tarball` 拉取，自动解压、缓存、重试 |
| 📦 **多语言/多包管理** | 模板声明 `package_managers`，生成后给出对应的构建「下一步」提示 |
| 📝 **声明式清单** | 每个模板由 `template.yaml` 描述：参数 / 替换 / 重命名 / 删除 / post 钩子 |
| 🧩 **参数化** | `--param k=v` 覆盖 > 交互提示（TTY）> 默认值；内置 `name` 变量 + 11 个过滤器 |
| 🗂️ **路径重命名** | 包名 `com.example.app` → `com/xxx/yyy` 目录自动迁移，遗留空目录自动清理 |
| 🔧 **post 钩子** | 生成后执行任意命令（可在子目录 `cwd` 运行），适合接初始化脚本 |
| 🧊 **缓存** | 模板缓存于本地，重复生成不重复下载；`--refresh` 强制刷新 |
| 📱 **跨端** | 纯 Rust 静态可执行文件；Termux 可用；Unix 下自动补齐 `gradlew`/`*.sh` 可执行位 |

---

## 安装

已发布到 **crates.io**，并提供各平台预编译二进制（含 Termux）。

### 方式一：cargo binstall（预编译，推荐）

无需本地 Rust 工具链，直接从 GitHub Release 下载对应平台的二进制：

```bash
cargo binstall lyco
```

支持 Windows / macOS（Intel + Apple Silicon）/ Linux（x86_64 + arm64，musl 静态）/ **Termux（Android arm64）**。

### 方式二：cargo install（源码编译）

```bash
cargo install lyco
# 或从源码
git clone https://github.com/lilyco-42/lyco-engine
cd lyco-engine
cargo install --path .
```

**依赖**：Rust ≥ 1.70、[gh CLI](https://cli.github.com/)（`gh auth login` 登录）。

### 方式三：GitHub Release 手动下载

到 [Releases](https://github.com/lilyco-42/lyco-engine/releases) 下载对应平台的 `lyco-<target>` 二进制：

| 平台 | 文件 |
|------|------|
| Windows | `lyco-x86_64-pc-windows-msvc` |
| macOS Apple Silicon | `lyco-aarch64-apple-darwin` |
| macOS Intel | `lyco-x86_64-apple-darwin` |
| Linux x86_64 | `lyco-x86_64-unknown-linux-gnu` / `-musl` |
| Linux arm64 | `lyco-aarch64-unknown-linux-musl` |
| Termux / Android | `lyco-aarch64-linux-android` |

### 移动端 Termux / Android

纯 Rust 静态二进制，Termux 上直接可用：

```bash
pkg install rust gh          # gh CLI 用于拉取模板
cargo binstall lyco         # 或 cargo install lyco
lyco template list
```

- 生成项目中 `gradlew` / `*.sh` 自动补齐可执行位（Unix）。
- 交互参数提示在 TTY 下可用；脚本 / CI 环境自动使用默认值。
- 可配置路径见[环境变量](#环境变量)。

---

## 快速开始

```bash
# 用默认模板（rust-android）生成项目
lyco new myapp

# 指定模板 + 覆盖参数（跳过交互）
lyco new mymod --template fabric-mod --param mod_id=auto-sprint

# 从本地模板目录生成（离线可用）
lyco template add-local ./my-template --name my-template
lyco new demo --template my-template
```

首次运行会自动生成默认配置（内置 2 个模板），无需手动配置。

典型输出：

```
[fetch] 下载模板: gh api repos/lilyco-42/rust-android-template/tarball

✓ 项目已生成: mygame
  模板: rust-android · 语言: rust
  · package = com.lain42.mygame
  · appname = Mygame
  文件数: 26
  包管理器: gradle, cargo

→ 下一步:
  $ cd mygame/native_lib && cargo ndk --target aarch64-linux-android --platform 24 -- build --release
  $ cd mygame && ./gradlew assembleRelease
```

---

## 命令参考

```
lyco <COMMAND>
```

### `lyco new <name>` — 生成新项目

```
lyco new [OPTIONS] <NAME>
```

| 选项 | 说明 |
|------|------|
| `<NAME>` | 项目名（同时是输出目录名，默认 `./<name>`） |
| `-t, --template <id>` | 模板 id（默认 `rust-android`；见 `lyco template list`） |
| `--param <k=v>` | 覆盖参数，可多次使用：`--param package=com.x.y` |
| `-y, --yes` | 全部使用默认值，跳过交互提问 |
| `--refresh` | 忽略缓存，强制重新下载模板 |
| `-o, --output <dir>` | 输出目录（默认 `./<name>`） |
| `--force` | 目标目录已存在时直接覆盖（先删除再生成） |

示例：

```bash
lyco new app --template rust-android --param package=com.acme.app --param appname="Acme App"
lyco new server --template my-tpl --yes --output /work/projects/server
```

### `lyco template` — 模板管理

```
lyco template <COMMAND>
```

| 子命令 | 说明 |
|--------|------|
| `list [--remote]` | 列出已注册模板；`--remote` 同时用 `gh api user/repos` 展示 GitHub 候选 |
| `add <owner>/<repo> [--name <id>]` | 注册 GitHub 模板仓库（校验存在性；id 默认取仓库名） |
| `add-local <path> --name <id>` | 注册本地目录模板 |
| `remove <id>` | 注销模板并清理其缓存 |
| `refresh [id]` | 刷新仓库模板缓存（不填刷新全部） |

示例：

```bash
lyco template add lilyco-42/rust-android-template
lyco template add lilyco-42/fabric-mod-template --name fabric
lyco template add-local ./templates/compose-android --name compose-android
lyco template list --remote
```

### `lyco config path`

打印配置与缓存路径：

```
配置: C:\Users\you\AppData\Roaming\lyco\templates.yaml
缓存: C:\Users\you\AppData\Local\lyco\templates
```

---

## 模板系统

一次 `lyco new` 的完整流程：

```
1. 解析模板 id → 配置注册表取到来源（GitHub 仓库 / 本地目录）
2. 获取模板源码根
   · 仓库：gh api repos/{owner}/{repo}/tarball → 解压到缓存（失败自动重试）
   · 本地：直接使用源目录
3. 解析清单（优先级）：
   ① 配置注册项内联 manifest:（本地覆盖）
   ② 模板根 template.yaml（模板自带、可移植）
   ③ 回退：自动「纯拷贝」清单
4. 收集参数：--param 覆盖 > 交互提示(TTY) > 默认值
5. 渲染：内容替换 → 路径重命名 → 删除 → post 钩子 → 空目录清理
6. 输出生成摘要 + 「下一步」提示
```

**清单优先级**：配置内联 `manifest:` > 模板仓库根 `template.yaml` > 自动纯拷贝。
也就是说：模板仓库带上 `template.yaml` 即开箱即用；若想在本地覆盖其行为，在配置注册项中写 `manifest:` 即可。

---

## `template.yaml` 详解

模板清单是一份 YAML，可放在模板仓库根目录，也可内联在配置中。完整示例：

```yaml
# template.yaml
id: my-template                 # 模板 id（kebab-case）
name: 我的模板                  # 人类可读名称
description: 一句话说明
language: rust                  # 主语言（摘要展示）
tags: [android, compose]        # 分类标签
package_managers: [gradle, cargo]  # 包管理器（生成后提示依据）

# 收集的参数
params:
  - key: package                 # 参数名（--param 键）
    label: 包名                  # 交互提示标签
    default: com.example.app     # 默认值，可引用 {{name}} 与更早定义的参数
    help: 例如 com.mycompany.myapp   # 交互时的帮助说明
    required: true               # 空值时报错（默认 false）

# 内容占位符替换（按声明顺序依次应用，字面匹配，非正则）
replace:
  - find: com.example.app        # 模板中的原始占位符
    with: "{{package}}"          # 替换值，支持模板与过滤器
  - find: com_example_app
    with: "{{package|rust}}"
  - find: com/example/app
    with: "{{package|path}}"

# 目录 / 文件重命名（from 为原始相对路径；to 支持模板；自动清理遗留空目录）
rename:
  - from: app/src/main/java/com/example/app
    to: "app/src/main/java/{{package|path}}"
  - from: src/main/resources/template-mod.mixins.json
    to: "src/main/resources/{{mod_id}}.mixins.json"

# 从输出中删除的模板文件（相对路径；目录规则会级联删除整个子树，容忍尾部斜杠）
delete:
  - README.md
  - docs/                        # 级联删除 docs/ 下所有文件

# 生成后执行的钩子（在输出目录内；cwd 可指定子目录）
post:
  - command: bash
    args: ["setup.sh", "{{package}}"]
    cwd: .

# 生成后的「下一步」提示（支持 {{name}}）
next_steps:
  - "cd {{name}} && cargo build"
  - "cd {{name}} && ./gradlew runClient"
```

### 字段说明

| 字段 | 必填 | 说明 |
|------|------|------|
| `id` | ✓ | 唯一 id，`--template` 使用 |
| `name` / `description` / `language` / `tags` | | 展示用元信息 |
| `package_managers` | | 摘要中提示使用的包管理器 |
| `params` | | 参数列表（`key` / `label` / `default` / `help` / `required`） |
| `replace` | | 内容替换规则，**顺序敏感**（如 `com.example.template` 应先于 `com.example`） |
| `rename` | | 路径重命名，最长前缀优先匹配 |
| `delete` | | 删除文件 / 目录（目录级联） |
| `post` | | 生成后钩子（`command` / `args` / `cwd`） |
| `next_steps` | | 摘要末尾的「下一步」命令 |

---

## 内置变量与过滤器

### 内置变量

| 变量 | 说明 |
|------|------|
| `{{name}}` | 项目名（`lyco new <name>` 的 name） |

### 过滤器（`{{key|filter1|filter2}}`，从左到右依次应用）

| 过滤器 | 作用 | 示例（`com.example.app`） |
|--------|------|---------------------------|
| `path` | `.` → `/` | `com/example/app` |
| `rust` | `.` → `_` | `com_example_app` |
| `parent` | 去掉最后一段 | `com.example` |
| `pascal` | 帕斯卡命名 | `ComExampleApp` |
| `camel` | 驼峰命名 | `comExampleApp` |
| `snake` | 蛇形命名 | `com_example_app` |
| `kebab` | 短横线命名 | `com-example-app` |
| `upper` | 全大写 | `COM.EXAMPLE.APP` |
| `lower` | 全小写 | `com.example.app` |
| `space` | `_` / `-` → 空格 | `com.example.app` |

过滤器会先按分隔符与驼峰边界切词再转换，`{{name|pascal}}` 对 `auto-sprint` → `AutoSprint`、
对已是驼峰的 `AutoSprint` 保持原样。

**参数解析顺序**：参数按 `params` 声明顺序解析，后定义的默认值可引用 `name` 与更早定义的参数。
`--param` 覆盖值同样支持模板（如 `--param appname={{package|parent}}`）。

---

## 内置模板

首次运行自动注册以下模板（来源：你的现有仓库）：

| id | 来源 | 语言 | 生成内容 |
|----|------|------|----------|
| `rust-android` | `lilyco-42/rust-android-template` | Rust + Kotlin | Android Compose 工程：包名/JNI 名/目录全替换，JNI 桥接 |
| `fabric-mod` | `lilyco-42/fabric-mod-template` | Java | Minecraft Fabric 1.21.4 Mod：mod id / 主类 / assets / mixin 全替换 |

```bash
lyco new mygame --template rust-android --param package=com.lain42.mygame
lyco new auto-sprint --template fabric-mod --param mod_id=auto-sprint --param package=com.example.autosprint
```

---

## 自定义模板

### 方式一：GitHub 仓库模板

```bash
lyco template add <owner>/<repo> [--name <id>]
```

- 校验仓库存在性后注册。
- 若仓库根目录有 `template.yaml`，自动采用其声明式清单。
- 若没有，则按「纯拷贝」渲染（需在配置中加内联 `manifest:` 才能参数化）。

```bash
lyco template add lilyco-42/my-android-template --name my-android
lyco new app --template my-android --param package=com.acme.app
```

### 方式二：本地目录模板（离线 / 自定义）

```bash
lyco template add-local <path> --name <id>
lyco new demo --template <id>
```

本地目录同样优先读取其 `template.yaml`。适合私有模板、离线环境、开发调试。

---

## 配置文件参考

配置文件：`{config_dir}/lyco/templates.yaml`（可用 `LYCO_CONFIG` 覆盖）。首次运行自动生成。

```yaml
templates:
  rust-android:            # 模板 id
    source: repo           # 来源类型：repo（GitHub 仓库）| local（本地目录）
    repo: lilyco-42/rust-android-template   # repo 型的仓库
    # manifest:            # 可选：内联清单（覆盖模板仓库内的 template.yaml）
    #   id: rust-android
    #   params: [ ... ]
    #   replace: [ ... ]
    #   rename: [ ... ]
  my-local:
    source: local
    path: /home/you/templates/my-local
```

手动编辑后立即生效。可用 `lyco template list` / `remove` 通过 CLI 管理，也可直接编辑该文件。

---

## 环境变量

| 变量 | 默认 | 说明 |
|------|------|------|
| `LYCO_CONFIG` | `{config_dir}/lyco/templates.yaml` | 配置文件路径 |
| `LYCO_CACHE` | `{cache_dir}/lyco/templates` | 模板缓存根目录 |
| `LYCO_BIN` | — | （仅测试）冒烟测试指定二进制 |

---

## 约定与注意事项

1. **空目录需 `.gitkeep` 才保留**：生成后空目录会被清理（`.gitkeep` 文件可保留空目录）。
2. **二进制文件原样拷贝**：含 NUL 字节的文件（如 `gradle-wrapper.jar`）不做替换。
3. **`replace` 顺序敏感**：`find` 为字面字符串，按声明顺序依次应用；短占位符请放在长占位符之后
   （如 `com.example` 应放在 `com.example.template` 之后）。
4. **`delete` 目录级联**：`delete: [docs/]` 会删除整个 `docs/` 子树。
5. **Windows 下 MSYS 路径**：bash 会转换**命令行参数**里的 `/tmp/...`，但 lyco 从**配置文件**读取的
   路径不会转换——Windows 上手写配置的本地路径请用 `C:\...` 风格（Linux / Termux 无此问题）。
6. **交互提示**：仅在 TTY 下出现；脚本 / CI / agent 环境自动使用默认值（或用 `--yes`）。
7. **`--force` 会先删除目标目录**再生成，请确认目标目录可覆盖。

---

## 开发与测试

```bash
cargo build                # 构建（target/debug/lyco）
cargo test                 # 单元测试（过滤器、渲染、重命名、目录清理、级联删除）
cargo clippy --all-targets # 静态检查
cargo fmt                  # 格式化

# 端到端冒烟测试（隔离配置/缓存，不影响真实配置）
bash tests/smoke.sh        # 113+ 项断言：CLI 表面/模板管理/生成正确性/错误路径/健壮性
```

冒烟测试使用隔离的 `LYCO_CONFIG` / `LYCO_CACHE`，不会触碰真实配置；模板缓存持久于
`target/.smoke-cache`，复跑稳定且不重复下载。用 `LYCO_BIN` 可指定其他二进制（如 release）。

### 发布（GitHub Actions 自动）

`.github/workflows/build-release.yml` 在 push / PR 到 main 时构建验证；打版本 tag 自动发布：

```bash
# 1) bump 版本
#    Cargo.toml 的 version 改为新版本
cargo publish --registry crates-io          # 发布到 crates.io

# 2) 打纯版本号 tag（触发 CI 构建全平台 + 发布 GitHub Release）
git tag 0.1.3 && git push origin 0.1.3
```

CI 构建产物（`cargo binstall` 用）：

| 目标 | 说明 |
|------|------|
| Windows / macOS / Linux | 主流平台，资产 `lyco-<target>` |
| Termux / Android | `aarch64-linux-android`（NDK + cargo-ndk） |
| yuzu 引擎 | `yuzu-cli` / `yuzu-server`（主流平台） |
| WASM | web 播放器引擎绑定 |

> tag 必须用**纯版本号**（如 `0.1.3`）而非 `v0.1.3`，`cargo binstall` 依此定位 release 资产。

---

## 常见问题

**Q：`lyco new` 提示「未知模板」？**
先 `lyco template list` 确认 id；新仓库需先 `lyco template add <owner>/<repo>` 注册。

**Q：生成的包名没替换干净？**
检查模板清单的 `replace` 顺序：短占位符应在长占位符之后；`find` 必须与模板内字节完全一致。

**Q：`gh api` 报错 / 下载失败？**
确认 `gh auth login` 已登录、`gh api user --jq .login` 有输出；下载内置重试，网络抖动会自动重试。

**Q：post 钩子报「cwd 目录不存在」？**
post `cwd` 指向的目录若在模板中是空目录，会被清理——在其中放 `.gitkeep`，或改用模板根目录。

**Q：想改模板参数 / 替换规则？**
编辑配置文件的 `manifest:` 段（本地覆盖优先于仓库 `template.yaml`），保存即生效。

---

## License

MIT
