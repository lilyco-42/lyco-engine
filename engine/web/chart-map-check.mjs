// 场景图章节地图:解析 scnchartdata → 章节列表 + 入口场景
import { readFileSync } from "node:fs";
import wasm from "./pkg-node/yuzu_wasm.js";
const { decode_scenario } = wasm;
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
      if (line.includes("]")) {
        if (/^s\d+[a-z]?$/.test(arrKey) && arrTargets.length) caps.push({ id: arrKey, chapter: arrTargets[0] || "", title: arrTargets[1] || arrTargets[0] || "" });
        else if (arrKey.includes("_sel") && !arrKey.includes(":")) sel[arrKey] = arrTargets;
        arrKey = null; continue;
      }
      continue;
    }
    const m = line.match(/^\s*"([^"]+)"\s*=>\s*"([^"]+)"/);
    if (m && !m[1].includes(":")) flow[m[1]] = m[2];
  }
  return { sel, flow, caps };
}
const raw = readFileSync("D:/Code/lyco/engine/realgame/scns/main/scnchartdata.tjs");
const { sel, flow, caps } = parseSceneChart(decode_scenario(new Uint8Array(raw)));
for (const ch of caps) { const ks = Object.keys(flow).filter(k => k.startsWith(ch.id + "*")); const ek = ks.find(k => k.includes("com_part_1")) || ks[0]; ch.entry = ek ? "*" + ek.split("*")[1] : null; }
ok(caps.length >= 80, "章节数 " + caps.length);
const c1 = caps.find(c => c.id === "s001");
ok(c1 && c1.entry === "*com_part_1", "s001 入口 " + (c1 && c1.entry));
ok(sel["s001*com_part_1_sel"] && sel["s001*com_part_1_sel"].length === 2, "s001 选择点 2 分支");
const routes = caps.filter(c => /s[1-5]\d\d/.test(c.id));
ok(routes.length >= 40, "路线章节 " + routes.length);
console.log("样例:", caps.slice(0,3).map(c => c.id + "=" + c.chapter + " " + c.title).join(" | "));
console.log("\n" + pass + " passed, " + fail + " failed");
process.exit(fail ? 1 : 0);
