// yuzu-web —— 完整的游戏体验播放器
// 数据源:
//   1) 后端懒加载(默认):/api/archives/:name/{files,file,img,scn} 按需取资源;
//   2) 本地文件:浏览器内 wasm 解包(xp3_info / xp3_read)。
// 剧本在客户端 wasm 编译(yuzu-engine),图像后端解码或 wasm 解码。

import init, {
  version,
  compile_scn,
  decode_tlg_png,
  decode_scenario,
  kag_run_scene,
  render_pimg_png,
  xp3_info,
  xp3_read,
} from "./pkg/yuzu_wasm.js";
import { AssetCache } from "./cache.mjs";

const $ = (id) => {
  const el = document.getElementById(id);
  if (!el) console.error(`[DOM缺失] #${id} 不存在(HTML 与 JS 版本不匹配?)`);
  return el;
};

// ---------------- 状态 ----------------
let scenes = [];          // 编译后的场景 [{ label, steps }]
let currentRun = null;    // 当前播放进度 { steps, index }
let scnName = null;
let currentSource = null; // { kind:'backend', name } | { kind:'local', bytes }

// 流式加载缓存:剧本/图像/音频 字节一次拉取,重放与跳回零网络
const cache = new AssetCache();
const cacheKey = (source, path, tag = "") => `${source.kind}:${source.name}:${path}${tag}`;

// ---------------- 设置 / 存档 / 历史 ----------------
const SETTINGS_KEY = "yuzu-settings-v1";
const SAVE_KEY = "yuzu-save-v1";
const LAST_KEY = "yuzu-lastplayed-v1";
let settings = { bgm: 0.8, voice: 1.0, speed: 0.8 };
try { settings = Object.assign(settings, JSON.parse(localStorage.getItem(SETTINGS_KEY) || "{}")); } catch {}
let autoMode = false, skipMode = false, autoTimer = null;
const backlog = [];       // [{ who, text, scene }]
let lastVoiceSrc = null;  // 最近语音(点击 🔊 重放)

// ---------------- 日志 ----------------
function log(...parts) {
  const el = $("log");
  const line = parts.join(" ");
  el.textContent += line + "\n";
  el.scrollTop = el.scrollHeight;
  console.log(line);
}

