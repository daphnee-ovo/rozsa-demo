# 模型配置 — `~/.rozsa/models/*.json`

Rózsa 通过扫描 `~/.rozsa/models/` 目录下的 JSON 文件加载可用模型。
每个文件定义一组 provider 及其模型列表。

## 文件格式

```json
{
  "providers": {
    "<provider-name>": {
      "baseUrl": "https://api.example.com/v1",
      "apiKey": "$ENV_VAR_NAME 或直接值",
      "api": "openai-completions",
      "models": [
        {
          "id": "model-id",
          "name": "显示名称",
          "contextWindow": 128000,
          "maxTokens": 16384,
          "reasoning": false,
          "thinkingEffortMap": {
            "low": "light",
            "high": null
          },
          "input": ["text", "image"]
        }
      ]
    }
  }
}
```

## 字段说明

### Provider 级

| 字段 | 必填 | 说明 |
|------|------|------|
| `baseUrl` | 自定义 provider 必填 | API endpoint |
| `apiKey` | 自定义 provider 必填 | `$NAME` 环境变量引用，或直接值；禁止 shell 命令 |
| `api` | 自定义 provider 必填 | 协议类型（见下方） |
| `headers` | 否 | 额外 HTTP headers |
| `compat` | 否 | 兼容性标志 |
| `models` | 否 | 模型定义列表 |
| `modelOverrides` | 否 | 按 model ID 覆盖已有模型字段 |

### Model 级

| 字段 | 必填 | 默认值 |
|------|------|--------|
| `id` | 是 | — |
| `name` | 否 | 同 id |
| `api` | 否 | 继承 provider 级 |
| `baseUrl` | 否 | 继承 provider 级 |
| `reasoning` | 否 | false |
| `thinkingEffortMap` | 否 | 模型的逻辑思考强度到 provider 请求值的映射；`null` 表示该档不可用 |
| `input` | 否 | ["text"] |
| `contextWindow` | 否 | 128000 |
| `maxTokens` | 否 | 16384 |
| `cost` | 否 | 全 0 |
| `headers` | 否 | — |
| `compat` | 否 | — |

### 思考强度与自动学习

界面与配置使用统一名称 **thinking effort**。逻辑档位固定为：`off`、`low`、`medium`、`high`、`xhigh`、`max`。`reasoning: false` 的模型只能使用 `off`；支持 reasoning 且未配置 `thinkingEffortMap` 的模型默认可选择全部六档。界面的 slider 只显示当前模型实际可用的档位，并会将被禁用的档位压缩掉，不保留空位。

切换模型时，如果当前 thinking effort 在新模型上不可用，Rózsa 会按档位顺序向下回退到最近的可用档位：`max` → `xhigh` → `high` → `medium` → `low` → `off`。例如当前为 `max` 时，新模型支持 `xhigh` 就切换到 `xhigh`，否则继续回退到 `high`。

`low` 的 provider 请求值依次尝试 `low`、`light`、`minimal`；其他非关闭档只尝试其同名值。只有 provider 明确以 HTTP 400 或 422 表示该思考强度不受支持时，Rózsa 才会进行上述重试。认证、配额、网络、模型不存在等错误不会触发重试，也不会改写配置。

若请求成功，实际成功的值会写回用户级 `~/.rozsa/models/*.json` 对应模型的 `thinkingEffortMap`。若某一逻辑档位的全部候选值均被明确拒绝，则写入 `null`，之后不会再向 API 发送该档请求。例如：

```json
"thinkingEffortMap": {
  "low": "light",
  "high": null
}
```

这里 `low` 发送给 provider 的值为 `light`，而 `high` 不可用。旧配置键 `thinkingLevelMap` 会在读取时兼容，但新写入一律使用 `thinkingEffortMap`。

### 协议类型 (`api`)

| 值 | 说明 |
|----|------|
| `anthropic-messages` | Anthropic Messages API |
| `openai-completions` | OpenAI Chat Completions (兼容大多数第三方) |
| `openai-responses` | OpenAI Responses API |
| `bedrock-converse-stream` | AWS Bedrock Converse Stream |

## 示例

### `~/.rozsa/models/anthropic.json`

```json
{
  "providers": {
    "anthropic": {
      "baseUrl": "https://api.anthropic.com",
      "apiKey": "$ANTHROPIC_API_KEY",
      "api": "anthropic-messages",
      "models": [
        {
          "id": "claude-sonnet-4-20250514",
          "name": "Claude Sonnet 4",
          "contextWindow": 200000,
          "maxTokens": 16384,
          "reasoning": true,
          "input": ["text", "image"],
          "cost": { "input": 3, "output": 15, "cacheRead": 0.3, "cacheWrite": 3.75 }
        }
      ]
    }
  }
}
```

### `~/.rozsa/models/deepseek.json`

```json
{
  "providers": {
    "deepseek": {
      "baseUrl": "https://api.deepseek.com/v1",
      "apiKey": "$DEEPSEEK_API_KEY",
      "api": "openai-completions",
      "models": [
        {
          "id": "deepseek-chat",
          "name": "DeepSeek V3",
          "contextWindow": 65536,
          "maxTokens": 8192,
          "reasoning": false,
          "input": ["text"],
          "cost": { "input": 0.27, "output": 1.1, "cacheRead": 0.07, "cacheWrite": 0.27 }
        },
        {
          "id": "deepseek-reasoner",
          "name": "DeepSeek R1",
          "contextWindow": 65536,
          "maxTokens": 8192,
          "reasoning": true,
          "input": ["text"],
          "cost": { "input": 0.55, "output": 2.19, "cacheRead": 0.14, "cacheWrite": 0.55 }
        }
      ]
    }
  }
}
```

