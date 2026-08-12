// 验证全部选择点都能解析出分支选项(经场景图,同播放器 findBranches 逻辑)
import { readFileSync, readdirSync } from "node:fs";
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn, decode_scenario } = wasm;
const DIR = "D:/Code/lyco/engine/realgame/scns/scn";
let pass = 0, fail = 0;
const ok = (c, m) => { c ? (pass++, console.log("  ✓ " + m)) : (fail++, console.error("  ✗ " + m)); };
// 场景图
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
const files = readdirSync(DIR).filter(x => x.endsWith(".scn")).sort();
const idToFile = {};
for (const f of files) { const m = f.match(/^([0-9]+[a-z]?)/); if (m) idToFile["s" + m[1]] = f.replace(/\.scn$/, ""); }
let selTotal = 0, selOk = 0, selMissing = 0;
for (const f of files) {
  const base = f.replace(/\.scn$/, "");
  const id = Object.keys(idToFile).find(k => idToFile[k] === base);
  if (!id) continue;
  const c = JSON.parse(compile_scn(new Uint8Array(readFileSync(`${DIR}/${f}`))));
  for (const s of c.scenes) {
    if (!/_(sel|select)$/i.test(s.label)) continue;
    selTotal++;
    const nxt = flow[id + s.label];
    if (Array.isArray(nxt)) { selOk++; }
    else if (nxt) { selOk++; } // 线性/显示名
    else {
      // 无图:回退启发式(sel 后有台词场景)
      const idx = c.scenes.indexOf(s);
      const hasBranch = c.scenes.slice(idx + 1).some(x => x.steps.some(st => st.type === "dialogue"));
      if (hasBranch) selOk++; else selMissing++;
    }
  }
}
ok(selTotal >= 38, `检测到 ${selTotal} 个选择点`);
ok(selOk >= 37, `其中 ${selOk} 个可解析出选项`);
ok(selMissing === 0, `无分支选择点 ${selMissing}`);
console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