// ---------------- 字节工具 ----------------
function downloadBytes(name, bytes) {
  const blob = new Blob([bytes], { type: "application/octet-stream" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  URL.revokeObjectURL(url);
  log(`[下载] ${name} (${bytes.length} bytes)`);
}

// ---------------- 数据源抽象 ----------------
async function api(path) {
  const r = await fetch(path);
  if (!r.ok) throw new Error(`${path} → HTTP ${r.status}`);
  return r;
}
const apiBytes = async (p) => new Uint8Array(await (await api(p)).arrayBuffer());
const apiJson = async (p) => (await api(p)).json();

async function listEntries(source) {
  if (source.kind === "backend") {
    const d = await apiJson(`/api/archives/${encodeURIComponent(source.name)}/files`);
    return d.files;
  }
  return JSON.parse(xp3_info(source.bytes)).files;
}
async function readEntry(source, path) {
  const key = cacheKey(source, path);
  const hit = await cache.getPersist(key);
  if (hit) { updateCacheStats(); return hit; }
  let bytes;
  if (source.kind === "backend") {
    bytes = await apiBytes(`/api/archives/${encodeURIComponent(source.name)}/file?path=${encodeURIComponent(path)}`);
  } else {
    bytes = xp3_read(source.bytes, path);
  }
  cache.set(key, bytes);
  updateCacheStats();
  return bytes;
}
/// 取可显示图像字节:后端走 /img(解码优化);本地 wasm 解 TLG,其余透传。
/// 解码后的 PNG 一并入缓存(内存 + IndexedDB),重放不再重解码/重拉。
async function readImage(source, path) {
  const key = cacheKey(source, path, "#img");
  const hit = await cache.getPersist(key);
  if (hit) { updateCacheStats(); return hit; }
  let bytes;
  if (source.kind === "backend") {
    bytes = await apiBytes(`/api/archives/${encodeURIComponent(source.name)}/img?path=${encodeURIComponent(path)}`);
  } else {
    const raw = xp3_read(source.bytes, path);
    bytes = isTlgBytes(raw) ? decode_tlg_png(raw) : raw;
  }
  cache.set(key, bytes);
  updateCacheStats();
  return bytes;
}

async function updateCacheStats() {
  const el = $("cache-stats");
  if (!el) return;
  const s = await cache.stats();
  el.textContent = `流式缓存: ${s.hits} 内存命中 / ${s.persistHits} 持久命中 / ${s.misses} 拉取 · 省 ${fmtSize(s.saved)} · ${s.persisted} 项持久`;
}

/// 热更新提示横幅,10s 自动隐藏。
function showUpdateBanner(msg) {
  const el = $("update-banner");
  if (!el) return;
  el.textContent = msg;
  el.hidden = false;
  clearTimeout(showUpdateBanner._t);
  showUpdateBanner._t = setTimeout(() => { el.hidden = true; }, 10000);
}

// ---------------- 启动 ----------------
async function main() {
  await init();
  $("engine-version").textContent = version();
  log(`引擎 ${version()} 就绪`);
  // 兜底:任何未处理拒绝都打进日志面板(而非只出现在控制台)
  window.addEventListener("unhandledrejection", (ev) => {
    const reason = ev?.reason;
    log(`[未处理拒绝] ${reason?.message || reason}`);
    if (reason?.stack) log("    " + reason.stack.split("\n").slice(0, 3).join("\n    "));
    ev.preventDefault();
  });
  // 服务端版本清单(热更新):比对归档指纹,增量拉取
  try {
    const v = await apiJson("/api/version");
    const CACHE_KEY = "yuzu-manifest-v1";
    let cached = {};
    try { cached = JSON.parse(localStorage.getItem(CACHE_KEY) || "{}"); } catch {}
    const changed = [], added = [];
    for (const a of v.archives) {
      const prev = cached[a.name];
      if (!prev) added.push(a.name);
      else if (prev.file_size !== a.file_size || prev.modified !== a.modified) changed.push(a.name);
    }
    localStorage.setItem(CACHE_KEY, JSON.stringify(Object.fromEntries(v.archives.map(a => [a.name, { file_size: a.file_size, modified: a.modified }]))));
    log(`服务端 v${v.version}: ${v.archives.length} 个归档`);
    // 热更新:归档变更 → 失效其缓存条目(内存 + IndexedDB),下次按新版本拉取
    const hot = [...changed, ...added];
    let evicted = 0;
    for (const name of hot) evicted += cache.invalidate((k) => k.includes(`:${name}:`));
    if (hot.length) {
      log(`  [热更新] ${hot.join(", ")} 已失效 ${evicted} 条缓存`);
      showUpdateBanner(`资源已更新: ${hot.join(", ")} · 已刷新本地缓存`);
    } else {
      log("  (与本地缓存一致,无增量)");
    }
  } catch { log("[提示] 版本清单不可用"); }

  // 启动预热:上次播放的剧本 + 场景图从持久缓存秒回内存(零网络)
  await warmupStartup();

  // 后端模式:列出归档,默认打开 data.xp3
  try {
    await loadBackendArchives();
  } catch (e) {
    log("[提示] 后端不可用,回退本地演示:", e.message);
    const local = { kind: "local", bytes: await fetchBytes("data/game.xp3") };
    await openSource(local);
  }

  // 原作流程:标题屏 → 开始游戏 / 场景图
  $("title-newgame").addEventListener("click", async () => {
    try {
      $("title-screen").hidden = true;
      await ensureSceneChart();
      await playChapter("s001");
    } catch (e) {
      log("[错误] 开始游戏失败:", e?.message || e);
      console.error(e);
    }
  });
  $("title-chart").addEventListener("click", () => {
    $("title-screen").hidden = true;
    ensureSceneChart();
    $("chapter-select").scrollIntoView({ behavior: "smooth" });
  });
  $("title-extra").addEventListener("click", () => log("[Extra] 后日谈章节需通关解锁(场景图可选)"));
  $("title-screen").hidden = false;

  // 本地 .xp3:浏览器内解包
  $("file-input").addEventListener("change", async (ev) => {
    const file = ev.target.files?.[0];
    if (!file) return;
    try {
      await openSource({ kind: "local", bytes: new Uint8Array(await file.arrayBuffer()) });
    } catch (e) {
      log("[错误] 打开归档失败:", e.message);
    }
    ev.target.value = "";
  });

  // 归档切换
  $("archive-select").addEventListener("change", async (ev) => {
    const name = ev.target.value;
    if (!name) return;
    try {
      await openSource({ kind: "backend", name });
    } catch (e) {
      log("[错误] 打开归档失败:", e.message);
    }
  });

  // 交互:点击舞台 / 空格 / 回车 / → 推进;↑ 回看历史
  $("stage").addEventListener("click", manualAdvance);
  $("stage").addEventListener("wheel", (e) => { e.preventDefault(); manualAdvance(); }, { passive: false });
  document.addEventListener("keydown", (e) => {
    if (["Space", "Enter", "ArrowRight", "ArrowDown"].includes(e.code)) {
      e.preventDefault();
      manualAdvance();
    } else if (e.code === "ArrowUp") {
      e.preventDefault();
      openBacklog();
    }
  });

  $("play-btn").addEventListener("click", () => {
    const label = $("scene-select").value;
    if (label) playScene(label);
  });

  // 播放控制:自动 / 跳过 / 回看 / 存档 / 读档 / 标题
  $("ctl-auto").addEventListener("click", () => {
    autoMode = !autoMode;
    skipMode = false;
    $("ctl-auto").classList.toggle("active", autoMode);
    $("ctl-skip").classList.remove("active");
    log(autoMode ? "[自动] 开" : "[自动] 关");
    if (autoMode) advanceEngine();
  });
  $("ctl-skip").addEventListener("click", () => {
    skipMode = !skipMode;
    autoMode = false;
    $("ctl-skip").classList.toggle("active", skipMode);
    $("ctl-auto").classList.remove("active");
    log(skipMode ? "[跳过] 开" : "[跳过] 关");
    if (skipMode) advanceEngine();
  });
  $("ctl-backlog").addEventListener("click", openBacklog);
  $("ctl-save").addEventListener("click", () => openSaveModal(true));
  $("ctl-load").addEventListener("click", () => openSaveModal(false));
  $("ctl-title").addEventListener("click", () => {
    stopAuto();
    $("title-screen").hidden = false;
  });
  $("backlog-close").addEventListener("click", () => $("backlog-modal").hidden = true);
  $("save-close").addEventListener("click", () => $("save-modal").hidden = true);
  for (const id of ["backlog-modal", "save-modal"]) {
    $(id).addEventListener("click", (e) => { if (e.target === $(id)) $(id).hidden = true; });
  }
  // 🔊 重放最近语音
  const voiceInd = $("voice-ind");
  if (voiceInd) {
    voiceInd.addEventListener("click", () => {
      if (lastVoiceSrc) { const a = $("voice"); a.src = lastVoiceSrc; a.play().catch(() => {}); }
    });
  }
  bindSettings();
}

// ---------------- 后端:归档列表 ----------------
async function loadBackendArchives() {
  const { archives } = await apiJson("/api/archives");
  if (!archives.length) throw new Error("后端无归档");

  const sel = $("archive-select");
  sel.textContent = "";
  for (const a of archives) {
    const opt = document.createElement("option");
    opt.value = a.name;
    opt.textContent = `${a.name} (${fmtSize(a.file_size)})`;
    sel.appendChild(opt);
  }
  log(`后端发现 ${archives.length} 个归档`);
  // 优先打开 data*.xp3(剧本),否则第一个
  const pick = archives.find((a) => /^data/.test(a.name)) || archives[0];
  sel.value = pick.name;
  await openSource({ kind: "backend", name: pick.name });
}

// ---------------- 打开数据源 ----------------
async function openSource(source) {
  currentSource = source;
  const files = await listEntries(source);
  const label = source.kind === "backend" ? source.name : "(本地文件)";
  $("xp3-summary").textContent = `${label} · ${files.length} 个文件`;

  const ul = $("xp3-files");
  ul.textContent = "";
  for (const f of files) {
    const li = document.createElement("li");
    li.tabIndex = 0;
    li.setAttribute("role", "button");
    const name = document.createElement("span");
    name.className = "fname";
    name.textContent = f.name;
    const size = document.createElement("span");
    size.className = "fsize";
    size.textContent = fmtSize(f.size);
    li.append(name, size);
    li.addEventListener("click", () => unpackEntry(source, f.name));
    li.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        unpackEntry(source, f.name);
      }
    });
    ul.appendChild(li);
  }
  log(`解包 ${label}:${files.length} 个文件`);

  // 自动播放 .scn,展示前两个图片(单资源失败不中止整体打开)。
  // 注意:不自动开演 —— 游戏只从标题屏"开始游戏"进入(autoStart=false)。
  const scn = files.find((f) => f.name.endsWith(".scn"));
  if (scn) {
    try {
      const bytes = await readEntry(source, scn.name);
      log(`[懒加载] ${scn.name} (${fmtSize(scn.size)}) → 编译剧本`);
      await loadScn(bytes, false);
      // 填充场景图(章节地图)供原作式导航
      await ensureSceneChart();
    } catch (e) {
      log(`[错误] 剧本加载失败: ${e?.message || e}`);
    }
  } else {
    log("[提示] 归档中没有 .scn 剧本文件");
  }
  for (const f of files.filter((x) => isImageName(x.name)).slice(0, 2)) {
    try {
      const bytes = await readImage(source, f.name);
      const kind = isCharaName(f.name) ? "chara" : "bg";
      log(`[懒加载] ${f.name} → ${kind === "chara" ? "角色" : "背景"}`);
      showBytes(kind, bytes);
    } catch { /* 单图失败忽略 */ }
  }
}

