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
let currentSource = null; // { kind:'backend', name, overlay? } | { kind:'local', bytes }
// 归档叠加:KiriKiri 引擎多归档按文件名(忽略目录)查找,后挂载优先。
// 汉化补丁(patch.xp3)覆盖原版(data.xp3):剧本/脚本/SE 从 patch 取,其余回退原归档。
const OVERLAY_PRIORITY = ["patch.xp3", "patch_data1080.xp3", "data.xp3", "data1080.xp3"];

// 流式加载缓存:剧本/图像/音频 字节一次拉取,重放与跳回零网络
const cache = new AssetCache();
// 缓存键含 overlay 签名:启用汉化叠加后路径解析不同,避免命中旧(日文)缓存
const cacheKey = (source, path, tag = "") =>
  `${source.kind}:${source.name}:${(source.overlays || []).join(",")}:${path}${tag}`;

// ---------------- 设置 / 存档 / 历史 ----------------
const SETTINGS_KEY = "yuzu-settings-v1";
const SAVE_KEY = "yuzu-save-v1";
const LAST_KEY = "yuzu-lastplayed-v1";
const DEFAULT_SETTINGS = { bgm: 0.8, voice: 1.0, speed: 0.8, lang: "zh" };
let settings = { ...DEFAULT_SETTINGS };
try { settings = Object.assign(settings, JSON.parse(localStorage.getItem(SETTINGS_KEY) || "{}")); } catch {}
let autoMode = false, skipMode = false, autoTimer = null;
const backlog = [];       // [{ who, text, scene, file, effectIndex, ts }]
let lastVoiceSrc = null;  // 最近语音(点击 🔊 重放)
let currentBgmName = null;   // 当前播放 BGM 名称(读档恢复用)
let currentScnBase = null;   // 当前剧本 base(不含 .scn / scn\ 前缀)
let replaying = false;       // 回看点击回跳:重放期间不重复记入 backlog
let lastDialogueLen = 0;     // 最近一句台词长度(自动推进间隔随文本长度自适应)
let saveModalMode = null;    // 当前存档弹窗模式(true=存档 / false=读档 / null=未打开),供切语言时重渲染

