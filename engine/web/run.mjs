#!/usr/bin/env node
// 跨平台 WASM 客户端宿主(Node):同一 wasm 引擎(compile_scn/kag_run_scene)
// + 同一流式缓存(cache.mjs),在非浏览器环境跑剧本(含跨文件跳转)。
// 证明规格"采用 WASM 作为客户端运行时"不受浏览器绑定。
// 用法: node run.mjs [--scene LABEL] [--steps N]   (文件从 *com_part_1 起,默认 data.xp3)
import wasm from "./pkg-node/yuzu_wasm.js";
import { AssetCache } from "./cache.mjs";

const { compile_scn, kag_run_scene } = wasm;
const BASE = process.env.YUZU_BASE || "http://127.0.0.1:8080/api/archives";

const args = process.argv.slice(2);
const arg = (k) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : undefined; };
const startScene = arg("--scene") || "*com_part_1";
const maxSteps = parseInt(arg("--steps") || "40", 10);

const cache = new AssetCache({ persist: false }); // node 无 IndexedDB → 纯内存,读入口与浏览器一致
let net = 0;
async function fetchBytes(url) { net++; const r = await fetch(url); if (!r.ok) throw new Error(`HTTP ${r.status} ${url}`); return new Uint8Array(await r.arrayBuffer()); }
async function readEntry(source, path) {
  const key = `${source.kind}:${source.name}:${path}`;
  const hit = await cache.getPersist(key);
  if (hit) return hit;
  const bytes = await fetchBytes(`${BASE}/${encodeURIComponent(source.name)}/file?path=${encodeURIComponent(path)}`);
  cache.set(key, bytes);
  return bytes;
}

const source = { kind: "backend", name: "data.xp3" };
let scnBytes = null, scenes = [];
let fileBase = null;
async function loadFile(base) {
  fileBase = base;
  scnBytes = await readEntry(source, `scn\\${base}.scn`);
  scenes = JSON.parse(compile_scn(scnBytes)).scenes;
}
const byLabel = (l) => scenes.find((s) => s.label === l);

await loadFile("001・アーサー王ver1.07.ks");
let label = startScene, step = 0;
const stats = { dlg: 0, layers: 0, audio: 0, chapters: 0 };
const seen = new Set();
const lines = [];

while (step < maxSteps && !seen.has(`${fileBase}#${label}`)) {
  seen.add(`${fileBase}#${label}`);
  const s = byLabel(label);
  if (!s) { console.log(`  ✗ 场景缺失 ${label} @ ${fileBase}`); process.exit(1); }
  const { effects } = JSON.parse(kag_run_scene(scnBytes, label));
  for (const e of effects) {
    if (e.type === "dialogue") {
      stats.dlg++;
      lines.push(`${e.character || "…"}: ${e.text}`);
      if (e.voice) stats.audio++;
    } else if (e.type === "layer") stats.layers++;
    else if (e.type === "audio") stats.audio++;
    else if (e.type === "chapter") { stats.chapters++; console.log(`  [章节] ${e.title}`); }
  }
  const jump = s.steps.find((st) => st.type === "jump" && (st.target || st.storage));
  if (!jump) break;
  const storageBase = jump.storage ? jump.storage.replace(/\.scn$/i, "") : fileBase;
  if (storageBase && storageBase !== fileBase) {
    const before = net;
    await loadFile(storageBase); // 跨文件:懒加载目标剧本(与浏览器 handleJump 同路径)
    console.log(`  [跳转] → ${storageBase} (新文件,${net - before} 次网络)`);
  }
  label = jump.target || label;
  step++;
}
const s = await cache.stats();
console.log(`▶ WASM 宿主: ${startScene} → ${fileBase}@${label}`);
console.log(`--- 台词 ${stats.dlg} 句 ---`);
lines.slice(0, 14).forEach((d, i) => console.log(`  ${i + 1}. ${d}`));
if (lines.length > 14) console.log(`  … 其余 ${lines.length - 14} 句`);
console.log(`--- 统计: 图层 ${stats.layers} | 音频 ${stats.audio} | 章节 ${stats.chapters} | 网络 ${net} 次 | 缓存命中 ${s.hits} ---`);
process.exit(stats.dlg > 0 ? 0 : 1);
