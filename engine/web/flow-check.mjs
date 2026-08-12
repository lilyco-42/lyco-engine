// 验证场景跳转流 + 选择分支推导(打真实剧本)
import { readFileSync } from "node:fs";
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn } = wasm;
let pass = 0, fail = 0;
const ok = (c, m) => { c ? (pass++, console.log("  ✓ " + m)) : (fail++, console.error("  ✗ " + m)); };

const scn = new Uint8Array(readFileSync("D:/Code/lyco/engine/realgame/scn001.scn"));
const c = JSON.parse(compile_scn(scn));
const scenes = c.scenes;
const byLabel = (l) => scenes.find((s) => s.label === l);

// 1) 线性流跟随:*com_part_1 → ... → *com_part_1_sel
const flow = [];
let cur = "*com_part_1";
const seen = new Set();
while (cur && !seen.has(cur) && flow.length < 12) {
  seen.add(cur); flow.push(cur);
  const s = byLabel(cur);
  const jump = s ? s.steps.find((st) => st.type === "jump" && st.target) : null;
  cur = jump ? jump.target : null;
}
ok(flow.includes("*com_part_1_sel"), `线性流到达选择点: ${flow.join(" → ")}`);

// 2) 选择分支推导:sel 场景后含台词的场景 → *001_01A / *001_01B
const selIdx = scenes.indexOf(byLabel("*com_part_1_sel"));
const branches = [];
for (let i = selIdx + 1; i < scenes.length && branches.length < 4; i++) {
  const s = scenes[i];
  if (s.steps.some((st) => st.type === "dialogue")) branches.push(s.label);
  else if (branches.length > 0 && !/dummy/i.test(s.label)) break;
}
ok(branches.includes("*001_01A") && branches.includes("*001_01B"), `选择分支推导: ${branches.join(", ")}`);

// 3) 分支汇合:*001_01A / *001_01B → dummyX(2 个 nexts)→ *001_01com
const jumpOf = (l) => byLabel(l).steps.find((st) => st.type === "jump" && st.target);
const aJump = jumpOf("*001_01A"), bJump = jumpOf("*001_01B");
ok(aJump && /dummy/i.test(aJump.target), `分支A → ${aJump.target}`);
ok(bJump && /dummy/i.test(bJump.target), `分支B → ${bJump.target}`);
const dummy3 = byLabel(aJump.target);
ok(dummy3.steps.filter((st) => st.type === "jump" && st.target === "*001_01com").length === 2, `分支A 汇合到 *001_01com`);

// 3b) 分支推导应过滤汇合场景,仅得 A/B
const selIdx2 = scenes.indexOf(byLabel("*com_part_1_sel"));
const br2 = [];
for (let i = selIdx2 + 1; i < scenes.length && br2.length < 4; i++) {
  const s = scenes[i];
  if (!s.steps.some((st) => st.type === "dialogue")) { if (br2.length > 0 && !/dummy/i.test(s.label)) break; continue; }
  if (/(com|common|合流)/i.test(s.label)) continue;
  br2.push(s.label);
}
ok(br2.join(",") === "*001_01A,*001_01B", `分支过滤后: ${br2.join(", ")}`);

// 4) 跨文件跳转:*dummy5 → 002・祟り神ver1.08.ks::*com_part_2
const dummy5jump = byLabel("*dummy5").steps.find(st => st.type==="jump" && st.target);
ok(dummy5jump && dummy5jump.storage === "002・祟り神ver1.08.ks", `跨文件跳转: ${dummy5jump.storage}::${dummy5jump.target}`);

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