// ---------------- 界面语言(zh / en) ----------------
// 说明:游戏台词来自数据包(无法即时翻译),此处负责界面文案。{1}/{2} 为占位符。
const I18N = {
  zh: {
    open: "打开 .xp3 文件", sub: "WASM 引擎 · 完全复刻", newgame: "开始游戏", chart: "场景图", extra: "Extra(后日谈)",
    auto: "自动", skip: "跳过", backlog: "回看", save: "存档", load: "读档", title: "标题",
    loading: "加载演示数据…", hintAdvance: "点击舞台 / 空格 推进",
    h_chapter: "场景图", h_scene: "场景", h_settings: "设置", h_archive: "解包(XP3)", h_log: "引擎日志",
    play: "▶ 播放",
    badge_flow: "原作流程", badge_backend: "后端懒加载",
    set_lang: "语言", set_bgm: "BGM 音量", set_voice: "语音音量", set_speed: "自动间隔",
    speed_fast: "快", speed_normal: "普通", speed_slow: "慢",
    settings_persist: "设置自动保存到本地", reset: "重置设置为默认",
    placeholder: "— 选择归档 —", backlog_title: "对话历史", close: "关闭",
    hint_wait: "选择场景后可重播", hint_sel: "选择接下来的行动:", hint_choose: "点击选项继续",
    end_chapter: "—— 本章结束 ——", end_hint: "选择场景 / 归档继续",
    choose_lbl: "选择", scene_end: "—— 场景结束 ——", sel_none: "—— 选择分支数据未解出 ——",
    save_save: "存档", save_load: "读档",
    slot_fmt: "槽 {1} · {2} · {3} @ {4}", slot_empty: "槽 {1} · 空", slot_empty2: "槽 {1} · 空(无存档)", slot_none: "该槽尚无存档", step_tag: " · 步{1}",
    cap_fmt: "{1} 条 · 点击台词行可回到该句",
    scene_info: "{1} 步 · 其中台词 {2} 句", chapter_info: "选择章节/路线,从该处开始播放(原作场景图)",
    files_fmt: "{1} · {2} 个文件", local_file: "(本地文件)",
    cache_fmt: "缓存: {1} 内存命中 / {2} 持久命中 / {3} 拉取 · 省 {4} · {5} 项持久",
    backlog_empty: "(暂无对话历史)", backlog_goto: "点击回到此处继续",
    open_t: "选择本地的 .xp3 归档,在浏览器内解包并播放", stage_aria: "游戏舞台",
    auto_t: "自动播放", skip_t: "快进(不停留台词)", backlog_t: "查看对话历史",
    save_t: "保存进度", load_t: "读取存档", title_t: "回到标题",
    chapter_aria: "选择章节/路线", scene_aria: "选择场景", archive_aria: "选择归档",
  },
  en: {
    open: "Open .xp3", sub: "WASM Engine · Full Remake", newgame: "Start", chart: "Chart", extra: "Extra",
    auto: "Auto", skip: "Skip", backlog: "Log", save: "Save", load: "Load", title: "Title",
    loading: "Loading demo…", hintAdvance: "Click stage / Space to advance",
    h_chapter: "Chapters", h_scene: "Scenes", h_settings: "Settings", h_archive: "Archive (XP3)", h_log: "Engine Log",
    play: "▶ Play",
    badge_flow: "Original Flow", badge_backend: "Lazy Backend",
    set_lang: "Language", set_bgm: "BGM Volume", set_voice: "Voice Volume", set_speed: "Auto Interval",
    speed_fast: "Fast", speed_normal: "Normal", speed_slow: "Slow",
    settings_persist: "Settings auto-saved locally", reset: "Reset to Defaults",
    placeholder: "— Select archive —", backlog_title: "Dialogue History", close: "Close",
    hint_wait: "Pick a scene to replay", hint_sel: "Choose your action:", hint_choose: "Click an option",
    end_chapter: "—— End of chapter ——", end_hint: "Pick a scene / archive to continue",
    choose_lbl: "Choice", scene_end: "—— End of scene ——", sel_none: "—— No branch data ——",
    save_save: "Save", save_load: "Load",
    slot_fmt: "Slot {1} · {2} · {3} @ {4}", slot_empty: "Slot {1} · Empty", slot_empty2: "Slot {1} · Empty (none)", slot_none: "No save here", step_tag: " · step {1}",
    cap_fmt: "{1} lines · click a line to return",
    scene_info: "{1} steps · {2} dialogue lines", chapter_info: "Pick a chapter/route to start from (original flow)",
    files_fmt: "{1} · {2} files", local_file: "(local file)",
    cache_fmt: "Cache: {1} mem hits / {2} persist hits / {3} fetches · saved {4} · {5} persisted",
    backlog_empty: "(no history yet)", backlog_goto: "Click to resume here",
    open_t: "Open a local .xp3 archive to unpack and play in the browser", stage_aria: "Game stage",
    auto_t: "Auto-play", skip_t: "Fast-forward (don't pause on lines)", backlog_t: "View dialogue history",
    save_t: "Save progress", load_t: "Load save", title_t: "Back to title",
    chapter_aria: "Pick a chapter/route", scene_aria: "Pick a scene", archive_aria: "Pick an archive",
  },
};
let lang = (settings.lang === "en" || settings.lang === "zh") ? settings.lang : "zh";
// 取当前语言文本;{1}/{2} 依次替换
function t(k, ...args) {
  let s = (I18N[lang] && I18N[lang][k]) ?? I18N.en[k] ?? k;
  args.forEach((a, i) => { s = s.split(`{${i + 1}}`).join(String(a)); });
  return s;
}
/// 应用界面语言:刷新所有 [data-i18n]、语言下拉、以及受语言影响的动态文案。
function applyLang() {
  document.documentElement.lang = lang;
  for (const el of document.querySelectorAll("[data-i18n]")) {
    // 跳过含子 [data-i18n] 的容器(如 <h2>场景图 <span class="badge" data-i18n>…</span></h2>),
    // 只改叶子节点文本,避免 textContent 覆盖清掉内部徽章/图标
    if (el.querySelector("[data-i18n]")) continue;
    const v = t(el.getAttribute("data-i18n"));
    if (v != null) el.textContent = v;
    const tk = el.getAttribute("data-i18n-title");
    if (tk) el.title = t(tk);
    const ak = el.getAttribute("data-i18n-aria");
    if (ak) el.setAttribute("aria-label", t(ak));
  }
  const ls = $("set-lang");
  if (ls) ls.value = lang;
  const stat = $("cache-stats");
  if (stat) stat.textContent = cacheStatsLine();
  updateSceneInfo();
  const cap = $("backlog-cap");
  if (cap && backlog.length) cap.textContent = t("cap_fmt", backlog.length);
  if (!$("save-modal").hidden && saveModalMode != null) openSaveModal(saveModalMode);
}

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
    const merged = new Map(); // 文件名(小写)→ 条目
    const add = (entries) => { for (const f of entries) merged.set(f.name.toLowerCase(), f); };
    // 基础归档先入,汉化补丁(patch)后入 → 同名时补丁覆盖
    const d = await apiJson(`/api/archives/${encodeURIComponent(source.name)}/files`);
    add(d.files);
    for (const overlay of source.overlays || []) {
      try {
        const od = await apiJson(`/api/archives/${encodeURIComponent(overlay)}/files`);
        add(od.files);
      } catch { /* 归档不存在则跳过 */ }
    }
    return [...merged.values()];
  }
  return JSON.parse(xp3_info(source.bytes)).files;
}
/// 后端读一个条目(含叠加):先按 overlay 归档按文件名匹配,再回退基础归档。
/// KiriKiri 归档查找忽略目录(汉化补丁常把同名单条放根路径),mode 为 "file"|"img"。
const archiveFileCache = new Map(); // 归档名 → Promise<[{name,size}]>

