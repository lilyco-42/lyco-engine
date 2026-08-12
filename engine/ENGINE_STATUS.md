# yuzu 引擎 — 交接状态(2026-08-12)

> **需求已降级**:不再追求「完全复刻整个游戏引擎」,改为**实现游戏完整的可玩性**——
> 基于 WASM 的跨平台视觉小说运行时:客户端 WASM 引擎 + 服务端资源懒加载/热更新 + Web 播放器,
> 让《千恋万花》在浏览器里完整可玩(全流程、全选择、可存档、可回看、可设置)。

## 架构
```
yuzu-scn(解析 .scn PSB) → yuzu-kag(引擎:图层/音频/流程状态机,存档 JSON round-trip)
                               ↓ 效果流(dialogue/layer/audio/chapter/wait)
yuzu-wasm(kag_run_scene) ← web 播放器(canvas 舞台 / 台词 / 语音 / BGM / 选择)
yuzu-xp3(归档/压缩/加密/场景文本解码) | yuzu-pimg/tlg(纹理)
yuzu-server(懒加载 / 反编译 / 解码 / /api/version 热更新)
```

## 状态:56 tests / 21 suites 全绿,clippy 0 警告
- ✅ 引擎:yuzu-kag(含存档/读取)、yuzu-scn、yuzu-xp3(25)、yuzu-pimg、yuzu-tlg、yuzu-engine、yuzu-server、yuzu-wasm
- ✅ 资源:XP3 懒加载 / 反编译 / 解码 / 版本热更新(/api/version + 客户端比对)
- ✅ 音频:voice.xp3(M4A)、bgm.xp3(opus)实际播放
- ✅ 逆向:scnchartdata 场景图、全部 47 选择点真实目标、场景文本解码(移植 krkrz TextStream)

## ✅ 完整可玩性(2026-08-12 新增)
- **全流程**:标题屏 → 开始游戏 → 章节流(场景图导航,跨文件跳转)→ 47 选择点 → 7 结局
- **存档/读档**:5 个本地槽位(localStorage),存当前场景+剧本+数据源,读档即恢复续播
  (`savecheck.mjs` 验证恢复路径:*com_part_1:2 读档后 211 步可续播)
- **回看历史**:记录全部台词,⌘/↑ 或控制栏「回看」打开,含说话人+场景标签
- **设置**:BGM/语音音量滑块、自动间隔(快/普通/慢),持久化 localStorage
- **自动/跳过**:控制栏开关;自动按设定间隔推进,跳过不停留台词;到选择点自动停;
  自动/跳过开启时单击仅停模式(标准 VN 行为)
- **语音重放**:点击 🔊 重播最近语音
- **推进方式**:点击舞台 / 空格 / 回车 / → / 滚轮
- **回到标题**:控制栏「标题」随时返回标题屏

## ✅ 客户端流式加载层(2026-08-12)
- **`web/cache.mjs` 两级缓存**:
  - 内存 LRU 热层(64MB,按字节逐出最久未用)
  - **IndexedDB 持久层**:读顺序 内存 → 持久 → 网络;写 内存 + 异步落盘
  - 剧本/解码图像/BGM/语音全部经此层;**跨会话复用,同一资源只从后端拉一次**(离线/秒开)
  - 内存逐出同步删持久条目,存储量跟随工作集,不无限膨胀
  - 非浏览器(Node 冒烟)自动降级为纯内存
  - **故障切换加固**(2026-08-12):`openDb` 处理 `onblocked`(标签页阻塞)+ 3s 超时;`dbGet/dbCount`
    兜底 1.5s 超时 + try/catch;`getPersist` 整体 try/catch —— **永不 reject、永不挂起**,
    任何 IndexedDB 异常都按未命中降级、网络兜底(修复浏览器"开始游戏无反应"的缓存挂起/抛错根因)
- **浏览器实机修复**(2026-08-12,据用户控制台日志):
  - 后端 `repo.read` 加**大小写不敏感回退**(KiriKiri 语义):`bgm_BGM01I` → 归档 `BGM01i.opus`(实测 200,音乐恢复)
  - 移除 `evimage1080.xp3`(realgame 无此归档,启动 404 噪音)
  - `applyEffect`/`showBackground`/`showChara` 加 try/catch:单资源缺失只日志不中断播放流
  - 全局 `unhandledrejection` → 打进引擎日志面板(含堆栈,不再只黑在控制台)
  - **去掉页面加载自动开播**(`openSource` 改 `loadScn(bytes,false)`):游戏只从标题屏"开始游戏"进入,
    修"点开始游戏无反应"(实为自动开播已重播同一段开场,视觉零变化)
  - 静态服务加 `Cache-Control: no-cache`(tower-http set-header):杜绝旧 HTML/JS 缓存版本错配;
    `$` 空值诊断打印缺失 `#id`
  - **CSS hidden 兜底(终极根因)**:`.title-screen{display:flex}` / `.stage-empty{display:grid}`
    覆盖了 UA 的 `[hidden]{display:none}` → 标题屏隐藏不生效、游戏在标题下播放 = "开始游戏无反应"。
    全局 `[hidden]{display:none!important}` 强制隐藏,任何 display 规则不得覆盖
- **素材渲染顺序对齐 APK**(2026-08-12):`parse_envupdate` 重写为 serde_json 解析 ——
  千恋万花的 envupdate `update` 载荷是**数组 Value**(`[{class,name,redraw:{imageFile:{file}}}]`),
  旧解析器只认 `["name","class",...]` 字符串数组 → stage 背景全漏。
  新解析器:同时支持字符串/数组载荷,只吃 `update`(忽略 `revupdate` 回滚)。
  第一章开场渲染顺序现与 APK 一致:
  `Stage=画面_黒`(黑屏)→ 台词 → `Stage=空_青空`(渐变)→ `Stage=街_雰囲気モブA` → `Character=芦花`。
  播放器图层资源改取 `e.file`(图像名)而非 `e.name`(图层名);新增回归测试
  `envupdate_object_format_parses_stage`(yuzu-kag 9 tests / 全工作区 57)
