// node 端验证合成表解析与选层(复刻 main.js 逻辑,无需 canvas)
import wasm from "./pkg-node/yuzu_wasm.js";
const { decode_scenario } = wasm;
const B = "http://127.0.0.1:8080/api/archives/fgimage1080.xp3/file?path=";
async function getStandTable(bodySet) {
  const bytes = new Uint8Array(await (await fetch(B + encodeURIComponent(`${bodySet}.txt`))).arrayBuffer());
  let text = null;
  try { text = decode_scenario(bytes); } catch {}
  if (!text) text = new TextDecoder("utf-8").decode(bytes);
  const rows = [];
  const lines = text.split("\n");
  const header = lines[0].split("\t").map((h) => h.trim().replace(/^#/, ""));
  const col = (name) => header.indexOf(name);
  const ci = { lt: col("layer_type"), nm: col("name"), l: col("left"), t: col("top"),
               w: col("width"), h: col("height"), ty: col("type"), op: col("opacity"),
               v: col("visible"), id: col("layer_id"), grp: col("group_layer_id") };
  for (let i = 1; i < lines.length; i++) {
    const p = lines[i].split("\t");
    if (p.length < 3) continue;
    rows.push({
      name: (p[ci.nm] || "").trim(),
      left: parseInt(p[ci.l]) || 0, top: parseInt(p[ci.t]) || 0,
      layer_type: parseInt(p[ci.lt]) || 0, visible: parseInt(p[ci.v]) === 1,
      layer_id: parseInt(p[ci.id]), group: parseInt(p[ci.grp]) || 0,
    });
  }
  return { bodySet, rows };
}
function selectLayers(table, expression) {
  const face = table.rows.find((r) => r.group === 679 && r.name === expression)
    || table.rows.find((r) => r.group === 679 && r.name === "ベース");
  const body = table.rows.filter((r) => r.layer_type === 0 && r.visible && r.group === 0);
  return face ? [...body, face] : body;
}
const t = await getStandTable("芦花a");
console.log("芦花a 合成表:", t.rows.length, "层");
console.log("默认合成层:", selectLayers(t).map((r) => `${r.name}[${r.layer_id}]@(${r.left},${r.top})`).join("  "));
const smile = selectLayers(t, "笑顔1");
console.log("笑顔1 合成层:", smile.map((r) => `${r.name}[${r.layer_id}]`).join("  "));
// 验证关键层存在
const ids = selectLayers(t).map((r) => r.layer_id);
if (!ids.includes(683) || !ids.includes(684) || !ids.includes(275)) { console.log("!!关键层缺失:", ids); process.exit(1); }
console.log("✓ 默认合成含 私服683 + 腕差分684 + ベース脸275");