/// 取归档条目清单(缓存):用于读取时的模糊回退。
async function archiveFilesOf(archive) {
  if (!archiveFileCache.has(archive)) {
    archiveFileCache.set(
      archive,
      apiJson(`/api/archives/${encodeURIComponent(archive)}/files`)
        .then((d) => d.files || [])
        .catch(() => [])
    );
  }
  return archiveFileCache.get(archive);
}

/// 在单归档内定位资源:精确(完整路径→文件名)→ 模糊。返回真实条目路径或 null。
async function resolveInArchive(archive, path, mode) {
  const enc = encodeURIComponent;
  const tryExact = async (p) => {
    try {
      return await apiBytes(`/api/archives/${enc(archive)}/${mode}?path=${enc(p)}`);
    } catch { return null; }
  };
  // 1) 完整路径 / 纯文件名精确
  const name = path.split(/[\\/]/).pop();
  for (const p of [path, name]) {
    const b = await tryExact(p);
    if (b) return b;
  }
  // 2) 模糊:在归档文件清单里按名称打分,取最优后读真实路径
  const files = await archiveFilesOf(archive);
  if (!files.length) return null;
  let best = null;
  for (const f of files) {
    const m = fuzzyScore(name, f.name);
    if (!m) continue;
    if (!best || m.score < best.m.score || (m.score === best.m.score && m.dist < best.m.dist)) {
      best = { f, m };
    }
  }
  if (!best) return null;
  const b = await tryExact(best.f.name);
  return b;
}

async function backendBytes(source, path, mode) {
  const name = path.split(/[\\/]/).pop(); // 纯文件名(忽略目录)
  for (const overlay of source.overlays || []) {
    const b = await resolveInArchive(overlay, name, mode);
    if (b) return b;
  }
  const base = await resolveInArchive(source.name, path, mode);
  if (base) return base;
  throw new Error(`归档 ${source.name} 内未找到 ${path}(含模糊匹配)`);
}

