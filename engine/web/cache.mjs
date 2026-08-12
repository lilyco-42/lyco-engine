// 流式加载缓存 —— 两级:内存 LRU(热)+ IndexedDB(持久)。
// 读:先内存,未命中再查 IndexedDB(异步回填内存);写:内存 + 异步落盘。
// 跨会话复用:同一 (源, 路径) 的剧本/解码图像/BGM/语音只从后端拉一次。
const DB_NAME = "yuzu-stream-cache";
const STORE = "assets";
const KEY = "cache-key";
const DEFAULT_MAX_BYTES = 64 * 1024 * 1024;

let _db = null;
async function openDb() {
  if (_db) return _db;
  if (typeof indexedDB === "undefined") return null; // 非浏览器(Node 测试)降级为纯内存
  try {
    return await new Promise((resolve) => {
      let done = false;
      const timer = setTimeout(() => finish(null), 3000); // 打开超时 → 视为无持久层
      const finish = (db) => {
        if (done) return;
        done = true;
        clearTimeout(timer);
        if (db) _db = db;
        resolve(db || null);
      };
      let req;
      try {
        req = indexedDB.open(DB_NAME, 1);
      } catch { finish(null); return; }
      req.onupgradeneeded = () => {
        try { if (!req.result.objectStoreNames.contains(STORE)) req.result.createObjectStore(STORE); } catch {}
      };
      req.onsuccess = () => finish(req.result);
      req.onerror = () => finish(null);
      req.onblocked = () => finish(null); // 升级被其它标签页阻塞 → 立即降级为无持久层
    });
  } catch { return null; }
}
async function dbGet(key) {
  const db = await openDb();
  if (!db) return null;
  return new Promise((resolve) => {
    let settled = false;
    const fin = (v) => { if (!settled) { settled = true; clearTimeout(timer); resolve(v); } };
    const timer = setTimeout(() => fin(null), 1500); // 兜底:事务永不回调也不挂起
    try {
      const tx = db.transaction(STORE, "readonly");
      const req = tx.objectStore(STORE).get(key);
      req.onsuccess = () => fin(req.result ? new Uint8Array(req.result) : null);
      req.onerror = () => fin(null);
    } catch { fin(null); } // 连接被关等同步异常 → 按未命中处理
  });
}
async function dbPut(key, bytes) {
  const db = await openDb();
  if (!db) return;
  try {
    const tx = db.transaction(STORE, "readwrite");
    tx.objectStore(STORE).put(bytes, key);
  } catch { /* 磁盘满/连接关闭等静默 */ }
}
async function dbDelete(key) {
  const db = await openDb();
  if (!db) return;
  try {
    const tx = db.transaction(STORE, "readwrite");
    tx.objectStore(STORE).delete(key);
  } catch { /* 忽略 */ }
}
async function dbCount() {
  const db = await openDb();
  if (!db) return 0;
  return new Promise((resolve) => {
    let settled = false;
    const fin = (v) => { if (!settled) { settled = true; clearTimeout(timer); resolve(v); } };
    const timer = setTimeout(() => fin(0), 1500);
    try {
      const tx = db.transaction(STORE, "readonly");
      const req = tx.objectStore(STORE).count();
      req.onsuccess = () => fin(req.result);
      req.onerror = () => fin(0);
    } catch { fin(0); }
  });
}

export class AssetCache {
  constructor({ maxBytes = DEFAULT_MAX_BYTES, persist = true } = {}) {
    this.maxBytes = maxBytes;
    this.persist = persist;
    this.map = new Map(); // key → { bytes, size, at }(内存热层)
    this.used = 0;
    this.hits = 0;
    this.misses = 0;
    this.persistHits = 0;
    this.savedBytes = 0;
  }

  /// 同步取内存热层;异步回填由 caller 负责时用 get。读 IndexedDB 请用 getOrFetch 或 await getPersist。
  get(key) {
    const v = this.map.get(key);
    if (!v) { this.misses++; return null; }
    v.at = performance.now();
    this.map.delete(key);
    this.map.set(key, v); // 重插到末尾 → LRU
    this.hits++;
    this.savedBytes += v.size;
    return v.bytes;
  }

  /// 持久层异步取:内存未命中时查 IndexedDB,命中则回填内存。
  /// 永不抛错、永不挂起:任何异常/超时都视为未命中,网络作兜底。
  async getPersist(key) {
    try {
      const hit = this.get(key);
      if (hit) return hit;
      if (!this.persist) return null;
      const bytes = await dbGet(key);
      if (bytes) {
        this.persistHits++;
        this.set(key, bytes, { skipPersist: true });
      }
      return bytes;
    } catch { return null; }
  }

  set(key, bytes, { skipPersist = false } = {}) {
    if (bytes.length > this.maxBytes) return; // 单文件超容量不缓存
    const old = this.map.get(key);
    if (old) this.used -= old.size;
    this.map.delete(key);
    this.map.set(key, { bytes, size: bytes.length, at: performance.now() });
    this.used += bytes.length;
    while (this.used > this.maxBytes && this.map.size) {
      const k = this.map.keys().next().value; // 头部即最久未用
      const v = this.map.get(k);
      this.used -= v.size;
      this.map.delete(k);
      if (this.persist) dbDelete(k); // 内存逐出时同步清持久条目(避免陈旧)
    }
    if (this.persist && !skipPersist) dbPut(key, bytes); // 异步落盘
  }

  async warmup(keys) {
    // 会话启动预热:把常用键从持久层拉到内存热层
    const got = await Promise.all(keys.map((k) => this.getPersist(k)));
    return got.filter(Boolean).length;
  }

  /// 热更新:按谓词失效缓存条目(内存 + 持久)。服务端归档版本变更时清陈旧字节。
  invalidate(predicate) {
    let removed = 0;
    for (const k of this.map.keys()) {
      if (predicate(k)) {
        const v = this.map.get(k);
        this.used -= v.size;
        this.map.delete(k);
        removed++;
        if (this.persist) dbDelete(k);
      }
    }
    return removed;
  }

  async stats() {
    const entries = this.map.size;
    const persisted = this.persist ? await dbCount() : 0;
    return {
      hits: this.hits,
      misses: this.misses,
      persistHits: this.persistHits,
      saved: this.savedBytes,
      used: this.used,
      max: this.maxBytes,
      entries,
      persisted,
    };
  }
}
