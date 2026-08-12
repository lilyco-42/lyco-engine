#!/usr/bin/env bash
# 重建 yuzu-web 的 wasm 绑定、yuzu-server 后端,并跑 Node 冒烟。
# 前置:cargo + wasm32-unknown-unknown target + wasm-bindgen-cli 0.2.127
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build -p yuzu-wasm --target wasm32-unknown-unknown --release
cargo build -p yuzu-server

wasm-bindgen target/wasm32-unknown-unknown/release/yuzu_wasm.wasm \
  --out-dir web/pkg --target web --no-typescript
wasm-bindgen target/wasm32-unknown-unknown/release/yuzu_wasm.wasm \
  --out-dir web/pkg-node --target nodejs --no-typescript

echo "--- 冒烟:wasm 端到端 ---"
(cd web && node smoke.mjs)

echo "--- 冒烟:后端契约(需 yuzu-server 已运行,127.0.0.1:8080)---"
(cd web && node backend-smoke.mjs) || echo "  (跳过:后端未运行。启动: yuzu-server --data realgame --web web)"

echo "--- 冒烟:全流程(需 yuzu-server 已运行)---"
(cd web && node backend-fullflow.mjs) || echo "  (跳过:后端未运行)"

echo "--- 冒烟:播放器入口流(需 yuzu-server 已运行)---"
(cd web && node player-entry-check.mjs) || echo "  (跳过:后端未运行)"

echo "--- 冒烟:场景图章节地图 ---"
(cd web && node chart-map-check.mjs) || echo "  (跳过)"

echo "--- 冒烟:章节导航 ---"
(cd web && node chapter-nav-check.mjs) || echo "  (跳过)"

echo "--- 冒烟:全选择点解析 ---"
(cd web && node choices-check.mjs) || echo "  (跳过)"
