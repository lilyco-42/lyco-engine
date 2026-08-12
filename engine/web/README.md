# yuzu-web — 柚子社剧本浏览器播放器

完整游戏体验:**WASM 游戏引擎**(客户端编译剧本/驱动对话)+ **web 后端**
(Rust `yuzu-server`,懒加载游戏资源 / 反编译剧本 / 解码图像)。数据本地处理,不上传。

## 运行

```sh
# 1. 重建(wasm 绑定 + 后端 + 冒烟)
bash web/build.sh

# 2. 起后端(同时服务页面 + 资源 API)
yuzu-server --data realgame --web web --addr 0.0.0.0:8080
# 打开 http://localhost:8080
```

`--data` 指向含 `.xp3` 的游戏数据目录(真实千恋万花 `data.xp3` / `fgimage1080.xp3` 等),
`--web` 指向本 web 目录。

## 后端 API(懒加载 / 反编译 / 解码)

| 端点 | 说明 |
|---|---|
| `GET /api/archives` | 列出数据目录 .xp3(只读文件名) |
| `GET /api/archives/:name/files` | 条目列表(首次访问懒打开索引并缓存) |
| `GET /api/archives/:name/file?path=…` | 原始提取单个文件(仅解压被请求条目) |
| `GET /api/archives/:name/scn?path=…` | 反编译 SCN 剧本为可读文本 |
| `GET /api/archives/:name/img?path=…` | 解码图像(TLG/PSB→PNG,WebP/PNG 透传) |
| 其它 | 静态服务 web 播放器 |

## 玩法(完整游戏体验)

- 打开页面,播放器经后端**懒加载**列出归档 → 自动打开 `data.xp3` → 解包剧本播放。
- **自动流程**:剧本内场景按 `nexts` 自动衔接(修复了对象格式 `nexts` 解析 bug),
  并在章节末**跨文件**载入下一剧本。**整个游戏主线可连续加载**:47 个选择点、
  7 条路线结局(普通/芳乃/茉子/ムラサメ/レナ/小春/芦花)、86 个剧本文件全流程可达
  (经后端懒加载 + wasm 编译验证)。
- **对话**:点击舞台 / 空格推进;台词含 **语音**(`voice.xp3` 懒加载,`<audio>` 播放,
  显示 🔊 指示)。
- **立绘/背景随剧本切换**:`cg_背景名` → 懒加载 bgimage1080 对应图并切换;
  `cg_角色.stand` → fgimage1080 取该角色整身立绘(最大变体)上舞台。
- **选择分支**:到达 `*_sel` 选择点时按**场景图**(`scnchartdata.tjs`,解码自
  KiriKiri 场景编码)展示真实分支目标;显示名选项(如 `【エピローグ】`)跟随该场景
  自身流;到达 `*gameend_title` 视为游戏终点。
- **切换场景/归档**:侧栏场景下拉、归档下拉;点条目查看/下载资源。
- **本地 .xp3**:不经后端,浏览器内 wasm 解包。

> `data/realgame.xp3` 由真实资源打包(剧本+背景+角色);真实 `data.xp3` /
> `fgimage1080.xp3` / `bgimage1080.xp3` / `voice.xp3` 放在 `realgame/` 目录由后端懒加载。

## 前置(重建时)

- Rust `wasm32-unknown-unknown` target
- `wasm-bindgen-cli` 0.2.127(与 `Cargo.lock` 中 wasm-bindgen 版本一致):
  ```sh
  cargo install wasm-bindgen-cli --version 0.2.127
  ```

## 目录

| 路径 | 说明 |
|---|---|
| `index.html` / `style.css` / `main.js` | 播放器页面(后端懒加载 + 本地解包双模式) |
| `pkg/` | 浏览器版 wasm 绑定(`--target web`) |
| `pkg-node/` | Node 版绑定(冒烟用) |
| `smoke.mjs` / `backend-smoke.mjs` | wasm 端到端 / 后端契约冒烟 |
| `data/game.xp3` | 合成演示归档(无后端时的本地回退) |
| `data/realgame.xp3` | 真实资源打包(剧本+背景+角色) |
| `build.sh` | 重建 wasm + 后端 + 冒烟 |

> 说明:归档内剧本若为柚子社加密(CxScheme/Riddle),需先经 `CxScheme::from_tpm`
> 配置控制块;当前演示数据(千恋万花 Kirikiroid2 版)未加密可直接读。
