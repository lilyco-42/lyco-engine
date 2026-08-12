// 缓存故障切换验证:真浏览器会遇到的 IndexedDB 失败模式,
// 确认 getPersist 永不 reject、永不挂起(超时降级为未命中 → 网络兜底)。
let pass = 0, fail = 0;
const ok = (c, m) => { c ? (pass++, console.log("  ✓ " + m)) : (fail++, console.error("  ✗ " + m)); };
const T = (ms) => new Promise((r) => setTimeout(r, ms));
// 每个场景独立模块状态(_db 不串场)
const fresh = async (i) => (await import(`./cache.mjs?fail=${i}`)).AssetCache;

// ---- 场景 1:transaction 同步抛错(连接被关) ----
globalThis.indexedDB = {
  open() {
    const req = {};
    setTimeout(() => {
      req.result = {
        objectStoreNames: { contains: () => true },
        transaction() { throw new Error("连接已关闭"); },
      };
      req.onsuccess?.();
    }, 0);
    return req;
  },
};
let c = new (await fresh(1))();
let val = null, err = null;
try { val = await c.getPersist("k"); } catch (e) { err = e; }
ok(err === null && val === null, `事务抛错 → null 而非 reject(err=${err?.message || "无"})`);

// ---- 场景 2:open 永不回调(挂起) ----
globalThis.indexedDB = {
  open() { return {}; }, // 永不触发任何回调 → 靠 3s 超时
};
c = new (await fresh(2))();
const started = Date.now();
val = null; err = null;
try { val = await c.getPersist("k"); } catch (e) { err = e; }
const elapsed = Date.now() - started;
ok(err === null && val === null, `open 挂起 → ${elapsed}ms 超时降级,不挂起`);
ok(elapsed >= 2500, `确实等了超时(${elapsed}ms≥2.5s)`);

// ---- 场景 3:正常 IDB,getPersist 仍工作(不因加固退化) ----
const store = new Map();
globalThis.indexedDB = {
  open() {
    const req = {};
    setTimeout(() => {
      req.result = {
        objectStoreNames: { contains: () => true },
        transaction() {
          return { objectStore() {
            return {
              get(k) { const r = {}; setTimeout(() => { r.result = store.get(k); r.onsuccess?.(); }, 0); return r; },
              put(v, k) { store.set(k, v); return {}; },
              delete(k) { store.delete(k); return {}; },
              count() { const r = {}; setTimeout(() => { r.result = store.size; r.onsuccess?.(); }, 0); return r; },
            };
          } };
        },
      };
      req.onsuccess?.();
    }, 0);
    return req;
  },
};
c = new (await fresh(3))();
c.set("k", new Uint8Array([9, 9]));
await T(20);
const d = new (await fresh(3))(); // 同场景新实例,读同一 store
const got = await d.getPersist("k");
ok(got !== null && got[0] === 9, `正常 IDB 仍工作:跨实例持久命中 ${got?.[0]}`);

console.log(`\n${pass} passed, ${fail} failed`);
await T(60);
process.exit(fail ? 1 : 0);
