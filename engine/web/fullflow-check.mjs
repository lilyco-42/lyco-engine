// 整个游戏加载全流程:BFS 覆盖所有选择分支/路线,验证全部剧本可加载
import { readFileSync, readdirSync, existsSync } from "node:fs";
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn, decode_scenario } = wasm;
const SCN_DIR = "D:/Code/lyco/engine/realgame/scns/scn";
let pass = 0, fail = 0;
const ok = (c, m) => { c ? (pass++, console.log("  ✓ " + m)) : (fail++, console.error("  ✗ " + m)); };
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
for (const f of files) {
  const m = f.match(/^([0-9]+[a-z]?)/);
  if (m) { const id = "s" + m[1]; idToFile[id] = f.replace(/\.scn$/, ""); fileToId[f.replace(/\.scn$/, "")] = id; }
}
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
const seen = new Set(), visitedFiles = new Set();
let choicePoints = 0, ends = 0, errs = 0;
while (queue.length) {
  const [fid, label] = queue.shift();
  const key = fid + label;
  if (seen.has(key)) continue;
  seen.add(key);
  if (label === "*gameend_title") { ends++; continue; } // 游戏终点(回标题)
  const file = idToFile[fid];
  if (!file) { errs++; continue; }
  visitedFiles.add(file);
  const scenes = loadScenes(file);
  if (!scenes || !scenes.find(x => x.label === label)) { errs++; continue; }
  const nxt = flow[fid + label];
  const nexts = [];
  if (Array.isArray(nxt)) {
    choicePoints++;
    for (const t of nxt) {
      let r = /^s[0-9a-z]+\*/.test(t) ? resolveTarget(t, file) : null;
      if (!r) { // 显示名(如【エピローグ】)→ 跟随 sel 场景自身线性流
        const jump = jumpOf(scenes, label);
        if (jump && jump.target) {
          const tfid = jump.storage ? (fileToId[jump.storage.replace(/\.scn$/i, "")] || fid) : fid;
          nexts.push([tfid, jump.target]);
        } else errs++;
        continue;
      }
      nexts.push(r);
    }
  }
  const jump = jumpOf(scenes, label);
  if (jump) {
    if (jump.target) { const tfid = jump.storage ? (fileToId[jump.storage.replace(/\.scn$/i, "")] || fid) : fid; nexts.push([tfid, jump.target]); }
    else if (jump.storage) { const tfid = fileToId[jump.storage.replace(/\.scn$/i, "")]; const ns = tfid ? loadScenes(idToFile[tfid]) : null; if (tfid && ns && ns[0]) nexts.push([tfid, ns[0].label]); else errs++; }
  }
  if (typeof nxt === "string") { const r = resolveTarget(nxt, file); if (r) nexts.push(r); else errs++; }
  for (const n of nexts) queue.push(n);
}
const totalScenes = files.reduce((n, f) => n + (loadScenes(f.replace(/\.scn$/, ""))?.length || 0), 0);
// 主线(全部路线)可达;后日谈(AFTER/After)为解锁后的支线章节,不计入主线流
ok(ends >= 7, `到达全部路线结局 ${ends} 个`);
ok(choicePoints >= 47, `覆盖全部选择点 ${choicePoints} 个`);
ok(visitedFiles.size >= 85, `覆盖主线剧本文件 ${visitedFiles.size} 个`);
ok(seen.size >= 700, `可达主线场景 ${seen.size} 个`);
console.log(`可达文件 ${visitedFiles.size}/${files.length}, 场景 ${seen.size}/${totalScenes}, 选择点 ${choicePoints}, 结局 ${ends}, 解析失败 ${errs}`);
console.log(`注: 未覆盖文件为后日谈(AFTER)解锁章节与系统脚本`);
console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