async function unpackEntry(source, name) {
  if (name.endsWith(".scn")) {
    const bytes = await readEntry(source, name);
    log(`[剧本] ${name} → 编译`);
    scnName = name;
    await loadScn(bytes);
  } else if (name.endsWith(".pimg")) {
    // 后端 /img 已把 pimg 解码为 PNG;本地走 wasm 渲染
    const bytes = source.kind === "backend"
      ? await readImage(source, name)
      : render_pimg_png(xp3_read(source.bytes, name));
    log(`[贴图] ${name} → 渲染背景`);
    showBytes("bg", bytes);
  } else if (isImageName(name)) {
    const bytes = await readImage(source, name);
    const kind = isCharaName(name) ? "chara" : "bg";
    log(`[图片] ${name} → ${kind === "chara" ? "角色" : "背景"}`);
    showBytes(kind, bytes);
  } else if (/\.(txt|json|ks|tjs|tjs2|csv|ini)$/i.test(name)) {
    const bytes = await readEntry(source, name);
    log(`[文本] ${name}:`);
    log(new TextDecoder().decode(bytes));
  } else {
    const bytes = await readEntry(source, name);
    downloadBytes(name, bytes);
  }
}

// ---------------- 剧本 ----------------
async function loadScn(bytes, autoStart = true) {
  currentScnBytes = bytes;
  const compiled = JSON.parse(compile_scn(bytes));
  scenes = compiled.scenes;
  scnName = compiled.name || scnName;

  const sel = $("scene-select");
  sel.textContent = "";
  for (const s of scenes) {
    const opt = document.createElement("option");
    opt.value = s.label;
    opt.textContent = s.label;
    sel.appendChild(opt);
  }
  const first = scenes.find((s) => s.steps.some((st) => st.type === "dialogue"));
  if (first) sel.value = first.label;
  updateSceneInfo();
  log(`剧本 ${scnName}:${scenes.length} 个场景`);
  rememberLastPlayed();
  // 流式加载:后台预取本剧本引用到的背景/立绘/BGM,转场零等待
  prefetchSceneAssets(scenes);
  // 从场景 0(游戏入口)开始,stub 场景自动经跳转流入首个台词场景
  if (autoStart) playScene(scenes[0]?.label || sel.value);
}

/// 记录"上次播放"引用,供下次启动预热。
function rememberLastPlayed() {
  if (currentSource?.kind !== "backend" || !scnName) return;
  const path = /^scn[\\/]/i.test(scnName) ? scnName : `scn\\${scnName}.scn`;
  try { localStorage.setItem(LAST_KEY, JSON.stringify({ source: currentSource.name, scn: path })); } catch {}
}

/// 启动预热:把上次播放的剧本 + 场景图从持久缓存拉进内存热层(零网络)。
async function warmupStartup() {
  try {
    let last = null;
    try { last = JSON.parse(localStorage.getItem(LAST_KEY) || "null"); } catch {}
    if (!last || !last.source || !last.scn) return 0;
    const keys = [
      cacheKey({ kind: "backend", name: last.source }, last.scn),
      cacheKey({ kind: "backend", name: "data.xp3" }, "main\\scnchartdata.tjs"),
    ];
    const n = await cache.warmup(keys);
    if (n > 0) log(`[预热] 持久缓存恢复 ${n} 项: ${last.scn.replace(/^scn[\\/]/, "")}`);
    return n;
  } catch { return 0; }
}

function updateSceneInfo() {
  const label = $("scene-select").value;
  const s = scenes.find((x) => x.label === label);
  const dialogues = s ? s.steps.filter((st) => st.type === "dialogue").length : 0;
  $("scene-info").textContent = s
    ? `${s.steps.length} 步 · 其中台词 ${dialogues} 句`
    : "";
}

let runId = 0;       // 场景切换代际
let currentScnBytes = null; // 当前剧本字节(供引擎执行)
let kagEffects = []; // 引擎效果流(kag_run_scene)
let kagIndex = 0;
let choosing = false; // 选择等待态:选项展示期间 advanceEngine 不跟随跳转

let currentSceneLabel = null;
async function playScene(label) {
  currentSceneLabel = label;
  const s = scenes.find((x) => x.label === label);
  if (!s) return;
  if (/_(sel|select)$/i.test(label)) { showChoices(s); return; }
  // 进入正常对话:清掉旧选项(否则点击选项后旧按钮叠在对话上)
  $("choices").textContent = "";
  $("dialogue-box").hidden = false;
  $("stage-empty").hidden = true;
  if (!currentScnBytes) { log("[错误] 剧本字节缺失"); return; }
  // 引擎执行场景 → 效果流(引擎驱动渲染,非播放器步骤)
  const j = JSON.parse(kag_run_scene(currentScnBytes, label));
  kagEffects = j.effects;
  kagIndex = 0;
  runId++;
  log(`▶ 引擎执行 ${label} (${kagEffects.length} 效果)`);
  advanceEngine();
}

