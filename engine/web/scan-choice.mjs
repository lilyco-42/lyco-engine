import { readFileSync, readdirSync } from "node:fs";
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn } = wasm;
const dir = "D:/Code/lyco/engine/realgame/scns/scn";
let selScenes = [], branchScenes = [];
for (const f of readdirSync(dir).filter(x => x.endsWith(".scn"))) {
  const c = JSON.parse(compile_scn(new Uint8Array(readFileSync(`${dir}/${f}`))));
  for (const s of c.scenes) {
    // 选择点:标签含 sel/select,或含多条非空跳转,或含选择事件
    const jumps = s.steps.filter(st => st.type === "jump" && (st.storage || st.target));
    const isSel = /_?sel/i.test(s.label);
    const dialogues = s.steps.filter(st => st.type === "dialogue").length;
    if (isSel && dialogues > 0) selScenes.push(`${f} :: ${s.label} (台词 ${dialogues})`);
    if (jumps.length >= 2) branchScenes.push(`${f} :: ${s.label} → ${jumps.map(j => j.target).join(", ")}`);
  }
}
console.log("== 含台词的 _sel 场景 =="); selScenes.slice(0, 15).forEach(x => console.log("  ", x));
console.log(`\n== 多分支跳转场景 (${branchScenes.length}) ==`); branchScenes.slice(0, 10).forEach(x => console.log("  ", x));