### `~/.rozsa/models/minimax.json`

```json
{
  "providers": {
    "minimax": {
      "baseUrl": "https://api.minimax.chat/v1",
      "apiKey": "$MINIMAX_API_KEY",
      "api": "openai-completions",
      "models": [
        {
          "id": "abab6.5s-chat",
          "name": "MiniMax abab6.5s",
          "contextWindow": 245760,
          "maxTokens": 16384,
          "input": ["text"]
        }
      ]
    }
  }
}
```

### `~/.rozsa/models/bedrock.json`

```json
{
  "providers": {
    "amazon-bedrock": {
      "baseUrl": "https://bedrock-runtime.us-east-1.amazonaws.com",
      "api": "bedrock-converse-stream",
      "models": [
        {
          "id": "us.anthropic.claude-sonnet-4-20250514-v1:0",
          "name": "Claude Sonnet 4 (Bedrock)",
          "contextWindow": 200000,
          "maxTokens": 16384,
          "reasoning": true,
          "input": ["text", "image"],
          "cost": { "input": 3, "output": 15, "cacheRead": 0.3, "cacheWrite": 3.75 }
        }
      ]
    }
  }
}
```

### `~/.rozsa/models/openrouter.json`

```json
{
  "providers": {
    "openrouter": {
      "baseUrl": "https://openrouter.ai/api/v1",
      "apiKey": "$OPENROUTER_API_KEY",
      "api": "openai-completions",
      "headers": {
        "HTTP-Referer": "https://rozsa.dev",
        "X-Title": "Rozsa"
      },
      "models": [
        {
          "id": "anthropic/claude-sonnet-4",
          "name": "Claude Sonnet 4 (OpenRouter)",
          "contextWindow": 200000,
          "maxTokens": 16384,
          "reasoning": true,
          "input": ["text", "image"],
          "cost": { "input": 3, "output": 15, "cacheRead": 0.3, "cacheWrite": 3.75 }
        }
      ]
    }
  }
}
```

## 加载顺序与优先级

加载顺序（后加载覆盖先加载）：

1. **用户级**：`~/.rozsa/models/*.json`
2. **项目级**：`<project>/.rozsa/models/*.json`

项目级配置优先于用户级 — 同名 provider/model 会被项目级覆盖。

每个目录内的文件按文件名字母序加载。

推荐命名：`<provider>.json`（如 `anthropic.json`、`deepseek.json`）。

## apiKey 格式

- 环境变量引用：`"apiKey": "$DEEPSEEK_API_KEY"` — 只有以 `$` 开头的值才会读取环境变量。
- 环境变量查找顺序：Rózsa 先读取进程环境，再读取 `~/.rozsa/.env`，最后在 Unix 系统读取默认 shell 启动文件中的字面量赋值。macOS 默认读取 zsh 启动文件；bash 读取 `.bash_profile`、`.bash_login`、`.profile`、`.bashrc`；fish 读取 `~/.config/fish/config.fish`。Windows 使用用户/系统环境变量的进程继承，不解析 PowerShell profile。`.env` 只由 Rózsa 读取，不会自动导出到 shell。
- 直接值：`"apiKey": "sk-xxx"` — 仅用于迁移已有配置。首次读取文件配置时，Rózsa 会把值写入 `~/.rozsa/.env`，并将配置改写成生成的 `$ROZSA_...` 引用。
- 未加 `$` 的名称（如 `"DEEPSEEK_API_KEY"`）会被当作明文值，不再被当作环境变量名。
- Shell 命令已禁止：任何以 `!` 开头的值都会被拒绝，Rózsa 不会执行配置中的命令。

`.env` 使用普通 `NAME=VALUE` 格式。Rózsa 写入的环境变量默认保存在全局 `~/.rozsa/.env`，不会写入项目级 model config；迁移和写入采用原子替换，并使用仅用户可读的权限。设置 `ROZSA_CONFIG_DIR` 时，私有环境文件位于该目录下的 `.env`。

Unix shell 启动文件只解析静态赋值：POSIX shell 使用 `NAME=value` 或 `export NAME=value`，fish 使用 `set -gx NAME value`。Rózsa 不会执行启动文件、命令替换或 `!` 命令。Windows 请通过系统/用户环境变量设置 API key，或写入 `~/.rozsa/.env`。

## 配置诊断

GUI 启动和重新加载模型目录时，会通过统一应用通知显示配置问题：

- `ERROR`：JSON 语法、字段结构、协议或凭据引用格式不符合规范。通知会显示具体错误原因、配置文件路径和修复提示；该文件会被跳过，其他有效模型仍可使用。
- `WARNING`：配置文件本身有效，但 `$NAME` 引用的环境变量无法解析。通知会显示 provider、变量引用和配置文件路径；请检查变量名，并在 Rózsa 进程环境或 `~/.rozsa/.env` 中设置。

环境变量缺失不会被当作 JSON 配置错误，也不会阻止其他模型加载。修复后重启 Rózsa，已解决的通知会自动收起。

GUI 的 model 选择列表只显示当前能够完成认证的模型。配置文件有效但凭据缺失或 `$NAME` 无法解析的模型会继续出现在诊断通知中，但不会出现在可选列表；通过 `auth.json` 登录的 Anthropic、Codex OAuth 和 GitHub Copilot 模型在凭据可用时仍会显示。

## 相关代码

- Model registry: `crates/rozsa-app/src/model_registry/mod.rs`
- CLI entry: `crates/rozsa-cli/src/run.rs`
