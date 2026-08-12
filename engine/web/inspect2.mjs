import { readFileSync } from "node:fs";
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn } = wasm;
const scn = new Uint8Array(readFileSync("D:/Code/lyco/engine/realgame/scn001.scn"));
const c = JSON.parse(compile_scn(scn));
// 统计事件类型
const kinds = {};
let selEvents = [];
for (const s of c.scenes) {
  for (const st of s.steps) {
    if (st.type === "event") {
      kinds[st.kind] = (kinds[st.kind]||0)+1;
      if (/sel|select|choice|choice/i.test(st.kind)) selEvents.push({scene:s.label, kind:st.kind, fields:st.fields.slice(0,3)});
    }
  }
}
console.log("事件类型统计:", JSON.stringify(kinds, null, 1));
console.log("\n选择类事件:", JSON.stringify(selEvents.slice(0,10), null, 1));
