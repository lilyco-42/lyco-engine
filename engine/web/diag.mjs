import { readFileSync, readdirSync, existsSync } from "node:fs";
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn, decode_scenario } = wasm;
const SCN_DIR = "D:/Code/lyco/engine/realgame/scns/scn";
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
const flow = parseFlow(decode_scenario(new Uint8Array(readFileSync("D:/Code/lyco/engine/realgame/scns/main/scnchartdata.tjs"))));
const files = readdirSync(SCN_DIR).filter(f => f.endsWith(".scn")).sort();
const fileCache = {}, idToFile = {}, fileToId = {};
for (const f of files) { const num = f.match(/^(\d+)/)?.[1]; if (num) { idToFile["s" + num] = f.replace(/\.scn$/, ""); fileToId[f.replace(/\.scn$/, "")] = "s" + num; } }
function loadScenes(file) {
  if (!existsSync(`${SCN_DIR}/${file}.scn`)) return null;
  if (!fileCache[file]) fileCache[file] = JSON.parse(compile_scn(new Uint8Array(readFileSync(`${SCN_DIR}/${file}.scn`)))).scenes;
  return fileCache[file];
}
const jumpOf = (scenes, label) => scenes.find(x => x.label === label)?.steps.find(st => st.type === "jump" && (st.target || st.storage));
function resolveTarget(t, curFile) {
  if (!t) return null;
  if (t.includes("*")) { const [fid, lab] = [t.split("*")[0], "*" + t.split("*")[1]]; return idToFile[fid] ? [fid, lab] : null; }
  const fid = idToFile[t] ? t : fileToId[t];
  const ns = fid ? loadScenes(idToFile[fid] || t) : null;
  return (fid && ns && ns[0]) ? [fid, ns[0].label] : null;
}
const queue = [["s001", "*com_part_1"]];
const seen = new Set(), reachedFiles = new Set();
const dead = [], errs = [];
while (queue.length) {
  const [fid, label] = queue.shift();
  const key = fid + label;
  if (seen.has(key)) continue;
  seen.add(key);
  const file = idToFile[fid];
  if (!file) { dead.push(`${fid}${label}(无文件)`); continue; }
  reachedFiles.add(file);
  const scenes = loadScenes(file);
  if (!scenes || !scenes.find(x => x.label === label)) { dead.push(`${file} ${label}(场景缺失)`); continue; }
  const nxt = flow[fid + label];
  const nexts = [];
  if (Array.isArray(nxt)) for (const t of nxt) { const r = resolveTarget(t, file); if (r) nexts.push(r); else errs.push(`选择[${file} ${label} -> ${t}]`); }
  const jump = jumpOf(scenes, label);
  if (jump) {
    if (jump.target) { const tfid = jump.storage ? (fileToId[jump.storage.replace(/\.scn$/i, "")] || fid) : fid; nexts.push([tfid, jump.target]); }
    else if (jump.storage) { const tfid = fileToId[jump.storage.replace(/\.scn$/i, "")]; const ns = tfid ? loadScenes(idToFile[tfid]) : null; if (tfid && ns && ns[0]) nexts.push([tfid, ns[0].label]); }
  }
  if (typeof nxt === "string") { const r = resolveTarget(nxt, file); if (r) nexts.push(r); else errs.push(`流[${file} ${label} -> ${nxt}]`); }
  for (const n of nexts) queue.push(n);
}
console.log("=== 死路(终端/缺失) ===");
dead.slice(0, 12).forEach(d => console.log("  ", d));
console.log("=== 解析失败(非死路) ===");
errs.slice(0, 10).forEach(e => console.log("  ", e));
const unreached = files.map(f => f.replace(/\.scn$/, "")).filter(f => !reachedFiles.has(f));
console.log(`\n未达文件 ${unreached.length}:`);
unreached.slice(0, 15).forEach(f => console.log("  ", f));
console.log(`场景总数(全部95文件): ${files.reduce((n, f) => n + (loadScenes(f.replace(/\.scn$/,""))?.length || 0), 0)}`);