function advanceEngine() {
  const id = runId;
  while (kagIndex < kagEffects.length) {
    const e = kagEffects[kagIndex++];
    applyEffect(e);
    if (id !== runId) return; // 场景被跳转/切换接管
    if (e.type === "dialogue") {
      if (autoMode || skipMode) { scheduleNext(); return; }
      $("hint").textContent = "点击舞台 / 空格 推进";
      return;
    }
  }
  // 场景结束 → 跟随场景跳转(跨文件导航)
  if (choosing) return; // 等待选择:不跟随 sel 场景自身的跳转
  if (currentSceneLabel) {
    const sc = scenes.find((x) => x.label === currentSceneLabel);
    const jump = sc && sc.steps.find((st) => st.type === "jump" && (st.target || st.storage));
    if (jump) { handleJump(jump.storage, jump.target); return; }
  }
  stopAuto();
  $("dialogue-text").textContent = "—— 场景结束 ——";
  $("speaker").textContent = "";
  $("hint").textContent = "选择场景后可重播";
}

async function applyEffect(e) {
  try {
    switch (e.type) {
      case "dialogue":
        $("speaker").textContent = e.character || "";
        $("dialogue-text").textContent = e.text;
        backlog.push({ who: e.character || "…", text: e.text, scene: currentSceneLabel, ts: Date.now() });
        if (backlog.length > 500) backlog.shift();
        if (e.voice) playVoice(e.voice);
        break;
      case "layer":
        // 图层资源取 e.file(图像名,如 画面_黒/空_青空);回退到 e.name(角色引用)
        {
          const asset = e.file || e.name;
          log(`[图层] ${e.layer} ${asset}${e.face ? " 表情" + e.face : ""}`);
          if (e.layer === "Character") await showChara(asset.replace(/\.stand$/, ""), e.face);
          else if (e.layer === "Stage" || e.layer === "Stage2" || e.layer === "Event") await showBackground(asset);
        }
        break;
      case "audio":
        if (e.kind === "bgm") playBgm(e.name);
        else if (e.kind === "voice") playVoice(e.name);
        else log(`[音频] ${e.kind} ${e.name}`);
        break;
      case "chapter":
        log(`[章节] ${e.title}`);
        break;
      case "wait":
        break;
    }
  } catch (err) {
    // 单效果失败不中断播放流(资源缺失等)
    log(`[效果忽略] ${e.type}: ${err?.message || err}`);
  }
}

// ---------------- 场景跳转 / 跨文件导航 ----------------
async function handleJump(storage, target) {
  if (target === "*gameend_title" || (!storage && !target)) {
    // 游戏终点(回标题 / 场景流结束)
    stopAuto();
    log("[终点] 返回标题");
    $("dialogue-text").textContent = "—— 本章结束 ——";
    $("hint").textContent = "选择场景 / 归档继续";
    return;
  }
  const storageBase = storage ? storage.replace(/\.scn$/i, "") : "";
  if (storageBase && storageBase !== scnName) {
    // 跨文件:懒加载目标剧本(同一归档内)
    log(`[跳转] → 加载 ${storage}`);
    const loaded = await loadSceneFile(storageBase);
    if (!loaded) return;
    if (target) {
      const s = scenes.find((x) => x.label === target);
      if (s) playScene(s.label);
    } else {
      // storage-only(无 target):进新文件首个有台词的场景
      const s = loaded.find((x) => x.steps.some((st) => st.type === "dialogue")) || loaded[0];
      if (s) playScene(s.label);
    }
    return;
  }
  log(`[跳转] → ${target}`);
  const s = scenes.find((x) => x.label === target);
  if (s) playScene(s.label);
}

async function loadSceneFile(base) {
  const source = currentSource;
  if (!source) return null;
  // 归档内路径形如 `scn\414レナ・イザナウ.ks.scn`(base 可能已含 .ks)
  const cand = base.endsWith(".ks")
    ? [`scn\\${base}.scn`, `scn\\${base}`]
    : [`scn\\${base}.ks.scn`, `scn\\${base}.scn`];
  let bytes;
  for (const p of cand) {
    try {
      bytes = await readEntry(source, p);
      break;
    } catch { /* 试下一种路径 */ }
  }
  if (!bytes) {
    log(`[错误] 无法加载剧本 ${base}`);
    return null;
  }
  await loadScn(bytes, false);
  return scenes;
}

// ---------------- 选择分支 ----------------
// 场景图(scnchartdata.tjs,场景编码文本)提供真实选择目标:
//   "s001*com_part_1_sel" => (const) [ "s001*001_01A", "s001*001_01B" ]
// 播放器解析后,到达 `*_sel` 场景时按场景图展示分支,回退到启发式。
const sceneChart = {};   // "s001*label_sel" → ["s001*target", ...]
const flowMap = {};      // "s001*label" → "s001*label" (线性)
const chapters = [];     // [{ id, chapter, title, entry }]
let sceneChartReady = false;

/// 解析 scnchartdata:选择点(sel)+ 线性流(flow)+ 章节标题(captions)
function parseSceneChart(text) {
  const sel = {}, flow = {}, caps = [];
  const lines = text.split("\n");
  let arrKey = null, arrTargets = [];
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const openArr = line.match(/^\s*"([^"]+)"\s*=>\s*\(const\)\s*\[\s*$/);
    if (openArr) { arrKey = openArr[1]; arrTargets = []; continue; }
    if (arrKey) {
      const t = line.match(/^\s*"([^"]+)",?\s*$/);
      if (t) { arrTargets.push(t[1]); continue; }
      if (line.includes("]")) {
        if (/^s\d+[a-z]?$/.test(arrKey) && arrTargets.length) {
          caps.push({ id: arrKey, chapter: arrTargets[0] || "", title: arrTargets[1] || arrTargets[0] || "" });
        } else if (arrKey.includes("_sel") && !arrKey.includes(":")) {
          sel[arrKey] = arrTargets;
        }
        arrKey = null; continue;
      }
      continue;
    }
    const m = line.match(/^\s*"([^"]+)"\s*=>\s*"([^"]+)"/);
    if (m && !m[1].includes(":")) flow[m[1]] = m[2];
  }
  return { sel, flow, caps };
}

