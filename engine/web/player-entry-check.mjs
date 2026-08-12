// 播放器入口流:从 *com_part_1 出发,stub 自动流到首个台词(经后端加载)
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn } = wasm;
const BASE = "http://127.0.0.1:8080/api/archives";
let pass = 0, fail = 0;
const ok = (c, m) => { c ? (pass++, console.log("  ✓ " + m)) : (fail++, console.error("  ✗ " + m)); };
async function scn(path) {
  const r = await fetch(`${BASE}/data.xp3/file?path=${encodeURIComponent(path)}`);
  return JSON.parse(compile_scn(new Uint8Array(await r.arrayBuffer()))).scenes;
}
const scenes = await scn("scn" + "\\" + "001・アーサー王ver1.07.ks.scn");
const byLabel = (l) => scenes.find((s) => s.label === l);
let label = "*com_part_1", steps = 0, dialogues = 0, visited = new Set();
while (steps < 200 && !visited.has(label)) {
  visited.add(label);
  const s = byLabel(label);
  if (!s) { ok(false, "场景缺失 " + label); process.exit(1); }
  const firstD = s.steps.find((st) => st.type === "dialogue");
  if (firstD) { dialogues++; ok(true, "入口流到达台词场景 " + label); break; }
  const jump = s.steps.find((st) => st.type === "jump" && st.target);
  if (jump) { label = jump.target; steps++; continue; }
  ok(false, "卡在 " + label); process.exit(1);
}
ok(dialogues >= 1 && steps > 0, "经 " + steps + " 个 stub 场景流入台词");
ok(visited.has("*com_part_1:2"), "到达主线 *com_part_1:2");
console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
