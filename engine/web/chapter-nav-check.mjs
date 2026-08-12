// 章节导航逻辑:解析场景图 → 选章节 → 加载文件 → 播放入口 → 流入台词
import { readFileSync } from "node:fs";
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn, decode_scenario } = wasm;
const SCN_DIR = "D:/Code/lyco/engine/realgame/scns/scn";
let pass = 0, fail = 0;
const ok = (c, m) => { c ? (pass++, console.log("  ✓ " + m)) : (fail++, console.error("  ✗ " + m)); };
function parseSceneChart(text) {
  const sel = {}, flow = {}, caps = [];
  const lines = text.split("\n");
  let arrKey = null, arrTargets = [];
  for (const line of lines) {
    const openArr = line.match(/^\s*"([^"]+)"\s*=>\s*\(const\)\s*\[\s*$/);
    if (openArr) { arrKey = openArr[1]; arrTargets = []; continue; }
    if (arrKey) {
      const t = line.match(/^\s*"([^"]+)",?\s*$/);
      if (t) { arrTargets.push(t[1]); continue; }
      if (line.includes("]")) { if (/^s\d+[a-z]?$/.test(arrKey) && arrTargets.length) caps.push({ id: arrKey, chapter: arrTargets[0] || "", title: arrTargets[1] || arrTargets[0] || "" }); else if (arrKey.includes("_sel") && !arrKey.includes(":")) sel[arrKey] = arrTargets; arrKey = null; continue; }
      continue;
    }
    const m = line.match(/^\s*"([^"]+)"\s*=>\s*"([^"]+)"/);
    if (m && !m[1].includes(":")) flow[m[1]] = m[2];
  }
  return { sel, flow, caps };
}
const chart = parseSceneChart(decode_scenario(new Uint8Array(readFileSync("D:/Code/lyco/engine/realgame/scns/main/scnchartdata.tjs"))));
const c1 = chart.caps.find(c => c.id === "s001");
const ks = Object.keys(chart.flow).filter(k => k.startsWith("s001*"));
const entryKey = ks.find(k => k.includes("com_part_1")) || ks[0];
c1.entry = "*" + entryKey.split("*")[1];
ok(c1.entry === "*com_part_1", "s001 入口 " + c1.entry);
// 加载 001 文件
const scenes = JSON.parse(compile_scn(new Uint8Array(readFileSync(SCN_DIR + "/001・アーサー王ver1.07.ks.scn")))).scenes;
ok(!!scenes.find(s => s.label === c1.entry), "入口场景存在");
// 播放入口 → 自动流到台词
let label = c1.entry, steps = 0, reached = false;
while (steps < 50 && !reached) {
  const s = scenes.find(x => x.label === label);
  if (!s) break;
  if (s.steps.some(st => st.type === "dialogue")) { reached = true; break; }
  const j = s.steps.find(st => st.type === "jump" && st.target);
  if (j) { label = j.target; steps++; } else break;
}
ok(reached, "从入口经 " + steps + " stub 流入台词 " + label);
// 路由章节可达
ok(chart.caps.filter(c => /s[1-5]\d\d/.test(c.id)).length >= 40, "路线章节可列");
console.log("\n" + pass + " passed, " + fail + " failed");
process.exit(fail ? 1 : 0);
