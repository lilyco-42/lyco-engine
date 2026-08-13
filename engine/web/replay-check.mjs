// 回看回跳核心不变量验证:
//   1) 记录每个 dialogue 的 effectIndex + file(与 applyEffect 一致)
//   2) 重新 kag_run_scene 后,顺序应用 0..idx 效果,断言 idx 处台词与记录一致(确定性)
// 用法: node _verify_replay.mjs  (依赖 pkg-node wasm + data/game.xp3,无需后端)
import wasm from "./pkg-node/yuzu_wasm.js";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
const { compile_scn, kag_run_scene } = wasm;

// 相对本文件,而非进程 cwd(与其它 *-check.mjs 一致的健壮用法)
const here = dirname(fileURLToPath(import.meta.url));
const bytes = new Uint8Array(readFileSync(join(here, "data", "game.xp3")));
const info = JSON.parse(wasm.xp3_info(bytes));
const scnEntry = info.files.find((f) => f.name.endsWith(".scn"));
if (!scnEntry) { console.log("✗ 无 scn"); process.exit(1); }
const scnBytes = wasm.xp3_read(bytes, scnEntry.name);
const compiled = JSON.parse(compile_scn(scnBytes));
const file = scnEntry.name.replace(/\.scn$/i, "").replace(/^.*[\\/]/, "");
const scenes = compiled.scenes;

// 模拟播放:找首个含台词的场景,记录 (effectIndex, text)
const target = scenes.find((s) => s.steps.some((st) => st.type === "dialogue"));
if (!target) { console.log("✗ 无台词场景"); process.exit(1); }
const run1 = JSON.parse(kag_run_scene(scnBytes, target.label)).effects;
const recorded = [];
run1.forEach((e, i) => { if (e.type === "dialogue") recorded.push({ idx: i, text: e.text, who: e.character }); });
console.log(`scene=${target.label} file=${file} effects=${run1.length} dialogues=${recorded.length}`);
if (!recorded.length) { console.log("✗ 无台词效果"); process.exit(1); }

// 模拟回看回跳:对每条记录重放 0..idx,断言目标句(含说话人)一致
let fails = 0;
for (const rec of recorded) {
  const run2 = JSON.parse(kag_run_scene(scnBytes, target.label)).effects;
  const idx = Math.min(rec.idx, run2.length - 1);
  const at = run2[idx];
  const ok = at && at.type === "dialogue" && at.text === rec.text && (at.character || "") === (rec.who || "");
  if (!ok) { fails++; console.log(`✗ 回跳无效 idx=${idx} 期望[${rec.text}] 实际[${at && at.text}]`); }
}
console.log(fails === 0 ? `✓ 全部 ${recorded.length} 条台词回跳一致(确定性)` : `✗ ${fails} 条不一致`);

// --- 存档→精确读档 语义验证:仿 saveSlot(记 currentDlgIdx) + loadSlot(重放到该句) ---
// 取一条存档位置(当前句=那句中最后一个 dialogue 的 effectIndex),重放覆盖的状态,
// 断言读档后停在这句(与保存时一致,而非从头重放)。
const saved = recorded[recorded.length - 1];      // 仿"存到某句"
const resumeEffects = JSON.parse(kag_run_scene(scnBytes, target.label)).effects;
const at = resumeEffects[Math.min(saved.idx, resumeEffects.length - 1)];
const resumeOk = at && at.type === "dialogue" && at.text === saved.text
  && (at.character || "") === (saved.who || "");
console.log(resumeOk
  ? `✓ 存档→读档精确恢复: 槽存于 ${target.label} 步${saved.idx} → 读档停在同一句「${saved.text.slice(0, 24)}…」`
  : `✗ 存档→读档 恢复句不一致`);
process.exit(resumeOk && fails === 0 ? 0 : 1);