/// 从当前归档构建 s-ID → 文件名 映射
async function buildIdToFile() {
  const map = {};
  try {
    const files = await listEntries(currentSource);
    for (const f of files) {
      if (!f.name.endsWith(".scn")) continue;
      const base = f.name.replace(/.*\\/, "").replace(/\.scn$/i, "");
      const m = base.match(/^([0-9]+[a-z]?)/);
      if (m) map["s" + m[1]] = base;
    }
  } catch { /* 忽略 */ }
  return map;
}
let idToFile = null;

async function ensureSceneChart() {
  if (sceneChartReady || currentSource?.kind !== "backend") return;
  sceneChartReady = true;
  try {
    const raw = await readEntry(currentSource, "main\\scnchartdata.tjs");
    const { sel, flow, caps } = parseSceneChart(decode_scenario(raw));
    Object.assign(sceneChart, sel);
    Object.assign(flowMap, flow);
    chapters.length = 0; chapters.push(...caps);
    idToFile = await buildIdToFile();
    // 每章节入口场景 = 该文件首个 flow key
    for (const ch of chapters) {
      const keys = Object.keys(flowMap).filter((k) => k.startsWith(ch.id + "*"));
      // 入口优先 `com_part_N`(共通部分入口),否则取首个
      const entryKey = keys.find((k) => k.includes("com_part_1")) || keys[0];
      ch.entry = entryKey ? "*" + entryKey.split("*")[1] : null;
    }
    log(`场景图就绪: ${Object.keys(sceneChart).length} 选择点, ${chapters.length} 章节`);
    populateChapters();
  } catch (e) {
    log(`[提示] 场景图加载失败(回退启发式): ${e}`);
  }
}

/// 填充章节地图下拉
function populateChapters() {
  const sel = $("chapter-select");
  if (!sel) return;
  sel.textContent = "";
  for (const ch of chapters) {
    const opt = document.createElement("option");
    opt.value = ch.id;
    opt.textContent = `${ch.id.slice(1)} · ${ch.chapter} ${ch.title}`.trim();
    sel.appendChild(opt);
  }
  if (chapters.length) {
    $("chapter-info").textContent = "选择章节/路线,从该处开始播放(原作场景图)";
    sel.onchange = async () => await playChapter(sel.value);
  }
}

/// 跳到指定章节:加载该文件 → 播放入口场景
async function playChapter(id) {
  await ensureSceneChart();
  const ch = chapters.find((x) => x.id === id);
  const file = idToFile?.[id];
  if (!ch || !file || !ch.entry) {
    log(`[错误] 章节 ${id} 入口未知`);
    return;
  }
  log(`▶ 场景图 → ${ch.chapter} ${ch.title} (${file})`);
  const loaded = await loadSceneFile(file);
  if (loaded) {
    const s = scenes.find((x) => x.label === ch.entry);
    if (s) playScene(s.label);
    else if (loaded[0]) playScene(loaded[0].label);
  }
}

async function findBranches(selScene) {
  await ensureSceneChart();
  const label = selScene.label;
  for (const [k, targets] of Object.entries(sceneChart)) {
    if (k.endsWith(label)) {
      // 场景引用分支(如 `s001*001_01A`)
      const found = targets
        .map((t) => scenes.find((s) => s.label === "*" + t.split("*")[1]))
        .filter(Boolean);
      if (found.length) return found;
      // 显示名分支(如 `【エピローグ】`)→ 跟随 sel 场景自身跳转(单一结局)
      const jump = selScene.steps.find((st) => st.type === "jump" && st.target);
      if (jump) {
        const s = scenes.find((x) => x.label === jump.target);
        if (s) return [s];
      }
      return [];
    }
  }
  // 回退启发式:取 sel 后含台词的分支场景(过滤汇合 com/common)
  const idx = scenes.indexOf(selScene);
  const branches = [];
  for (let i = idx + 1; i < scenes.length && branches.length < 4; i++) {
    const s = scenes[i];
    if (!s.steps.some((st) => st.type === "dialogue")) {
      if (branches.length > 0 && !/dummy/i.test(s.label)) break;
      continue;
    }
    if (/(com|common|合流)/i.test(s.label)) continue;
    branches.push(s);
  }
  return branches;
}

async function showChoices(selScene) {
  stopAuto();
  choosing = true; // 等待选择:期间 advanceEngine 不跟随 sel 的跳转
  const branches = await findBranches(selScene);
  $("speaker").textContent = "选择";
  const box = $("choices");
  box.textContent = "";
  if (!branches.length) {
    choosing = false;
    $("dialogue-text").textContent = "—— 选择分支数据未解出 ——";
    $("hint").textContent = "选择场景后可重播";
    return;
  }
  // 选择提示:优先用分支场景首个有台词的副标题
  $("dialogue-text").textContent = "选择接下来的行动:";
  $("hint").textContent = "点击选项继续";
  branches.forEach((b, i) => {
    const first = b.steps.find((st) => st.type === "dialogue");
    // 选项标签:分支首句台词(真实选项原文仅在未随游戏的 .ks 源码中)
    const label = (first ? first.text : b.label).replace(/^[「『]|[」』]$/g, "");
    const btn = document.createElement("button");
    btn.className = "choice-btn";
    btn.textContent = `${i + 1}. ${label.slice(0, 28)}`;
    btn.title = `跳转到 ${b.label}`;
    // 阻断冒泡到 .stage 的 manualAdvance;选择后清除等待态并跳转
    btn.addEventListener("click", (ev) => {
      ev.stopPropagation();
      ev.preventDefault();
      choosing = false;
      playScene(b.label);
    });
    box.appendChild(btn);
  });
}

// ---------------- 命令驱动换图 ----------------
// 剧本资源命令(`cg_` 背景/立绘,`bgm_` BGM)按名称懒加载对应资产:
//   `cg_神社_神社内（刀）A` → bgimage1080 的 `神社_神社内（刀）A.png`
//   `cg_芦花.stand`        → fgimage1080 的 `芦花a_*.tlg`(整身立绘,取最大变体)
const IMAGE_ARCHIVES = ["bgimage1080.xp3", "fgimage1080.xp3", "patch_data1080.xp3"];
const assetIndex = new Map(); // 名称(去扩展名,小写) → { archive, path, size }
let assetIndexReady = false;

