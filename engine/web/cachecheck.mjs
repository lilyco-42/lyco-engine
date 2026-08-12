// 流式缓存两级验证:
//   1) 内存热层:同实例二次读取零网络;
//   2) 持久层:新实例(模拟新会话)读同一资源走 IndexedDB,仍零网络。
// 用内存版假 IndexedDB 在 node 里模拟浏览器持久存储。
import wasm from "./pkg-node/yuzu_wasm.js";

const { compile_scn } = wasm;
const BASE = "http://127.0.0.1:8080/api/archives";
let pass = 0, fail = 0;
const ok = (c, m) => { c ? (pass++, console.log("  ✓ " + m)) : (fail++, console.error("  ✗ " + m)); };

// ---- 假 IndexedDB(内存实现 cache.mjs 用到的 API) ----
const store = new Map();
globalThis.indexedDB = {
  open() {
    const req = {};
    setTimeout(() => {
      req.result = {
        objectStoreNames: { contains: () => true },
        transaction() {
          return {
            objectStore() {
              return {
                get(k) { const r = {}; setTimeout(() => { r.result = store.get(k); r.onsuccess?.(); }, 0); return r; },
                put(v, k) { store.set(k, v); return { onsuccess: null, onerror: null }; },
                delete(k) { store.delete(k); return {}; },
                count() { const r = {}; setTimeout(() => { r.result = store.size; r.onsuccess?.(); }, 0); return r; },
              };
            },
          };
        },
      };
      req.onsuccess?.();
    }, 0);
    return req;
  },
};

const { AssetCache } = await import("./cache.mjs");

let net = 0;
async function fetchBytes(url) {
  net++;
  const r = await fetch(url);
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return new Uint8Array(await r.arrayBuffer());
}
async function readEntry(cache, source, path) {
  const key = `${source.kind}:${source.name}:${path}`;
  const hit = await cache.getPersist(key);
  if (hit) return hit;
  const bytes = await fetchBytes(`${BASE}/${encodeURIComponent(source.name)}/file?path=${encodeURIComponent(path)}`);
  cache.set(key, bytes);
  return bytes;
}

const src = { kind: "backend", name: "data.xp3" };
const scnPath = "scn\\001・アーサー王ver1.07.ks.scn";

// 1) 实例 A:首次拉取,二次内存命中,网络仅 1
const A = new AssetCache();
const a1 = await readEntry(A, src, scnPath);
const a2 = await readEntry(A, src, scnPath);
const sA = await A.stats();
ok(net === 1 && a1.length === a2.length, `实例A 二次读取:1 次网络,内存命中 ${sA.hits}`);
ok((await A.stats()).persisted === 1, `持久层已落盘 1 项`);

// 2) 实例 B(全新,模拟新会话):从持久层恢复,网络仍 1
const B = new AssetCache();
const b = await readEntry(B, src, scnPath);
const sB = await B.stats();
ok(net === 1 && b.length === a1.length, `新会话零网络:持久命中 ${sB.persistHits},总拉取仍 1`);
ok(sB.entries === 1, `持久命中回填内存热层(entries=${sB.entries})`);

// 3) 复读确认
const b2 = await readEntry(B, src, scnPath);
ok(net === 1 && b2.length === b.length, `实例B 二次读取仍 1 次网络`);

// 4) 启动预热:warmup 把持久层上次播放的剧本拉进新实例内存,零网络
const D = new AssetCache();
const scnKey = `${src.kind}:${src.name}:${scnPath}`;
const warmed = await D.warmup([scnKey]);
ok(warmed === 1 && D.get(scnKey) !== null, `warmup 从持久层预热 1 项入内存`);
ok(net === 1, `预热零网络(总拉取仍 ${net})`);

// 5) 热更新失效:归档 A 变更 → 只清 A 的缓存(A 内存/持久消失,B 保留)
const C = new AssetCache();
const keyA = "backend:data.xp3:scn\\001・アーサー王ver1.07.ks.scn";
const keyB = "backend:bgm.xp3:BGM01.opus";
C.set(keyA, new Uint8Array([1, 2, 3]));
C.set(keyB, new Uint8Array([4, 5, 6]));
const evicted = C.invalidate((k) => k.includes(":data.xp3:"));
const sC = await C.stats();
ok(evicted === 1 && C.get(keyA) === null && C.get(keyB) !== null,
  `invalidate 精确失效 A(${evicted} 条),B 保留`);
ok((await C.getPersist(keyA)) === null, `持久层 A 条目已同步清除`);
ok((await C.getPersist(keyB)) !== null, `持久层 B 条目保留`);

// 6) 服务端版本清单形状(供热更新比对)
const v = await (await fetch("http://127.0.0.1:8080/api/version")).json();
ok(Array.isArray(v.archives) && v.archives.every((a) => a.name && a.file_size && a.modified),
  `api/version 形状正确: v${v.version}, ${v.archives.length} 归档`);

console.log(`\n${pass} passed, ${fail} failed`);
// 排空假 IndexedDB 挂起的 setTimeout,避免 libuv 断言
await new Promise((r) => setTimeout(r, 60));
process.exit(fail ? 1 : 0);