- **立绘面部组合(对齐 APK)**(2026-08-13):逆向出 KiriKiri stand 合成机制 ——
  身体集(`芦花a`)+ 合成表(`芦花a.txt` TSV,`#layer_type` 表头)→ 把多层按 `(left,top)` 叠到
  3600×5100 画布:衣服层(私服683+腕差分684,visible=1)+ 表情脸(group 679:ベース275/笑顔1=300/
  悲しい=437…)+ 附属(頬689)。此前播放器只挑最大单层图(芦花a_710=甘味処服腕差分),故无表情。
  客户端 `compositeStand()` canvas 合成(按 角色|表情 缓存);`stand-check.mjs` 验证选层正确。
- **表情随台词切换(对齐 APK)**(2026-08-13):APK 的表情由场景 `imageFile.options.face` 索引驱动
  (如 `{file:"芦花.stand",options:{dress:"私服",face:"13",pose:"3"}}`,face=13=不満げ)。
  实现链路:
  1. yuzu-kag `collect_layer_ops` 提取 `options.face/dress` → `LayerImage.face/dress` → 效果携带
  2. wasm effect_json 加 `face`/`dress` 字段
  3. 播放器:face 索引 → `*_info.txt` 表情名(data.xp3 `fgimage\`)→ 合成表 layer_id → 重合成
  实测开场 face 序列 04→13→21→02… 变化;`face-resolve.mjs` 验证 01→ベース275、
  04→きょとん347、13→不満げ599、21→複合695(组709)互不相同
- **选择流程修复**(2026-08-13,浏览器实测):
  - `choosing` 等待态:选项展示期间 `advanceEngine` 不跟随 sel 场景自身跳转(此前残留推进把选择
    抢跑成"场景结束/无法选择")
  - 进入正常场景时清空 `#choices`(此前点选后旧选项按钮叠在对话上)
  - `playVoice`/🔊 空值防护(voice-ind 缺失时不再刷 unhandledrejection)
  - 选择后分支续流已验证:`*001_01A/B → *dummy3/4 → *001_01com(1946 台词)→ 跨文件
    `002・祟り神ver1.08.ks::*com_part_2`(HTTP 200)
- **预取**:剧本编译后后台预取本文件全部 `cg_`/`bgm_` 引用 → 转场零等待;
  跳转/章节加载时对新剧本同样预取
- **可见统计**:侧栏「流式缓存」实时显示 内存命中/持久命中/拉取/节省字节/持久项数
- **启动预热**:`rememberLastPlayed` 存上次播放剧本引用(`yuzu-lastplayed-v1`);
  `warmupStartup` 在标题屏前把上次剧本+场景图从持久层拉进内存(零网络);
  顺序保证:版本比对先失效陈旧 → 再预热(不拉已变更归档)
- **验证**:`cachecheck.mjs`(11 项) —— 同实例二次读取 1 次网络;新会话从持久层恢复零网络;
  **warmup 新实例预热 1 项入内存、总拉取仍 1**;invalidate 精确失效、`/api/version` 形状

## ✅ 跨平台 WASM 客户端宿主(2026-08-12)
- **`web/run.mjs`**:Node WASM 宿主 —— 复用同一 `compile_scn`/`kag_run_scene`(wasm)+ 同一 `cache.mjs`
  流式缓存,在非浏览器跑剧本(含跨文件跳转)。证明规格「采用 WASM 作为客户端运行时」不受浏览器绑定
- 用法:`node run.mjs [--scene LABEL] [--steps N]`(默认 *com_part_1 → data.xp3)
- 实测:*com_part_1 经 stub 链流入台词场景,106 句台词 / 44 音频 / 2 章节 / 1 次网络拉取
- 也作冒烟:无台词即退出码 1

## ✅ 热更新客户端动作(2026-08-12)
- `/api/version` 比对到归档变更/新增 → `cache.invalidate(谓词)` **精确失效该归档**的内存+IndexedDB 条目,下次按新版本拉取;顶部琥珀横幅提示「资源已更新…」,10s 自动隐藏
- 旧归档索引未失效(核对发现数据1080.xp3 无 'data.xp3' 子串,谓词精确)
- 验证:`cachecheck.mjs` —— invalidate 只清归档 A(B 保留,含持久层)、`/api/version` 形状 7 归档

## 运行
```bash
yuzu-server --data realgame --web web   # 后端:资源懒加载 + 静态 web
# 浏览器打开 http://127.0.0.1:8080/
```
冒烟:`web/*.mjs`(backend-smoke / flow-check / fullflow-check / savecheck 等)

## 关键路径
- 播放器: `web/main.js`(引擎驱动,非步骤播放器)+ `web/index.html` / `web/style.css`
- 引擎: `crates/yuzu-kag/src/lib.rs`
- 后端: `crates/yuzu-server/src/{repo,api,decode}.rs`
- 逆向: `realgame/`(scn/scnchartdata.tjs 解码源、libgame.so、选卡)

## 已知限制(不再追引擎复刻)
- TJS 解释器未移植(选卡/流程图逻辑已用 scnchartdata 数据驱动替代,不阻塞可玩)
- KAG 标签仅子集(可玩所需已覆盖)
- 选项原文为未随游戏的 .ks 源码,播放器用分支首句台词作选项标签