async function ensureAssetIndex() {
  if (assetIndexReady || currentSource?.kind !== "backend") return;
  assetIndexReady = true;
  const names = new Set([...IMAGE_ARCHIVES, currentSource.name]);
  for (const arc of names) {
    try {
      const { files } = await apiJson(`/api/archives/${encodeURIComponent(arc)}/files`);
      for (const f of files) {
        const key = f.name.replace(/\.(png|webp|jpg|jpeg|gif|bmp|tlg)$/i, "").toLowerCase();
        if (!assetIndex.has(key)) assetIndex.set(key, { archive: arc, path: f.name, size: f.size });
      }
    } catch {
      log(`[提示] 归档 ${arc} 不可用,跳过索引`);
    }
  }
  log(`资源索引就绪: ${assetIndex.size} 个资产`);
}

async function lookupAsset(asset) {
  await ensureAssetIndex();
  return assetIndex.get(asset.toLowerCase()) || null;
}

async function handleCommand(value) {
  if (value.startsWith("bgm_")) {
    log(`[BGM] ${value.slice(4)}`);
    return;
  }
  if (value.startsWith("cg_")) {
    const asset = value.slice(3);
    if (asset.endsWith(".stand")) {
      log(`[立绘] ${asset}`);
      await showChara(asset.slice(0, -6));
    } else {
      log(`[背景] ${asset}`);
      await showBackground(asset);
    }
    return;
  }
  log(`[命令] ${value}`);
}

async function showBackground(asset) {
  const hit = await lookupAsset(asset);
  if (!hit) {
    log(`  ↳ 未找到背景 ${asset}`);
    return;
  }
  try {
    const bytes = await readImage({ kind: "backend", name: hit.archive }, hit.path);
    showBytes("bg", bytes);
  } catch (e) {
    log(`  ↳ 背景 ${asset} 读取失败: ${e?.message || e}`);
  }
}

async function showChara(character, faceIndex) {
  // 立绘合成(对齐 APK):解析合成表,把 衣服层+腕差分+表情脸+附属 叠到画布。
  // faceIndex = stand options.face(如 "13"=不満げ),经 info.txt 映射到表情名。
  try {
    const png = await compositeStand(character, faceIndex);
    showBytes("chara", png);
    return;
  } catch (e) {
    log(`  ↳ ${character} 合成失败(回退启发式): ${e?.message || e}`);
  }
  // 回退:取 fgimage 中 `{character}a*.tlg` 的最大变体(无合成表时)
  await ensureAssetIndex();
  const prefix = character.toLowerCase() + "a";
  let best = null;
  for (const [key, v] of assetIndex) {
    if (key.startsWith(prefix) && key !== prefix && !key.includes("_0_")) {
      if (!best || v.size > best.size) best = v;
    }
  }
  if (!best) {
    log(`  ↳ 未找到 ${character} 立绘`);
    return;
  }
  try {
    const bytes = await readImage({ kind: "backend", name: best.archive }, best.path);
    showBytes("chara", bytes);
  } catch (err) {
    log(`  ↳ ${character} 立绘读取失败: ${err?.message || err}`);
  }
}

// ---------------- 立绘合成(stand,对齐 APK) ----------------
// APK 做法(KiriKiri stand):身体集(`芦花a`)+ 合成表(`芦花a.txt`,TSV)
// 把多个图层按 (left,top) 叠到 3600×5100 画布:
//   身体层(穿衣服):私服+腕差分(visible=1) → 替换时换裸/下着/甘味処服…
//   表情层(group 679):ベース=275 笑顔1=300 微笑み=326 悲しい=437 … 按表情选一个
//   附属:頬=689(腮红) 髪かぶせ=678(头发盖,非默认)
// 我们的播放器此前只挑了最大单层图(如 芦花a_710=甘味処服腕差分),故无表情组合。
const standTables = new Map();        // 身体集名 → 合成表行
const standCompositeCache = new Map(); // "角色|表情" → PNG 字节

