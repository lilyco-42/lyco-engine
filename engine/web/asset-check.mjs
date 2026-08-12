// 验证命令驱动换图的资产解析逻辑(与 main.js 一致,打真实后端)。
const BASE = "http://127.0.0.1:8080/api/archives";
const IMAGE_ARCHIVES = ["bgimage1080.xp3", "fgimage1080.xp3", "evimage1080.xp3", "patch_data1080.xp3"];
const index = new Map();
let pass = 0, fail = 0;
const ok = (c, m) => { c ? (pass++, console.log("  ✓ " + m)) : (fail++, console.error("  ✗ " + m)); };

for (const arc of IMAGE_ARCHIVES) {
  try {
    const { files } = await (await fetch(`${BASE}/${encodeURIComponent(arc)}/files`)).json();
    for (const f of files) {
      const key = f.name.replace(/\.(png|webp|jpg|jpeg|gif|bmp|tlg)$/i, "").toLowerCase();
      if (!index.has(key)) index.set(key, { archive: arc, path: f.name, size: f.size });
    }
  } catch {
    console.log(`  · 归档 ${arc} 缺失,跳过索引`);
  }
}
ok(index.size > 1800, `资源索引 ${index.size} 项`);

const resolveBg = async (asset) => index.get(asset.toLowerCase()) || null;
async function resolveChara(character) {
  const prefix = character.toLowerCase() + "a";
  let best = null;
  for (const [k, v] of index)
    if (k.startsWith(prefix) && k !== prefix && !k.includes("_0_") && (!best || v.size > best.size)) best = v;
  return best;
}

const cases = [
  ["背景 神社_神社内（刀）A", await resolveBg("神社_神社内（刀）A"), "bgimage1080.xp3"],
  ["背景 ヒロイン_茉子の部屋D", await resolveBg("ヒロイン_茉子の部屋D"), "bgimage1080.xp3"],
  ["背景 画面_黒", await resolveBg("画面_黒"), "bgimage1080.xp3"],
  ["立绘 芦花.stand", await resolveChara("芦花"), "fgimage1080.xp3"],
  ["立绘 レナ.stand", await resolveChara("レナ"), "fgimage1080.xp3"],
  ["立绘 茉子.stand", await resolveChara("茉子"), "fgimage1080.xp3"],
];
for (const [label, hit, expectArc] of cases) {
  ok(!!hit && hit.archive === expectArc, `${label} → ${hit ? hit.path + " (" + hit.size + "B)" : "未找到"}`);
}

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
