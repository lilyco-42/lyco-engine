// 播放器全流程端到端:经后端 /api 懒加载 + wasm 编译 + 流程逻辑,遍历整个游戏主线。
// 需 yuzu-server 运行(127.0.0.1:8080, --data realgame)。
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn, decode_scenario } = wasm;
const BASE = "http://127.0.0.1:8080/api/archives";
let pass = 0, fail = 0;
const ok = (c, m) => { c ? (pass++, console.log("  ✓ " + m)) : (fail++, console.error("  ✗ " + m)); };

async function apiBytes(p) { const r = await fetch(`${BASE}/data.xp3${p}`); if (!r.ok) throw new Error(`${p} HTTP ${r.status}`); return new Uint8Array(await r.arrayBuffer()); }

// 1) 文件列表 → s-ID ↔ 文件名
const files = (await (await fetch(`${BASE}/data.xp3/files`)).json()).files;
const scnFiles = files.filter((f) => f.name.endsWith(".scn"));
const idToFile = {}, fileToId = {};
for (const f of scnFiles) {
  const base = f.name.replace(/.*\\/, "").replace(/\.scn$/i, "");
  const m = base.match(/^([0-9]+[a-z]?)/);
  if (m) { idToFile["s" + m[1]] = base; fileToId[base] = "s" + m[1]; }
}
ok(scnFiles.length >= 90, `归档内 ${scnFiles.length} 个剧本文件`);

// 2) 场景图(经后端解码 scnchartdata.tjs)
function parseFlow(text) {
  const flow = {};
  const lines = text.split("\n");
  let arrKey = null, arrTargets = [];
  for (const line of lines) {
    const openArr = line.match(/^\s*"([^"]+)"\s*=>\s*\(const\)\s*\[\s*$/);
    if (openArr) { arrKey = openArr[1]; arrTargets = []; continue; }
    if (arrKey) {
      const t = line.match(/^\s*"([^"]+)",?\s*$/);
      if (t) { arrTargets.push(t[1]); continue; }
      if (line.includes("]")) { if (!arrKey.includes(":")) flow[arrKey] = arrTargets; arrKey = null; continue; }
      continue;
    }
    const m = line.match(/^\s*"([^"]+)"\s*=>\s*"([^"]+)"/);
    if (m && !m[1].includes(":")) flow[m[1]] = m[2];
  }
  return flow;
}
const flow = parseFlow(decode_scenario(await apiBytes(`/file?path=${encodeURIComponent("main\\scnchartdata.tjs")}`)));

// 3) 经后端懒加载场景文件
const fileCache = {};
async function loadScenes(file) {
  if (fileCache[file]) return fileCache[file];
  const cand = file.endsWith(".ks")
    ? [`scn\\${file}.scn`, `scn\\${file}`]
    : [`scn\\${file}.ks.scn`, `scn\\${file}.scn`];
  for (const p of cand) {
    try {
      const bytes = await apiBytes(`/file?path=${encodeURIComponent(p)}`);
      fileCache[file] = JSON.parse(compile_scn(bytes)).scenes;
      return fileCache[file];
    } catch { /* 试下一种 */ }
  }
  return null;
}
const jumpOf = (scenes, label) => scenes.find((x) => x.label === label)?.steps.find((st) => st.type === "jump" && (st.target || st.storage));
function resolveTarget(t, curFile) {
  if (!t) return null;
  if (t.includes("*")) { const [fid, lab] = [t.split("*")[0], "*" + t.split("*")[1]]; return idToFile[fid] ? [fid, lab] : null; }
  const fid = idToFile[t] ? t : fileToId[t];
  return fid ? [fid, null] : null; // 纯文件:进入首个有台词场景(遍历时用 null 标记)
}

// 4) BFS 遍历全部主线
const queue = [["s001", "*com_part_1"]];
const seen = new Set(), visitedFiles = new Set();
let choicePoints = 0, ends = 0, errs = 0;
while (queue.length) {
  const [fid, label] = queue.shift();
  const key = fid + label;
  if (seen.has(key)) continue;
  seen.add(key);
  if (label === "*gameend_title") { ends++; continue; }
  const file = idToFile[fid];
  if (!file) { errs++; continue; }
  visitedFiles.add(file);
  const scenes = await loadScenes(file);
  if (!scenes) { errs++; continue; }
  if (label) {
    if (!scenes.find((x) => x.label === label)) { errs++; continue; }
  } else {
    // 纯文件入口:首个有台词场景
    const s = scenes.find((x) => x.steps.some((st) => st.type === "dialogue")) || scenes[0];
    if (!s) { errs++; continue; }
  }
  const useLabel = label || (scenes.find((x) => x.steps.some((st) => st.type === "dialogue")) || scenes[0]).label;
  const nxt = flow[fid + useLabel];
  const nexts = [];
  if (Array.isArray(nxt)) {
    choicePoints++;
    for (const t of nxt) {
      let r = /^s[0-9a-z]+\*/.test(t) ? resolveTarget(t, file) : null;
      if (!r) {
        const jump = jumpOf(scenes, useLabel);
        if (jump && jump.target) { const tfid = jump.storage ? (fileToId[jump.storage.replace(/\.scn$/i, "")] || fid) : fid; nexts.push([tfid, jump.target]); }
        else errs++;
        continue;
      }
      nexts.push([r[0], r[1] || (scenes.find((x) => x.steps.some((st) => st.type === "dialogue")) || scenes[0])?.label]);
    }
  }
  const jump = jumpOf(scenes, useLabel);
  if (jump) {
    if (jump.target) { const tfid = jump.storage ? (fileToId[jump.storage.replace(/\.scn$/i, "")] || fid) : fid; nexts.push([tfid, jump.target]); }
    else if (jump.storage) { const tfid = fileToId[jump.storage.replace(/\.scn$/i, "")]; if (tfid) nexts.push([tfid, null]); else errs++; }
  }
  if (typeof nxt === "string") { const r = resolveTarget(nxt, file); if (r) nexts.push([r[0], r[1] || null]); else errs++; }
  for (const n of nexts) queue.push(n);
}
ok(ends >= 7, `经后端到达全部路线结局 ${ends} 个`);
ok(choicePoints >= 47, `覆盖全部选择点 ${choicePoints} 个`);
ok(visitedFiles.size >= 85, `经后端覆盖主线文件 ${visitedFiles.size} 个`);
ok(seen.size >= 700, `可达主线场景 ${seen.size} 个`);
console.log(`后端全流程: 文件 ${visitedFiles.size}/${scnFiles.length}, 场景 ${seen.size}, 选择点 ${choicePoints}, 结局 ${ends}, 解析失败 ${errs}`);
console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