async function readEntry(source, path) {
  const key = cacheKey(source, path);
  const hit = await cache.getPersist(key);
  if (hit) { updateCacheStats(); return hit; }
  let bytes;
  if (source.kind === "backend") {
    bytes = await backendBytes(source, path, "file");
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
    bytes = await backendBytes(source, path, "img");
  } else {
    const raw = xp3_read(source.bytes, path);
    bytes = isTlgBytes(raw) ? decode_tlg_png(raw) : raw;
  }
  cache.set(key, bytes);
  updateCacheStats();
  return bytes;
}

let lastCacheStats = null; // 最近一次缓存统计快照(供 applyLang 同步取用,无需重复 await)

/// 本地化缓存统计行;无快照时返回占位。
function cacheStatsLine() {
  const s = lastCacheStats;
  if (!s) return t("cache_fmt", 0, 0, 0, "0 B", 0);
  return t("cache_fmt", s.hits, s.persistHits, s.misses, fmtSize(s.saved), s.persisted);
}

async function updateCacheStats() {
  const el = $("cache-stats");
  if (!el) return;
  lastCacheStats = await cache.stats();
  el.textContent = cacheStatsLine();
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
      const { archives } = await apiJson("/api/archives");
      const names = new Set(archives.map((a) => a.name));
      const overlays = OVERLAY_PRIORITY.filter((n) => n !== name && names.has(n));
      await openSource({ kind: "backend", name, overlays });
    } catch (e) {
      log("[错误] 打开归档失败:", e.message);
    }
  });

  // 交互:点击舞台 / 空格 / 回车 / → 推进;↑ 回看历史
  // 注意:控制栏按钮(自动/跳过/回看/存档…)嵌在 #stage 内,点击会冒泡到这里——必须忽略,
  // 否则点「自动」会被 manualAdvance 的 stopAuto 立刻关掉、点「存档」还会多推一步。
  $("stage").addEventListener("click", (e) => {
    if (e.target.closest?.(".controls")) return;
    manualAdvance();
  });
  $("stage").addEventListener("wheel", (e) => {
    if (e.target.closest?.(".controls")) return;
    e.preventDefault();
    manualAdvance();
  }, { passive: false });
  document.addEventListener("keydown", (e) => {
    if (["Space", "Enter", "ArrowRight", "ArrowDown"].includes(e.code)) {
      e.preventDefault();
      manualAdvance();
    } else if (e.code === "ArrowUp") {
      e.preventDefault();
      openBacklog();
    } else if (e.ctrlKey && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "s") {
      e.preventDefault();
      log("[快存] Ctrl+S → 槽 1");
      saveSlot(0);
    } else if (e.ctrlKey && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "l") {
      e.preventDefault();
      loadSlot(0);
    }
  });

  $("play-btn").addEventListener("click", () => {
    const label = $("scene-select").value;
    if (label) playScene(label);
  });

  // 归档文件模糊搜索:输入即过滤列表(忽略大小写/全半角/部分匹配)
  const filter = $("archive-filter");
  if (filter) {
    let timer = null;
    filter.addEventListener("input", () => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        archiveFilterText = filter.value;
        if (currentSource) renderArchiveFiles(currentSource);
      }, 120);
    });
  }

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
  // 🔊 重放最近语音(阻断冒泡到 .stage,避免重播语音时又推进一步)
  const voiceInd = $("voice-ind");
  if (voiceInd) {
    voiceInd.addEventListener("click", (ev) => {
      ev.stopPropagation();
      if (lastVoiceSrc) { const a = $("voice"); a.src = lastVoiceSrc; a.play().catch(() => {}); }
    });
  }
  bindSettings();
  applyLang(); // 启动即按已保存的语言刷新界面
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
  const names = new Set(archives.map((a) => a.name));
  // 汉化叠加:patch.xp3 覆盖 data.xp3(剧本/脚本/SE),仅挂载实际存在的归档
  const overlays = OVERLAY_PRIORITY.filter((n) => n !== pick.name && names.has(n));
  await openSource({ kind: "backend", name: pick.name, overlays });
}

// ---------------- 打开数据源 ----------------
let archiveAllFiles = [];   // 当前归档全量条目(供搜索过滤)
let archiveFilterText = ""; // 当前过滤关键词

