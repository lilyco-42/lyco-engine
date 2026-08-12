// 端到端冒烟测试:wasm 层(解包 → 解析 → 渲染)在 Node 下验证。
// 与浏览器版同一份 wasm,验证 Rust 逻辑 + wasm-bindgen 编组正确。
import { readFileSync } from "node:fs";
import wasm from "./pkg-node/yuzu_wasm.js";

const {
  version,
  parse_scn,
  compile_scn,
  render_pimg_png,
  xp3_info,
  xp3_read,
} = wasm;

const bytes = (p) => new Uint8Array(readFileSync(p));
let pass = 0, fail = 0;
const ok = (cond, msg) => { cond ? (pass++, console.log("  ✓ " + msg)) : (fail++, console.error("  ✗ " + msg)); };

ok(version().startsWith("yuzu-wasm "), "version()");

// 1) 解包:列出归档
const xp3 = bytes("data/game.xp3");
const info = JSON.parse(xp3_info(xp3));
ok(info.count === 2, `xp3_info 列出 2 个文件 (${info.files.map(f => f.name).join(", ")})`);

// 2) 解包:提取 .scn 并编译
const scn = xp3_read(xp3, "c01c.txt.scn");
ok(scn.length === 326422, `xp3_read .scn 字节数 ${scn.length}`);
const compiled = JSON.parse(compile_scn(scn));
ok(compiled.scenes.length >= 2, `compile_scn 场景数 ${compiled.scenes.length}`);
let dialogues = 0;
for (const s of compiled.scenes) for (const st of s.steps) if (st.type === "dialogue") dialogues++;
ok(dialogues >= 10, `编译出 ${dialogues} 句台词`);

// 3) 解包:提取 .pimg 并渲染为 PNG
const pimg = xp3_read(xp3, "title.pimg");
ok(pimg.length === 1024000, `xp3_read .pimg 字节数 ${pimg.length}`);
const png = render_pimg_png(pimg);
ok(png[0] === 0x89 && png[1] === 0x50 && png[2] === 0x4E && png[3] === 0x47, `render_pimg_png 输出 PNG (${png.length} B)`);

// 4) 剧本摘要
const summary = JSON.parse(parse_scn(scn));
ok(summary.name === "c01c.txt", `parse_scn name=${summary.name}`);

// 5) 真实游戏数据(千恋万花 realgame.xp3)
const real = bytes("data/realgame.xp3");
const realInfo = JSON.parse(xp3_info(real));
ok(realInfo.count === 3, `realgame.xp3 含 3 个文件`);
const realScn = xp3_read(real, "scn/scn001.ks.scn");
ok(realScn.length === 2192468, `真实剧本字节数 ${realScn.length}`);
const realCompiled = JSON.parse(compile_scn(realScn));
ok(realCompiled.scenes[0].label === "*com_part_1", `场景0 = ${realCompiled.scenes[0].label}`);
let realDialogues = 0;
for (const s of realCompiled.scenes)
  for (const st of s.steps) if (st.type === "dialogue") realDialogues++;
ok(realDialogues >= 50, `真实剧本台词 ${realDialogues} 句`);
const bg = xp3_read(real, "bg/room.webp");
ok(bg[0] === 0x52 && bg[8] === 0x57 && bg[9] === 0x45, `背景为 WebP (${bg.length} B)`);
const chara = xp3_read(real, "chara/mizuha.webp");
ok(chara[0] === 0x52 && chara[8] === 0x57, `角色为 WebP (${chara.length} B)`);

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
