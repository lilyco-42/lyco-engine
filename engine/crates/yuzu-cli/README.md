# yuzu-cli

Project2(柚子社)引擎命令行工具 —— PSB / SCN / PIMG / TLG / XP3 检视与提取。

## 安装

```bash
cargo binstall yuzu-cli    # 预编译(推荐)
# 或
cargo install yuzu-cli
```

## 用法

```bash
# 傻瓜模式:一个 APK 一条命令,自动开浏览器玩
yuzu-cli 千恋万花.apk

# 或直接指定数据目录
yuzu-cli web --data 你的数据目录/

# 检视 / 提取
yuzu-cli xp3 list 归档.xp3
yuzu-cli scn dump 剧本.scn --lang cn
```

完整文档见仓库根 [`README.md`](../../README.md) 与 [`engine/README.md`](../README.md)。