/// 渲染文件列表(应用当前过滤)。
function renderArchiveFiles(source) {
  const ul = $("xp3-files");
  ul.textContent = "";
  const q = archiveFilterText.trim();
  let shown = 0;
  for (const f of archiveAllFiles) {
    if (q && fuzzyScore(q, f.name) === null && !normName(f.name).includes(normName(q))) continue;
    shown++;
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
  const cap = $("xp3-summary");
  if (cap) {
    cap.textContent = q
      ? `${archiveAllFiles.length} → ${shown}`
      : t("files_fmt", source.kind === "backend" ? source.name : t("local_file"), archiveAllFiles.length);
  }
}

async function openSource(source) {
  currentSource = source;
  archiveAllFiles = await listEntries(source);
  archiveFilterText = "";
  const filter = $("archive-filter");
  if (filter) filter.value = "";
  renderArchiveFiles(source);

  // 自动播放 .scn,展示前两个图片(单资源失败不中止整体打开)。
  // 注意:不自动开演 —— 游戏只从标题屏"开始游戏"进入(autoStart=false)。
  const scn = archiveAllFiles.find((f) => f.name.endsWith(".scn"));
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
  for (const f of archiveAllFiles.filter((x) => isImageName(x.name)).slice(0, 2)) {
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
  currentScnBase = (compiled.name || "").replace(/^scn[\\/]/i, "").replace(/\.scn$/i, "");

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
    ? t("scene_info", s.steps.length, dialogues)
    : "";
}

let runId = 0;       // 场景切换代际
let currentScnBytes = null; // 当前剧本字节(供引擎执行)
let kagEffects = []; // 引擎效果流(kag_run_scene)
let kagIndex = 0;
let choosing = false; // 选择等待态:选项展示期间 advanceEngine 不跟随跳转

let currentSceneLabel = null;
let currentDlgIdx = -1; // 当前场景内最近显示的台词 effect 索引(存档精确恢复用;-1=尚无台词)
async function playScene(label) {
  currentSceneLabel = label;
  currentDlgIdx = -1;
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
    applyEffect(e, kagIndex - 1); // 记录该效果在场景内的索引,供回看回跳
    if (id !== runId) return; // 场景被跳转/切换接管
    if (e.type === "dialogue") {
      if (autoMode || skipMode) { scheduleNext(); return; }
      $("hint").textContent = t("hintAdvance");
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
  $("dialogue-text").textContent = t("scene_end");
  $("speaker").textContent = "";
  $("hint").textContent = t("hint_wait");
}

async function applyEffect(e, effectIndex) {
  try {
    switch (e.type) {
      case "dialogue":
        // 说话人文本 + 语音指示(#voice-ind):textContent 赋值会清掉子节点,
        // 先摘出 voice-ind 再重设文本并放回,避免指示图标从 DOM 消失。
        {
          const sp = $("speaker");
          const vInd = sp.querySelector("#voice-ind");
          sp.textContent = e.character || "";
          if (vInd) sp.appendChild(vInd);
          else sp.insertAdjacentHTML("beforeend", `<span id="voice-ind" hidden>🔊</span>`);
        }
        $("dialogue-text").textContent = e.text;
        // 回看回跳时不重复记入 backlog(该句已存在)
        if (!replaying) {
          backlog.push({ who: e.character || "…", text: e.text, scene: currentSceneLabel, file: currentScnBase, effectIndex, ts: Date.now() });
          if (backlog.length > 500) backlog.shift();
        }
        lastDialogueLen = e.text ? e.text.length : 0;
        currentDlgIdx = effectIndex;   // 存档精确恢复:记住当前这句的位置
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
    $("dialogue-text").textContent = t("end_chapter");
    $("hint").textContent = t("end_hint");
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

/// 解码 KiriKiri 文本文件:scenario 编码(fe fe 头)走 wasm;UTF-16 LE(含 BOM fffe)直接解。
/// 汉化补丁(patch.xp3)里的 tjs/scn 常是已解码的 UTF-16 明文,需绕过 scenario 解码。
function decodeKiriText(bytes) {
  if (bytes.length >= 2 && bytes[0] === 0xfe && bytes[1] === 0xfe) {
    return decode_scenario(bytes);
  }
  if (bytes.length >= 2 && bytes[0] === 0xff && bytes[1] === 0xfe) {
    return new TextDecoder("utf-16le").decode(bytes);
  }
  return decode_scenario(bytes);
}

async function ensureSceneChart() {
  if (sceneChartReady || currentSource?.kind !== "backend") return;
  sceneChartReady = true;
  try {
    const raw = await readEntry(currentSource, "main\\scnchartdata.tjs");
    const { sel, flow, caps } = parseSceneChart(decodeKiriText(raw));
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
    $("chapter-info").textContent = t("chapter_info");
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
  $("speaker").textContent = t("choose_lbl");
  const box = $("choices");
  box.textContent = "";
  if (!branches.length) {
    choosing = false;
    $("dialogue-text").textContent = t("sel_none");
    $("hint").textContent = t("hint_wait");
    return;
  }
  // 选择提示:优先用分支场景首个有台词的副标题
  $("dialogue-text").textContent = t("hint_sel");
  $("hint").textContent = t("hint_choose");
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
      backlog.push({ who: t("choose_lbl"), text: btn.textContent.replace(/^\d+\.\s*/, ""), scene: currentSceneLabel, ts: Date.now() });
      if (backlog.length > 500) backlog.shift();
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

// ---------------- 资源名称模糊匹配 ----------------
// 剧本引用的资源名与归档文件名常有差异(变体后缀 `みづは`→`みづはa`、目录前缀、
// 大小写、全/半角空白)。匹配按优先级:完全相等 → 忽略扩展名/目录后相等 →
// 前缀(引用是文件名前缀) → 包含 → 编辑距离小。返回 { entry, score, dist }。
const normName = (s) =>
  (s || "")
    .replace(/\\/g, "/")
    .replace(/^.*\//, "")                    // 去目录
    .replace(/\.(png|webp|jpg|jpeg|gif|bmp|tlg|ogg|opus|wav|mp3|scn|ks)$/i, "") // 去扩展名
    .replace(/[\s　]/g, "")              // 去空白(含全角空格)
    .toLowerCase();

/// Levenshtein 编辑距离(限制 max,超限即返回 max+1,避免长串全量计算)。
function editDistance(a, b, max = 3) {
  if (a === b) return 0;
  if (!a.length) return b.length;
  if (!b.length) return a.length;
  if (Math.abs(a.length - b.length) > max) return max + 1;
  let prev = new Array(b.length + 1).fill(0).map((_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    const cur = [i];
    let rowMin = cur[0];
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + cost);
      if (cur[j] < rowMin) rowMin = cur[j];
    }
    if (rowMin > max) return max + 1; // 早停:当前行最小已超限
    prev = cur;
  }
  return prev[b.length];
}

/// 对资源名打分:score 越小越优;无匹配返回 null。
/// 0=完全相等,1=忽略目录/扩展名后相等,2=引用是文件名前缀,3=文件名含引用,4=编辑距离≤max。
function fuzzyScore(query, name) {
  if (!query || !name) return null;
  if (query === name) return { score: 0, dist: 0 };
  const q = normName(query), n = normName(name);
  if (q === n) return { score: 1, dist: 0 };
  if (n.startsWith(q)) return { score: 2, dist: 0 };
  if (n.includes(q) && q.length >= 2) return { score: 3, dist: 0 };
  const dist = editDistance(q, n);
  if (dist <= 2) return { score: 4, dist };
  return null;
}

/// 在 assetIndex 中模糊查找:先精确,再按 (score, 名称长度差) 取最优。
function fuzzyLookupAsset(query) {
  if (!query) return null;
  const exact = assetIndex.get(query.toLowerCase()) || assetIndex.get(normName(query));
  if (exact) return exact;
  let best = null;
  for (const [key, v] of assetIndex) {
    const m = fuzzyScore(query, key);
    if (!m) continue;
    const lenDiff = Math.abs(normName(query).length - normName(key).length);
    if (!best || m.score < best.m.score || (m.score === best.m.score && lenDiff < best.lenDiff)) {
      best = { v, m, lenDiff };
    }
  }
  return best ? best.v : null;
}

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
  return fuzzyLookupAsset(asset);
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
  try { text = decodeKiriText(bytes); } catch {}
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
    try { text = decodeKiriText(bytes); } catch {}
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
  currentBgmName = name;
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
    currentBgmName = null;
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
// 已显示图像的 blob URL 管理:每类只保留最近 3 个,旧的可回收(长流程不无限泄漏)
const _shownUrls = { bg: [], chara: [] };
function showBytes(kind, bytes) {
  const img = kind === "chara" ? $("character") : $("background");
  const url = URL.createObjectURL(new Blob([bytes], { type: "application/octet-stream" }));
  img.src = url;
  img.hidden = false;
  $("stage-empty").hidden = true;
  const arr = kind === "chara" ? "chara" : "bg";
  _shownUrls[arr].push(url);
  while (_shownUrls[arr].length > 3) {
    const old = _shownUrls[arr].shift();
    try { URL.revokeObjectURL(old); } catch { /* 忽略 */ }
  }
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
  if (skipMode) {
    autoTimer = setTimeout(() => { if (autoMode || skipMode) advanceEngine(); }, 60);
    return;
  }
  // 自动间隔随台词长度自适应:短句快、长句慢,读起来更自然
  const base = Math.round(settings.speed * 700);
  const lenFactor = Math.min(lastDialogueLen / 18, 2.5);
  const delay = Math.round(base * (1 + 0.35 * lenFactor));
  autoTimer = setTimeout(() => { if (autoMode || skipMode) advanceEngine(); }, delay);
}

// ---------------- 设置 ----------------
function bindSettings() {
  const bgm = $("set-bgm"), voice = $("set-voice"), speed = $("set-speed");
  const sync = () => {
    bgm.value = settings.bgm * 100;
    voice.value = settings.voice * 100;
    speed.value = String(settings.speed);
    const ls = $("set-lang");
    if (ls) ls.value = lang;
  };
  const apply = () => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    $("bgm").volume = settings.bgm;
    $("voice").volume = settings.voice;
  };
  sync();
  bgm.addEventListener("input", () => { settings.bgm = bgm.value / 100; apply(); });
  voice.addEventListener("input", () => { settings.voice = voice.value / 100; apply(); });
  speed.addEventListener("change", () => { settings.speed = parseFloat(speed.value); apply(); });
  const langSel = $("set-lang");
  if (langSel) langSel.addEventListener("change", () => {
    settings.lang = langSel.value;
    lang = langSel.value === "en" ? "en" : "zh";
    apply();
    applyLang();
  });
  const reset = $("set-reset");
  if (reset) reset.addEventListener("click", () => {
    settings = { ...DEFAULT_SETTINGS };
    lang = settings.lang;
    sync();
    apply();
    applyLang();
    log("[设置] 已恢复默认");
  });
  apply();
}

// ---------------- 回看历史 ----------------
function openBacklog() {
  const list = $("backlog-list");
  list.textContent = "";
  if (!backlog.length) {
    list.textContent = t("backlog_empty");
  }
  for (const b of backlog.slice().reverse()) {
    const row = document.createElement("div");
    row.className = "bl-row";
    row.setAttribute("role", "button");
    row.tabIndex = 0;
    row.title = b.file && b.effectIndex != null ? t("backlog_goto") : "";
    const who = document.createElement("div");
    who.className = "bl-who";
    who.textContent = (b.who || "…") + (b.scene ? ` · ${b.scene}` : "");
    const txt = document.createElement("div");
    txt.className = "bl-text";
    txt.textContent = b.text;
    row.append(who, txt);
    if (b.file && b.effectIndex != null) {
      const go = () => {
        $("backlog-modal").hidden = true;
        replayToDialogue(b.file, b.scene, b.effectIndex);
      };
      row.addEventListener("click", go);
      row.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") { e.preventDefault(); go(); }
      });
    }
    list.appendChild(row);
  }
  $("backlog-modal").hidden = false;
  list.scrollTop = list.scrollHeight;
  const cap = $("backlog-cap");
  if (cap) cap.textContent = backlog.length
    ? t("cap_fmt", backlog.length)
    : "";
}

/// 回看回跳:重新执行该场景到目标句(重放视觉,不重复记入历史)。
/// 目标句可能在另一个剧本文件(跨文件跳转后),需先按需重载。
async function replayToDialogue(file, label, effectIndex) {
  if (choosing) { log("[回看] 请先完成当前选择"); return; }
  stopAuto();
  try {
    // 非当前剧本 → 跨文件懒加载目标文件(缓存内,零额外网络)
    if (file && file !== currentScnBase) {
      const loaded = await loadSceneFile(file);
      if (!loaded) return;
    }
    const s = scenes.find((x) => x.label === label);
    if (!s) { log(`[回看] 场景 ${label} 缺失`); return; }
    if (!currentScnBytes) { log("[回看] 剧本字节缺失"); return; }
    const effs = JSON.parse(kag_run_scene(currentScnBytes, label)).effects;
    const idx = Math.min(effectIndex ?? 0, effs.length - 1);
    runId++;            // 接管当前任何运行中的场景
    choosing = false;
    currentSceneLabel = label;
    $("choices").textContent = "";
    $("dialogue-box").hidden = false;
    $("stage-empty").hidden = true;
    replaying = true;   // 重放期间不重复写入 backlog
    try {
      for (let i = 0; i <= idx; i++) await applyEffect(effs[i], i);
    } finally {
      replaying = false;
    }
    log(`[回看] 回到 ${label} 第 ${idx + 1}/${effs.length} 步`);
    $("hint").textContent = t("hintAdvance");
    if (autoMode || skipMode) scheduleNext(); else stopAuto();
  } catch (e) {
    log(`[回看] 回跳失败: ${e?.message || e}`);
  }
}

// ---------------- 存档 / 读档 ----------------
function openSaveModal(isSave) {
  saveModalMode = isSave; // 记录当前模式,供切语言时按同模式重渲染
  $("save-title").textContent = isSave ? t("save_save") : t("save_load");
  const slots = $("save-slots");
  slots.textContent = "";
  const all = JSON.parse(localStorage.getItem(SAVE_KEY) || "[]");
  for (let i = 0; i < 5; i++) {
    const s = all[i];
    const btn = document.createElement("button");
    btn.className = "save-slot";
    btn.textContent = s
      ? t("slot_fmt", i + 1, new Date(s.ts).toLocaleString(), (s.scn || "").replace(/.*[\\/]/, "").slice(0, 20), s.label)
      : t("slot_empty", i + 1);
    if (s && s.effectIndex != null) btn.textContent += t("step_tag", s.effectIndex);
    if (s && s.bgm) btn.textContent += " ♪";
    btn.title = s ? `${s.scn} → ${s.label}` : t("slot_none");
    if (!isSave && !s) {
      // 读档模式:空槽置灰,避免点了没反应又无提示
      btn.disabled = true;
      btn.textContent = t("slot_empty2", i + 1);
      btn.title = t("slot_none");
    }
    if (!(!isSave && !s)) btn.addEventListener("click", async () => {
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
    effectIndex: currentDlgIdx >= 0 ? currentDlgIdx : undefined, // 场景内精确位置,读档恢复到同一句
    ts: Date.now(),
    bgm: currentBgmName,   // 存档当前 BGM 名,读档后恢复(而非易失的 blob URL)
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
    const exact = scenes.find((x) => x.label === s.label);
    const target = exact || loaded.find((x) => x.steps.some((st) => st.type === "dialogue"));
    if (target) {
      if (s.effectIndex != null && exact) {
        // 精确恢复:重放到存档时的同一句(复用回看回跳机制,不从头重放)
        await replayToDialogue(base, s.label, s.effectIndex);
        log(`[读档] 已恢复到 ${s.label} 第 ${s.effectIndex} 步`);
      } else {
        playScene(target.label);
      }
      // 恢复存档时正在播放的 BGM(名称持久化,读档后重新懒加载)
      if (s.bgm) playBgm(s.bgm);
    }
  }
}

main();
