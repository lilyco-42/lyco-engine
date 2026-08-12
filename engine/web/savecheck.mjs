// 存档→读档 恢复路径验证:取当前场景标签 → 反推 base → 重新加载 → 目标可续播
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn } = wasm;
const BASE = "http://127.0.0.1:8080/api/archives/data.xp3/file?path=";
async function loadEntry(path) {
  const r = await fetch(BASE + encodeURIComponent(path));
  if (!r.ok) throw new Error(`HTTP ${r.status} ${path}`);
  return new Uint8Array(await r.arrayBuffer());
}
// 模拟 saveSlot 存下的字段(与 web/main.js 完全一致的反推逻辑)
const scn = "scn\\001・アーサー王ver1.07.ks.scn";
const label = "*com_part_1:2";
const base = scn.replace(/^scn[\\/]/i, "").replace(/\.scn$/i, "");
const bytes = await loadEntry(`scn\\${base}.scn`);
const scenes = JSON.parse(compile_scn(bytes)).scenes;
const target = scenes.find((s) => s.label === label)
  || scenes.find((s) => s.steps.some((st) => st.type === "dialogue"));
console.log("scn:", scn, "→ base:", base);
console.log("读档目标:", target ? target.label : "缺失", target && target.steps.length, "步");
if (!target) process.exit(1);
const dlg = target.steps.find((st) => st.type === "dialogue");
console.log(dlg ? `✓ 台词场景可续播: ${dlg.text.slice(0, 40)}…` : "✗ 无台词");
process.exit(dlg ? 0 : 1);
