#!/usr/bin/env bash
# lyco CLI 冒烟测试：CLI 表面 + 模板管理 + 生成正确性 + 错误路径。
# 使用隔离的 LYCO_CONFIG/LYCO_CACHE，不会触碰真实配置。
# 注：交互 TTY 提示路径需要真实 pty，未纳入本脚本（非 TTY 自动回退默认值已覆盖）。
set -u

BIN="${LYCO_BIN:-/d/Code/lyco/target/debug/lyco.exe}"
WORK="$(mktemp -d)"
CACHE_DIR="$(cd "$(dirname "$0")/.." && pwd)/target/.smoke-cache"
export LYCO_CONFIG="$WORK/lyco.yaml"
export LYCO_CACHE="$CACHE_DIR"
mkdir -p "$CACHE_DIR"
cd "$WORK"

PASS=0; FAIL=0
note()  { echo "  · $1"; }
ok()    { PASS=$((PASS+1)); echo "  ✓ $1"; }
bad()   { FAIL=$((FAIL+1)); echo "  ✗ $1"; }

# rc_check <desc> <expected_rc> <actual_rc>
rc_check() { if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (rc=$3, 期望=$2)"; fi; }
# contains <desc> <file_or_empty> <pattern> ; 空 file 表示用变量 $OUT
contains() {
  local hay="$2"
  if [ -z "$hay" ]; then hay="$OUT"; fi
  if printf '%s' "$hay" | grep -q "$3"; then ok "$1"; else bad "$1 (缺: $3)"; fi
}
# run <desc> <expected_rc> <args...>
run() {
  local desc="$1"; local want="$2"; shift 2
  OUT="$("$BIN" "$@" 2>&1)"; local rc=$?
  rc_check "$desc" "$want" "$rc"
}

if [ ! -x "$BIN" ]; then echo "✗ 二进制不存在: $BIN（先 cargo build）"; exit 1; fi

echo "== CLI 表面 =="
run "lyco --help" 0 --help
contains "--help 列出子命令" "" "template"
contains "--help 列出子命令" "" "new"
run "lyco -V" 0 -V
contains "版本号" "" "lyco"
run "lyco new --help" 0 new --help
run "lyco template --help" 0 template --help
run "未知子命令报错" 2 bogus-cmd
run "缺 name 参数报错" 2 new

echo "== config path =="
run "config path" 0 config path
contains "打印配置路径(含覆盖名)" "" "lyco.yaml"
# 首次运行后应生成默认配置
run "首次 template list 生成默认配置" 0 template list
contains "含 rust-android" "" "rust-android"
contains "含 fabric-mod" "" "fabric-mod"
[ -f "$LYCO_CONFIG" ] && ok "配置文件已生成" || bad "配置文件未生成"

echo "== template list =="
run "template list" 0 template list
contains "本地标题" "" "本地"
run "template list --remote (gh api)" 0 template list --remote
contains "远程候选标题" "" "远程"

echo "== template add =="
run "add 合法仓库" 0 template add lilyco-42/minigrep-cn
contains "注册成功" "" "已注册"
run "add 重复覆盖" 0 template add lilyco-42/minigrep-cn --name minigrep-cn
run "add 无斜杠" 1 template add not-a-repo
run "add 不存在的仓库" 1 template add nobody/definitely-not-real-xyz
run "add 带 --name" 0 template add lilyco-42/minigrep-cn --name mg
run "list 出现新模板" 0 template list
contains "新模板 mg 在列表" "" "mg"
run "remove mg" 0 template remove mg
run "remove 未注册" 1 template remove mg

echo "== template add-local =="
mkdir -p localtpl/sub
printf 'hello\n' > localtpl/hello.txt
printf 'x' > localtpl/sub/data.bin
run "add-local 合法目录" 0 template add-local "$WORK/localtpl" --name localtpl
run "add-local 不存在目录" 1 template add-local /no/such/dir --name nope

echo "== new: 默认/参数/错误 =="
run "new 未知模板" 1 new x --template nope --yes
run "new 默认 rust-android" 0 new app1 --yes
contains "生成了 app1" "" "已生成"
[ -d app1 ] && ok "app1 目录存在" || bad "app1 目录缺失"
run "new 重复目录拒绝" 1 new app1 --yes
run "new --force 覆盖" 0 new app1 --force --yes
run "new --output 自定义目录" 0 new app1 --output custom_out --yes
[ -d custom_out ] && ok "custom_out 存在" || bad "custom_out 缺失"
run "new 畸形 --param" 1 new p1 --param badparam --yes
run "new 指定模板 rust-android" 0 new p2 --template rust-android --param package=com.t.p2 --yes
run "new --refresh 强制重下" 0 new p3 --template rust-android --yes --refresh
run "new fabric-mod" 0 new mod1 --template fabric-mod --param mod_id=mod1 --param package=com.t.mod1 --yes
run "new 本地模板" 0 new lp1 --template localtpl --yes

echo "== 生成内容正确性 =="
[ -f p2/app/src/main/java/com/t/p2/MainActivity.kt ] && ok "rust 包目录重命名" || bad "rust 包目录未重命名"
grep -q "package com.t.p2" p2/app/src/main/java/com/t/p2/MainActivity.kt && ok "包名替换" || bad "包名未替换"
grep -q "Java_com_t_p2" p2/native_lib/src/lib.rs && ok "JNI 名替换" || bad "JNI 名未替换"
grep -q "namespace = \"com.t.p2\"" p2/app/build.gradle.kts && ok "namespace 替换" || bad "namespace 未替换"
if grep -rq "com.example.app" p2 --include="*" 2>/dev/null; then bad "rust 残留占位符"; else ok "rust 无残留占位符"; fi
[ ! -d p2/app/src/main/java/com/example ] && ok "rust 空目录已清理" || bad "rust 空目录残留"
[ -f mod1/src/main/resources/mod1.mixins.json ] && ok "fabric mixin 文件重命名" || bad "fabric mixin 未重命名"
[ -f mod1/src/main/java/com/t/mod1/Mod1Mod.java ] && ok "fabric 主类重命名" || bad "fabric 主类未重命名"
grep -q '"id": "mod1"' mod1/src/main/resources/fabric.mod.json && ok "fabric id 替换" || bad "fabric id 未替换"
grep -q 'com.t.mod1.Mod1Mod' mod1/src/main/resources/fabric.mod.json && ok "fabric entrypoint 替换" || bad "fabric entrypoint 未替换"
if grep -rq "com.example.template\|template-mod\|TemplateMod" mod1/src/main --include="*" 2>/dev/null; then bad "fabric 残留占位符"; else ok "fabric 无残留占位符"; fi
[ -f lp1/hello.txt ] && ok "本地纯拷贝文件" || bad "本地纯拷贝缺失"
cmp -s localtpl/sub/data.bin lp1/sub/data.bin && ok "二进制原样拷贝" || bad "二进制被改动"

echo "== template.yaml 检测 =="
mkdir -p mtest/src/com/a/b
printf 'package com.a.b\n' > mtest/src/com/a/b/F.java
cat > mtest/template.yaml <<'EOF'
id: mtest
params:
  - key: package
    label: 包名
    default: com.a.b
replace:
  - find: com.a.b
    with: "{{package}}"
rename:
  - from: src/com/a/b
    to: "src/{{package|path}}"
EOF
run "add-local 带 template.yaml" 0 template add-local "$WORK/mtest" --name mtest
run "new 使用清单" 0 new mdemo --template mtest --param package=com.m.demo --yes
[ -f mdemo/src/com/m/demo/F.java ] && ok "清单重命名生效" || bad "清单重命名未生效"
grep -q "package com.m.demo" mdemo/src/com/m/demo/F.java && ok "清单替换生效" || bad "清单替换未生效"

echo "== template refresh =="
run "refresh 指定模板" 0 template refresh rust-android
run "refresh 全部" 0 template refresh
run "refresh 未知模板" 1 template refresh nope

echo "== 深水区: CJK/.gitkeep/过滤器链/参数引用/纯拷贝 =="
head -1 p2/app/src/main/java/com/t/p2/MainActivity.kt | grep -q "换成你的包名" && ok "CJK 注释保留" || bad "CJK 注释丢失"
mkdir -p keeptpl/empty-dir
touch keeptpl/empty-dir/.gitkeep
printf 'x' > keeptpl/keep.txt
run "add-local .gitkeep 模板" 0 template add-local "$WORK/keeptpl" --name keeptpl
run "new .gitkeep 模板" 0 new kp --template keeptpl --yes
[ -f kp/empty-dir/.gitkeep ] && ok ".gitkeep 空目录保留" || bad ".gitkeep 目录被清理"
mkdir -p filtpl/src
cat > filtpl/template.yaml <<'EOF'
id: filtpl
params:
  - key: package
    default: com.acme.sub.app
replace:
  - find: GROUPPATH
    with: "{{package|parent|path}}"
  - find: RUSTPKG
    with: "{{package|rust|upper}}"
EOF
printf 'GROUP=GROUPPATH RUST=RUSTPKG\n' > filtpl/src/g.txt
run "add-local 过滤器模板" 0 template add-local "$WORK/filtpl" --name filtpl
run "new 过滤器模板" 0 new fl --template filtpl --yes
grep -q "GROUP=com/acme/sub RUST=COM_ACME_SUB_APP" fl/src/g.txt && ok "过滤器链生效" || bad "过滤器链失效"
run "new 参数引用其他参数" 0 new refp --template rust-android --param package=com.refp.app --param "appname={{package|parent}}" --yes
grep -q "app_name\">com.refp<" refp/app/src/main/res/values/strings.xml && ok "参数引用解析" || bad "参数引用未解析"
run "add 无清单仓库" 0 template add lilyco-42/minigrep-cn
run "new 无清单仓库(纯拷贝)" 0 new mgc --template minigrep-cn --yes
[ -f mgc/src/main.rs ] && ok "无清单仓库纯拷贝" || bad "无清单仓库纯拷贝失败"

echo "== 边界/健壮性 =="
mkdir -p reqtpl && printf 'x\n' > reqtpl/r.txt
cat > reqtpl/template.yaml <<'EOF'
id: reqtpl
params:
  - key: must
    label: 必填
    default: ""
    required: true
EOF
run "add-local 必填模板" 0 template add-local "$WORK/reqtpl" --name reqtpl
run "必填参数为空报错" 1 new r1 --template reqtpl --yes
echo "templates: [broken" > "$WORK/bad.yaml"
OUT=$(LYCO_CONFIG="$WORK/bad.yaml" "$BIN" template list 2>&1); rc_check "损坏配置报错" 1 "$?"
mkdir -p failtpl && printf 'x\n' > failtpl/f.txt
cat > failtpl/template.yaml <<'EOF'
id: failtpl
params: []
post:
  - command: sh
    args: ["-c", "exit 3"]
EOF
run "add-local 失败钩子模板" 0 template add-local "$WORK/failtpl" --name failtpl
run "post 钩子失败报错" 1 new f1 --template failtpl --yes
run "new 空名报错" 1 new "" --yes
mkdir -p deltpl/docs/nested
printf 'keep\n' > deltpl/keep.txt
printf 'g\n' > deltpl/docs/guide.md
printf 'd\n' > deltpl/docs/nested/deep.txt
cat > deltpl/template.yaml <<'EOF'
id: deltpl
params: []
delete:
  - docs/
EOF
run "add-local 目录删除模板" 0 template add-local "$WORK/deltpl" --name deltpl
run "new 目录删除模板" 0 new dd --template deltpl --yes
if [ -f dd/keep.txt ] && [ ! -e dd/docs ]; then ok "delete 目录规则级联"; else bad "delete 目录规则未级联"; fi
mkdir -p cwdok/sub && printf 'keep\n' > cwdok/sub/inner.txt
cat > cwdok/template.yaml <<'EOF'
id: cwdok
params: []
post:
  - command: sh
    args: ["-c", "echo inner > marker.txt"]
    cwd: sub
EOF
run "add-local cwd 钩子模板" 0 template add-local "$WORK/cwdok" --name cwdok
run "new cwd 钩子模板" 0 new ck --template cwdok --yes
[ -f ck/sub/marker.txt ] && ok "post 钩子 cwd 生效" || bad "post 钩子 cwd 未生效"
mkdir -p badcwd && printf 'x\n' > badcwd/b.txt
cat > badcwd/template.yaml <<'EOF'
id: badcwd
params: []
post:
  - command: sh
    args: ["-c", "true"]
    cwd: no-such-dir
EOF
run "add-local 缺失cwd模板" 0 template add-local "$WORK/badcwd" --name badcwd
run "缺失 cwd 报错" 1 new bk --template badcwd --yes

echo "== post 钩子 / next_steps / 边界 =="
mkdir -p posttpl
printf 'data\n' > posttpl/data.txt
cat > posttpl/template.yaml <<'EOF'
id: posttpl
params: []
post:
  - command: sh
    args: ["-c", "echo done > marker.txt"]
EOF
run "add-local post 模板" 0 template add-local "$WORK/posttpl" --name posttpl
run "new post 模板" 0 new ptest --template posttpl --yes
[ -f ptest/marker.txt ] && ok "post 钩子执行" || bad "post 钩子未执行"
grep -q "done" ptest/marker.txt && ok "post 钩子产物内容" || bad "post 钩子产物内容错误"
run "new next_steps 解析" 0 new nsapp --template rust-android --yes
contains "next_steps 替换 {{name}}" "" "cd nsapp/native_lib"
run "new 未知 --param 不崩溃" 0 new up1 --template rust-android --param bogus=1 --yes
run "new 小写 kebab 名" 0 new kebab-name --template rust-android --param package=com.t.k --yes
[ -d kebab-name ] && ok "kebab 名目录" || bad "kebab 名目录缺失"

echo "== 清理 =="
run "remove minigrep-cn" 0 template remove minigrep-cn
run "remove localtpl" 0 template remove localtpl
run "remove mtest" 0 template remove mtest
run "remove posttpl" 0 template remove posttpl
run "remove keeptpl" 0 template remove keeptpl
run "remove filtpl" 0 template remove filtpl
run "remove reqtpl" 0 template remove reqtpl
run "remove failtpl" 0 template remove failtpl
run "remove deltpl" 0 template remove deltpl
run "remove cwdok" 0 template remove cwdok
run "remove badcwd" 0 template remove badcwd
run "保留默认模板 rust-android" 0 template list
contains "rust-android 仍在" "" "rust-android"
contains "fabric-mod 仍在" "" "fabric-mod"

echo ""
echo "======================================"
echo "  PASS=$PASS  FAIL=$FAIL"
echo "======================================"
rm -rf "$WORK"
[ "$FAIL" -eq 0 ]
