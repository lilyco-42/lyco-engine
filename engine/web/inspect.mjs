import { readFileSync } from "node:fs";
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn } = wasm;
const scn = new Uint8Array(readFileSync("D:/Code/lyco/engine/realgame/scn001.scn"));
const c = JSON.parse(compile_scn(scn));
for (const s of c.scenes) {
  if (!/sel|dummyselect/.test(s.label)) continue;
  console.log(`=== ${s.label} ===`);
  for (const st of s.steps) {
    const t = st.type;
    if (t === "dialogue") console.log(`  DIALOGUE: ${st.character}: ${st.text}`);
    else if (t === "label") console.log(`  LABEL: ${st.value}`);
    else if (t === "event") console.log(`  EVENT: ${st.kind} ${JSON.stringify(st.fields).slice(0,120)}`);
    else if (t === "jump") console.log(`  JUMP: ${st.storage}::${st.target}`);
    else if (t === "command") console.log(`  CMD: ${st.value}`);
    else console.log(`  ${t}`);
  }
}
