// 检视编译后场景的步骤结构,确认可预取的资源引用形式
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn } = wasm;
const BASE = "http://127.0.0.1:8080/api/archives/data.xp3/file?path=";
const r = await fetch(BASE + encodeURIComponent("scn\\001・アーサー王ver1.07.ks.scn"));
const scenes = JSON.parse(compile_scn(new Uint8Array(await r.arrayBuffer()))).scenes;
const s = scenes.find((x) => x.label === "*com_part_1:2");
const types = {}, samples = {};
for (const st of s.steps) {
  types[st.type] = (types[st.type] || 0) + 1;
  if (!samples[st.type]) samples[st.type] = st;
}
console.log("步骤类型分布:", JSON.stringify(types));
for (const [t, st] of Object.entries(samples)) {
  console.log("——", t, "——", JSON.stringify(st).slice(0, 240));
}
