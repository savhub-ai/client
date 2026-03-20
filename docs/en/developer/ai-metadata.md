---
title: AI Metadata Generation
description: Auto-generate flock descriptions with AI
---

# AI Metadata Generation

When a flock contains multiple skills, Savhub can use an AI API to generate a meaningful flock name and description from the collected skill metadata.

## Configuration

| Variable | Description | Default |
|----------|-------------|---------|
| `SAVHUB_AI_PROVIDER` | AI provider: `zhipu` or `doubao` | *(disabled)* |
| `SAVHUB_AI_API_KEY` | API key for the provider | |
| `SAVHUB_AI_CHAT_MODEL` | Override default model | Provider-specific |

## Supported Providers

### Zhipu (GLM)

- Default model: `glm-4-flash`
- Get an API key at [open.bigmodel.cn](https://open.bigmodel.cn)

```env
SAVHUB_AI_PROVIDER=zhipu
SAVHUB_AI_API_KEY=your_zhipu_api_key
```

### Doubao (Volcengine)

- Default model: `doubao-1-5-pro-32k-250115`
- Get an API key at [console.volcengine.com/ark](https://console.volcengine.com/ark)

```env
SAVHUB_AI_PROVIDER=doubao
SAVHUB_AI_API_KEY=your_doubao_api_key
```

## How It Works

1. During an index job, when a flock group has **more than one skill**, AI generation is triggered
2. Skill names and descriptions are collected (truncated to ~2000 characters total)
3. A prompt asks the AI to generate a short flock name (2-5 words) and description (under 120 chars)
4. The AI responds with a JSON object containing `name` and `description`
5. If AI is not configured or the call fails, the system falls back to using the directory name

Both providers use an OpenAI-compatible chat completions API endpoint.

## Example

For a flock containing skills like:
- **Python Helper**: Write and debug Python scripts
- **Rust Analyzer**: Analyze Rust code structure
- **Go Builder**: Build and test Go projects

The AI might generate:
```json
{
  "name": "Programming Language Tools",
  "description": "A collection of language-specific coding assistants for Python, Rust, and Go development."
}
```
