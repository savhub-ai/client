---
title: AI 元数据生成
description: 使用 AI 自动生成 flock 描述
---

# AI 元数据生成

当一个 flock 包含多个 skill 时，Savhub 可以调用 AI API，从收集的 skill 元数据中自动生成有意义的 flock 名称和描述。

## 配置

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `SAVHUB_AI_PROVIDER` | AI 提供商：`zhipu` 或 `doubao` | *（未启用）* |
| `SAVHUB_AI_API_KEY` | API 密钥 | |
| `SAVHUB_AI_CHAT_MODEL` | 覆盖默认模型 | 取决于提供商 |

## 支持的提供商

### 智谱 (GLM)

- 默认模型：`glm-4-flash`
- 获取 API 密钥：[open.bigmodel.cn](https://open.bigmodel.cn)

```env
SAVHUB_AI_PROVIDER=zhipu
SAVHUB_AI_API_KEY=你的智谱API密钥
```

### 豆包 (火山引擎)

- 默认模型：`doubao-1-5-pro-32k-250115`
- 获取 API 密钥：[console.volcengine.com/ark](https://console.volcengine.com/ark)

```env
SAVHUB_AI_PROVIDER=doubao
SAVHUB_AI_API_KEY=你的豆包API密钥
```

## 工作原理

1. 索引任务执行时，当 flock 分组包含 **多于一个 skill** 时，触发 AI 生成
2. 收集所有 skill 的名称和描述（截断到约 2000 字符）
3. 通过提示词请求 AI 生成简短的 flock 名称（2-5 个词）和描述（120 字符以内）
4. AI 返回包含 `name` 和 `description` 的 JSON
5. 如果 AI 未配置或调用失败，系统回退到使用目录名称

两个提供商都使用 OpenAI 兼容的 chat completions API。
