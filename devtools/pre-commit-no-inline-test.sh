#!/usr/bin/env bash
# Git pre-commit hook: 禁止在 crates/*/src/ 下提交包含 #[cfg(test)] 或 mod tests { 的代码
# 检查暂存区中文件的完整内容（不仅仅是 diff），确保零内嵌测试。

set -euo pipefail

# 获取暂存区中存在的 src/ 下 .rs 文件（A=新增, M=修改, C=复制）
STAGED_RS=$(git diff --cached --name-only --diff-filter=AMC | grep -E '^crates/.*/src/.*\.rs$' || true)

if [ -z "$STAGED_RS" ]; then
    exit 0
fi

VIOLATIONS=""

for file in $STAGED_RS; do
    # 检查暂存区中文件的完整内容
    if git show ":$file" | grep -qE '#\[cfg\(test\)\]|mod tests \{'; then
        VIOLATIONS="$VIOLATIONS\n  $file"
    fi
done

if [ -n "$VIOLATIONS" ]; then
    echo "ERROR: 内嵌测试代码检测到 (#[cfg(test)] 或 mod tests {)"
    echo "以下文件包含内嵌测试:"
    echo -e "$VIOLATIONS"
    echo ""
    echo "请将测试代码移到 crates/<crate>/tests/ 目录下。"
    exit 1
fi
