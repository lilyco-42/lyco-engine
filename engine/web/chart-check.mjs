// 验证场景图解析:解码 scnchartdata.tjs → 选择点 → 真实目标
import { readFileSync } from "node:fs";
import wasm from "./pkg-node/yuzu_wasm.js";
const { decode_scenario } = wasm;
let pass = 0, fail = 0;
const ok = (c, m) => { c ? (pass++, console.log("  ✓ " + m)) : (fail++, console.error("  ✗ " + m)); };

const raw = readFileSync("D:/Code/lyco/engine/realgame/scns/main/scnchartdata.tjs");
const text = decode_scenario(new Uint8Array(raw));

// 与 main.js 相同的 parseSceneChart
function parseSceneChart(text) {
  const map = {};
  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(/^\s*"([^"]+)"\s*=>\s*\(const\)\s*\[\s*$/);
    if (!m || !m[1].includes("_sel") || m[1].includes(":")) continue;
    const targets = [];
    for (let j = i + 1; j < lines.length; j++) {
      const t = lines[j].match(/^\s*"([^"]+)",?\s*$/);
      if (t) targets.push(t[1]);
      else if (lines[j].includes("]")) break;
    }
    if (targets.length) map[m[1]] = targets;
  }
  return map;
}
const chart = parseSceneChart(text);
ok(Object.keys(chart).length >= 8, `场景图解析出 ${Object.keys(chart).length} 个选择点`);
ok(JSON.stringify(chart["s001*com_part_1_sel"]) === JSON.stringify(["s001*001_01A","s001*001_01B"]),
  `com_part_1_sel → ${JSON.stringify(chart["s001*com_part_1_sel"])}`);
ok(chart["s006*com_part_6_sel"] && chart["s006*com_part_6_sel"].length === 2, `s006 sel → 2 分支`);
console.log("选择点:", Object.keys(chart).slice(0, 8).join(", "));
console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
