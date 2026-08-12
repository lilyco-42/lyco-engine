import { readFileSync } from "node:fs";
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn } = wasm;
for (const file of ["scn001.scn", "scn100.scn"]) {
  const scn = new Uint8Array(readFileSync(`D:/Code/lyco/engine/realgame/${file}`));
  const c = JSON.parse(compile_scn(scn));
  const cmds = new Set();
  for (const s of c.scenes) for (const st of s.steps)
    if (st.type === "command") cmds.add(st.value);
  console.log(`\n== ${file} 命令字符串 ==`);
  for (const v of [...cmds].slice(0, 40)) console.log("   ", v);
  // envupdate 中的 imageFile.file 引用(背景/立绘)
  const imgs = new Set();
  for (const s of c.scenes) for (const st of s.steps)
    if (st.type === "event" && st.kind === "envupdate")
      for (const f of st.fields) {
        if (typeof f === "string" && f.includes("imageFile")) { /* skip */ }
        try { const m = JSON.stringify(f).match(/"file":"([^"]+)"/g); if (m) for (const x of m) imgs.add(x); } catch {}
      }
  console.log(`== envupdate imageFile.file 引用(前20) ==`);
  for (const v of [...imgs].slice(0,20)) console.log("   ", v);
}
