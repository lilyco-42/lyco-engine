import { readFileSync, readdirSync } from "node:fs";
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn } = wasm;
const DIR = "D:/Code/lyco/engine/realgame/scns/scn";
let withText = 0, total = 0;
for (const f of readdirSync(DIR).filter(x => x.endsWith(".scn"))) {
  const c = JSON.parse(compile_scn(new Uint8Array(readFileSync(`${DIR}/${f}`))));
  for (const s of c.scenes) {
    if (!/_(sel|select)$/i.test(s.label)) continue;
    total++;
    const d = s.steps.filter(st => st.type === "dialogue");
    if (d.length) {
      withText++;
      console.log(`${f} :: ${s.label} → 台词 ${d.length}:`, d[0].text.slice(0, 50));
    }
  }
}
console.log(`\n选择点 ${total}, 含文本 ${withText}`);