async function getStandTable(char, bodySet) {
  const key = bodySet;
  if (standTables.has(key)) return standTables.get(key);
  const bytes = await readEntry({ kind: "backend", name: "fgimage1080.xp3" }, `${bodySet}.txt`);
  let text = null;
  try { text = decode_scenario(bytes); } catch {}
  if (!text) text = new TextDecoder("utf-8").decode(bytes);
  const rows = [];
  const lines = text.split("\n");
  const header = lines[0].split("\t").map((h) => h.trim().replace(/^#/, ""));
  const col = (name) => header.indexOf(name);
  const ci = { lt: col("layer_type"), nm: col("name"), l: col("left"), t: col("top"),
               w: col("width"), h: col("height"), ty: col("type"), op: col("opacity"),
               v: col("visible"), id: col("layer_id"), grp: col("group_layer_id") };
  for (let i = 1; i < lines.length; i++) {
    const p = lines[i].split("\t");
    if (p.length < 3) continue;
    rows.push({
      name: (p[ci.nm] || "").trim(),
      left: parseInt(p[ci.l]) || 0,
      top: parseInt(p[ci.t]) || 0,
      layer_type: parseInt(p[ci.lt]) || 0,   // 0=图像行,2=组标记
      visible: parseInt(p[ci.v]) === 1,
      layer_id: parseInt(p[ci.id]),
      group: parseInt(p[ci.grp]) || 0,
    });
  }
  const table = { bodySet, rows };
  standTables.set(key, table);
  return table;
}

/// 选层:可见的身体层(layer_type=0, visible, 非表情组)+ 指定的表情行。
function selectStandLayers(table, faceRow) {
  const body = table.rows.filter((r) => r.layer_type === 0 && r.visible && r.group === 0);
  return faceRow ? [...body, faceRow] : body;
}

/// 表情索引 → 表情名:解析 `芦花a_info.txt`(`face\t13\tbase\t表情/不満げ`)。
const faceIndexMaps = new Map(); // 身体集名 → { 索引: 表情名 }
async function getFaceIndexMap(bodySet) {
  if (faceIndexMaps.has(bodySet)) return faceIndexMaps.get(bodySet);
  const map = {};
  try {
    // info.txt 在 data.xp3 的 fgimage\ 下(如 fgimage\芦花a_info.txt)
    const bytes = await readEntry({ kind: "backend", name: "data.xp3" }, `fgimage\\${bodySet}_info.txt`);
    let text = null;
    try { text = decode_scenario(bytes); } catch {}
    if (!text) text = new TextDecoder("utf-8").decode(bytes);
    for (const line of text.split("\n")) {
      const p = line.split("\t");
      if (p[0] === "face" && p[1] && p[3] && p[3].startsWith("表情")) {
        map[p[1]] = p[3].split("/").pop().trim(); // "表情/不満げ" → "不満げ"(去 \r)
      }
    }
  } catch {}
  faceIndexMaps.set(bodySet, map);
  return map;
}
async function resolveFaceName(bodySet, faceIndex) {
  if (!faceIndex) return "ベース";
  const map = await getFaceIndexMap(bodySet);
  return map[faceIndex] || "ベース";
}

/// 解码 fgimage 图层图像为可绘制 Image。
async function loadDecodedImage(path) {
  const bytes = await readImage({ kind: "backend", name: "fgimage1080.xp3" }, path);
  const url = URL.createObjectURL(new Blob([bytes], { type: "image/webp" }));
  try {
    const img = new Image();
    await new Promise((res, rej) => { img.onload = res; img.onerror = rej; img.src = url; });
    return img;
  } finally {
    URL.revokeObjectURL(url);
  }
}

function canvasToPngBytes(canvas) {
  return new Promise((resolve, reject) => {
    canvas.toBlob(async (b) => {
      try { resolve(new Uint8Array(await b.arrayBuffer())); } catch (e) { reject(e); }
    }, "image/png");
  });
}

/// 合成角色立绘 → PNG 字节(按角色+表情索引缓存)。faceIndex 如 "13"。
async function compositeStand(char, faceIndex) {
  const cacheKey = `${char}|${faceIndex || "base"}`;
  const hit = standCompositeCache.get(cacheKey);
  if (hit) return hit;
  const bodySet = `${char}a`; // 身体集 = 角色名+a(芦花 → 芦花a)
  const table = await getStandTable(char, bodySet);
  const faceName = await resolveFaceName(bodySet, faceIndex);
  const faceRow = table.rows.find((r) => r.name === faceName)
    || table.rows.find((r) => r.group === 679 && r.name === "ベース");
  const layers = selectStandLayers(table, faceRow);
  const canvas = document.createElement("canvas");
  canvas.width = 3600;
  canvas.height = 5100;
  const ctx = canvas.getContext("2d");
  for (const layer of layers) {
    try {
      const img = await loadDecodedImage(`${bodySet}_${layer.layer_id}.tlg`);
      ctx.drawImage(img, layer.left, layer.top);
    } catch { /* 单层失败忽略 */ }
  }
  const png = await canvasToPngBytes(canvas);
  standCompositeCache.set(cacheKey, png);
  log(`[立绘合成] ${char} 表情${faceIndex || "ベース"}(${faceName}) → ${layers.length} 层`);
  return png;
}

// ---------------- 流式预取 ----------------
/// 剧本编译后后台预取:cg_ 背景/立绘 + bgm_ → 灌入缓存,转场即时显示。
async function prefetchSceneAssets(scn) {
  if (currentSource?.kind !== "backend") return;
  try {
    await ensureAssetIndex();
    const jobs = [];
    for (const s of scn) {
      for (const st of s.steps) {
        if (st.type !== "command" || typeof st.value !== "string") continue;
        const v = st.value;
        if (v.startsWith("bgm_")) jobs.push(prefetchBgm(v.slice(4)));
        else if (v.startsWith("cg_")) {
          const asset = v.slice(3);
          jobs.push(asset.endsWith(".stand") ? prefetchChara(asset.slice(0, -6)) : prefetchBg(asset));
        }
      }
    }
    await Promise.allSettled(jobs);
    const s = await cache.stats();
    log(`[预取] ${scn.length} 场景 → ${jobs.length} 资源入缓存 (命中 ${s.hits}, 持久 ${s.persistHits})`);
  } catch { /* 预取失败不影响播放 */ }
}

async function prefetchBg(asset) {
  const hit = await lookupAsset(asset);
  if (!hit) return;
  await readImage({ kind: "backend", name: hit.archive }, hit.path);
}
async function prefetchChara(character) {
  await ensureAssetIndex();
  const prefix = character.toLowerCase() + "a";
  let best = null;
  for (const [key, v] of assetIndex) {
    if (key.startsWith(prefix) && key !== prefix && !key.includes("_0_")) {
      if (!best || v.size > best.size) best = v;
    }
  }
  if (best) await readImage({ kind: "backend", name: best.archive }, best.path);
}
async function prefetchBgm(name) {
  for (const path of [`${name}.opus`, `${name}.ogg`, `${name}.mp3`]) {
    try { await readEntry({ kind: "backend", name: "bgm.xp3" }, path); return; } catch { /* 试下一种 */ }
  }
}

// ---------------- 语音 ----------------
// 台词语音引用(如 `uts001_001`)经后端从 voice.xp3 懒加载播放。
/// 播放 BGM(bgm.xp3 懒加载 + 缓存,循环)。
function playBgm(name) {
  const audio = $("bgm");
  const cand = [`${name}.opus`, `${name}.ogg`, `${name}.mp3`, `${name}`];
  (async () => {
    for (const path of cand) {
      try {
        const bytes = await readEntry({ kind: "backend", name: "bgm.xp3" }, path);
        audio.src = URL.createObjectURL(new Blob([bytes], { type: "audio/ogg" }));
        audio.volume = settings.bgm;
        audio.play().catch(() => {});
        log(`[BGM] ${path}`);
        return;
      } catch { /* 试下一种 */ }
    }
    log(`[BGM] 未找到 ${name}`);
  })();
}

async function playVoice(ref) {
  if (!ref || currentSource?.kind !== "backend") return;
  // 空值防护:某些缓存/旧页可能缺失 #voice-ind
  const ind = $("voice-ind");
  const candidates = [`${ref}.ogg`, `${ref}.wav`, ref];
  for (const path of candidates) {
    try {
      const bytes = await readEntry({ kind: "backend", name: "voice.xp3" }, path);
      const audio = $("voice");
      audio.src = URL.createObjectURL(new Blob([bytes]));
      lastVoiceSrc = audio.src;
      audio.volume = settings.voice;
      audio.play().catch(() => {});
      if (ind) ind.hidden = false;
      log(`[语音] ${path}`);
      return;
    } catch { /* 尝试下一种命名 */ }
  }
  if (ind) ind.hidden = true;
  log(`[语音] 未找到 ${ref}`);
}

// ---------------- 图像 ----------------
function isImageName(name) {
  return /\.(png|webp|jpg|jpeg|gif|bmp|tlg)$/i.test(name);
}
function isCharaName(name) {
  return /(chara|face|立绘|fg|mizuha|みづは)/i.test(name.toLowerCase());
}
function isTlgBytes(b) {
  return b[0] === 0x54 && b[1] === 0x4c && b[2] === 0x47; // "TLG"
}
function showBytes(kind, bytes) {
  const img = kind === "chara" ? $("character") : $("background");
  img.src = URL.createObjectURL(new Blob([bytes], { type: "application/octet-stream" }));
  img.hidden = false;
  $("stage-empty").hidden = true;
}

function fmtSize(n) {
  if (n >= 1e6) return (n / 1e6).toFixed(1) + " MB";
  if (n >= 1e3) return (n / 1e3).toFixed(1) + " KB";
  return n + " B";
}

async function fetchBytes(path) {
  const res = await fetch(path);
  if (!res.ok) throw new Error(`加载失败 ${path}: HTTP ${res.status}`);
  return new Uint8Array(await res.arrayBuffer());
}

// ---------------- 播放控制:自动 / 跳过 ----------------
function stopAuto() {
  autoMode = false;
  skipMode = false;
  clearTimeout(autoTimer);
  $("ctl-auto").classList.remove("active");
  $("ctl-skip").classList.remove("active");
}

/// 手动推进:自动/跳过开启时,单击仅停掉模式(标准 VN 行为),不推进。
function manualAdvance() {
  if (autoMode || skipMode) { stopAuto(); return; }
  advanceEngine();
}

function scheduleNext() {
  clearTimeout(autoTimer);
  const delay = skipMode ? 60 : Math.round(settings.speed * 700);
  autoTimer = setTimeout(() => { if (autoMode || skipMode) advanceEngine(); }, delay);
}

// ---------------- 设置 ----------------
function bindSettings() {
  const bgm = $("set-bgm"), voice = $("set-voice"), speed = $("set-speed");
  bgm.value = settings.bgm * 100;
  voice.value = settings.voice * 100;
  speed.value = String(settings.speed);
  const apply = () => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    $("bgm").volume = settings.bgm;
    $("voice").volume = settings.voice;
  };
  bgm.addEventListener("input", () => { settings.bgm = bgm.value / 100; apply(); });
  voice.addEventListener("input", () => { settings.voice = voice.value / 100; apply(); });
  speed.addEventListener("change", () => { settings.speed = parseFloat(speed.value); apply(); });
  apply();
}

// ---------------- 回看历史 ----------------
function openBacklog() {
  const list = $("backlog-list");
  list.textContent = "";
  if (!backlog.length) {
    list.textContent = "(暂无对话历史)";
  }
  for (const b of backlog.slice().reverse()) {
    const row = document.createElement("div");
    row.className = "bl-row";
    const who = document.createElement("div");
    who.className = "bl-who";
    who.textContent = (b.who || "…") + (b.scene ? ` · ${b.scene}` : "");
    const txt = document.createElement("div");
    txt.className = "bl-text";
    txt.textContent = b.text;
    row.append(who, txt);
    list.appendChild(row);
  }
  $("backlog-modal").hidden = false;
  list.scrollTop = list.scrollHeight;
}

// ---------------- 存档 / 读档 ----------------
function openSaveModal(isSave) {
  $("save-title").textContent = isSave ? "存档" : "读档";
  const slots = $("save-slots");
  slots.textContent = "";
  const all = JSON.parse(localStorage.getItem(SAVE_KEY) || "[]");
  for (let i = 0; i < 5; i++) {
    const s = all[i];
    const btn = document.createElement("button");
    btn.className = "save-slot";
    btn.textContent = s
      ? `槽 ${i + 1} · ${new Date(s.ts).toLocaleString()} · ${(s.scn || "").replace(/.*[\\/]/, "").slice(0, 20)} @ ${s.label}`
      : `槽 ${i + 1} · 空`;
    btn.title = s ? `${s.scn} → ${s.label}` : "尚未存档";
    btn.addEventListener("click", async () => {
      if (isSave) {
        await saveSlot(i);
        openSaveModal(true);
      } else {
        $("save-modal").hidden = true;
        await loadSlot(i);
      }
    });
    slots.appendChild(btn);
  }
  $("save-modal").hidden = false;
}

async function saveSlot(i) {
  if (!currentSceneLabel || !currentSource) { log("[存档] 尚无进行中的剧情"); return; }
  const all = JSON.parse(localStorage.getItem(SAVE_KEY) || "[]");
  all[i] = {
    label: currentSceneLabel,
    scn: scnName,
    source: currentSource.kind === "backend" ? currentSource.name : null,
    ts: Date.now(),
    bgm: $("bgm").src,
  };
  localStorage.setItem(SAVE_KEY, JSON.stringify(all));
  log(`[存档] 槽 ${i + 1} → ${currentSceneLabel}`);
}

async function loadSlot(i) {
  const all = JSON.parse(localStorage.getItem(SAVE_KEY) || "[]");
  const s = all[i];
  if (!s || !s.source) { log("[读档] 该槽为空"); return; }
  stopAuto();
  log(`[读档] 槽 ${i + 1} → ${s.scn} @ ${s.label}`);
  await openSource({ kind: "backend", name: s.source });
  const base = (s.scn || "").replace(/^scn[\\/]/i, "").replace(/\.scn$/i, "");
  const loaded = await loadSceneFile(base);
  if (loaded) {
    const target = scenes.find((x) => x.label === s.label)
      || loaded.find((x) => x.steps.some((st) => st.type === "dialogue"));
    if (target) playScene(target.label);
  }
}

main();
