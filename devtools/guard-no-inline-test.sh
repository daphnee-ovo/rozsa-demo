#!/usr/bin/env bash
# PreToolUse hook: 禁止在 src/ 下写入 #[test] / #[cfg(test)] 内嵌测试
# 通过 stdin 接收 Claude Code 传入的 JSON（不依赖 jq）

set -euo pipefail

INPUT=$(cat)

# 提取 file_path（简易 JSON 提取，适配 Write/Edit 两种格式）
FILE=$(echo "$INPUT" | grep -oP '"file_path"\s*:\s*"\K[^"]+' | head -1)
[ -z "$FILE" ] && exit 0

# 只关心 src/ 下的 .rs 文件
case "$FILE" in
  */src/*.rs) ;;
  *) exit 0 ;;
esac

# 提取写入内容（Write 用 content，Edit 用 new_string）
# 由于 content/new_string 可能包含换行和转义，用 Python 做简单提取
CONTENT=$(python3 -c "
import json, sys
data = json.loads(sys.stdin.read())
ti = data.get('tool_input', data)
print(ti.get('content', '') or ti.get('new_string', ''))
" <<< "$INPUT" 2>/dev/null) || exit 0

[ -z "$CONTENT" ] && exit 0

# 检查是否包含内嵌测试标记
if echo "$CONTENT" | grep -qE '#\[test\]|#\[cfg\(test\)\]'; then
  echo '{"permissionDecision":"deny","message":"禁止在 src/ 中写内嵌测试（#[test] / #[cfg(test)]）。请将测试写在 tests/ 目录下。"}'
  exit 0
fi
