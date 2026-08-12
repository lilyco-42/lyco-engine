// 后端契约冒烟:验证 main.js 依赖的 /api 端点(懒加载 → wasm 编译 → 反编译 → 解码)。
// 需 yuzu-server 运行(默认 127.0.0.1:8080, --data realgame)。
const BASE = process.env.YUZU_API || "http://127.0.0.1:8080/api/archives";
import wasm from "./pkg-node/yuzu_wasm.js";
const { compile_scn } = wasm;

async function j(p) { const r = await fetch(p); if (!r.ok) throw new Error(`${p} → HTTP ${r.status}`); return r.json(); }
async function b(p) { const r = await fetch(p); if (!r.ok) throw new Error(`${p} → HTTP ${r.status}`); return new Uint8Array(await r.arrayBuffer()); }

let pass = 0, fail = 0;
const ok = (c, m) => { c ? (pass++, console.log("  ✓ " + m)) : (fail++, console.error("  ✗ " + m)); };

// 1) 归档列表
const { archives } = await j(`${BASE}`);
ok(archives.some((a) => a.name === "data.xp3"), `后端列出归档: ${archives.map((a) => a.name).join(", ")}`);

// 2) 懒加载:data.xp3 文件列表
const files = (await j(`${BASE}/data.xp3/files`)).files;
ok(files.length === 1141, `data.xp3 条目 ${files.length}`);
const scnPath = files.find((f) => f.name.endsWith(".scn")).name;
ok(!!scnPath, `找到剧本 ${scnPath}`);

// 3) 懒加载:提取 scn → wasm 编译 → 台词
const scnBytes = await b(`${BASE}/data.xp3/file?path=${encodeURIComponent(scnPath)}`);
ok(scnBytes.length === 2192468, `scn 字节 ${scnBytes.length}`);
const compiled = JSON.parse(compile_scn(scnBytes));
let dialogues = 0;
for (const s of compiled.scenes) for (const st of s.steps) if (st.type === "dialogue") dialogues++;
ok(dialogues >= 100, `编译台词 ${dialogues} 句`);

// 4) 反编译接口
const text = new TextDecoder().decode(await b(`${BASE}/data.xp3/scn?path=${encodeURIComponent(scnPath)}`));
ok(text.includes("将臣:"), "反编译含真实台词");

// 5) 图像解码(背景 webp 透传)
const bgName = "ヒロイン_芦花の部屋A.png";
const bg = await b(`${BASE}/bgimage1080.xp3/img?path=${encodeURIComponent(bgName)}`);
ok(bg[0] === 0x52 && bg[8] === 0x57, `背景 webp 透传 ${bg.length} B`);

// 6) pimg 服务端解码为 PNG
const pm = await b(`${BASE}/patch_data1080.xp3/img?path=${encodeURIComponent("exchview.pimg")}`);
ok(pm[0] === 0x89 && pm[1] === 0x50, `pimg 服务端解码为 PNG ${pm.length} B`);

// 7) 语音:剧本台词含 voice 引用,voice.xp3 提供 audio/mp4
let withVoice = 0, vref = null;
for (const s of compiled.scenes) for (const st of s.steps)
  if (st.type === "dialogue") { if (st.voice) { withVoice++; if (!vref) vref = st.voice; } }
ok(withVoice > 100, `剧本 ${withVoice} 句台词含语音引用`);
const vres = await fetch(`${BASE}/voice.xp3/file?path=${encodeURIComponent(vref + ".ogg")}`);
ok(vres.ok && vres.headers.get("content-type") === "audio/mp4", `语音 ${vref}.ogg → audio/mp4`);
const vbytes = new Uint8Array(await vres.arrayBuffer());
ok(vbytes[4] === 0x66 && vbytes[5] === 0x74 && vbytes[6] === 0x79 && vbytes[7] === 0x70, `语音为 MP4 (${vbytes.length} B)`);

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
