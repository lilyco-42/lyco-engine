import { readFileSync } from "node:fs";
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn } = wasm;
for (const file of ["scn001.scn", "scn100.scn"]) {
  const scn = new Uint8Array(readFileSync(`D:/Code/lyco/engine/realgame/${file}`));
  const c = JSON.parse(compile_scn(scn));
  console.log(`\n######## ${file} ########`);
  for (const s of c.scenes) {
    const events = s.steps.filter(st => st.type==="event");
    const jumps = s.steps.filter(st => st.type==="jump");
    const cmds = s.steps.filter(st => st.type==="command");
    if (events.length || jumps.length || cmds.length) {
      console.log(`== ${s.label} (steps=${s.steps.length}) events=${events.length} jumps=${jumps.length} cmds=${cmds.length}`);
      const evkinds = {};
      for (const e of events) evkinds[e.kind]=(evkinds[e.kind]||0)+1;
      console.log("   events:", JSON.stringify(evkinds));
      for (const j of jumps) console.log("   JUMP:", j.storage, "::", j.target);
    }
  }
}
