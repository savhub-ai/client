# Savhub

[English](README.md) | 中文

**Savhub 是一个项目感知的 AI 技能管理器。**

## 为什么需要 Savhub？

AI 编码智能体（Claude Code、Codex、Cursor 等）越来越强大，但它们缺少对特定框架和工具链的深度理解。社区为此编写了大量 AI 技能（提示词、规则、工作流），却散落在各处，难以发现和复用。

Savhub 解决的问题：**让正确的 AI 技能自动匹配到正确的项目。**

## 工作原理

```
savhub apply
```

1. **选择器分析项目** — 检查项目中的文件和依赖（Cargo.toml → Rust，package.json + react → React，...）
2. **匹配技能集合** — 根据检测结果，从注册表推荐对应的技能集合（flocks）
3. **你选择安装** — 交互式选择需要的 flocks
4. **同步到智能体** — 技能自动写入 Claude Code、Codex、Cursor 等智能体的技能目录

选择器是核心机制。Savhub 内置了常见语言和框架（Rust、Python、Go、Salvo、Dioxus、React、Vue 等）的选择器，也支持自定义选择器匹配任意项目结构。

## 支持的 AI 智能体

| 智能体 | 技能目录 |
|-------|---------|
| Claude Code | `.claude/skills/` |
| Codex | `.agents/skills/` |
| Cursor | 已支持 |
| Windsurf | 已支持 |
| Continue | 已支持 |
| VS Code (Copilot) | 已支持 |

## 快速开始

### 安装

**Linux / macOS：**
```sh
curl -fsSL https://raw.githubusercontent.com/savhub-ai/savhub/main/scripts/install.sh | bash
```

**Windows (PowerShell)：**
```powershell
irm https://raw.githubusercontent.com/savhub-ai/savhub/main/scripts/install.ps1 | iex
```

也可从 [GitHub Releases](https://github.com/savhub-ai/savhub/releases) 直接下载。

### 使用

```bash
cd your-project
savhub apply          # 检测项目 → 推荐技能 → 同步到智能体
```

```bash
savhub login          # GitHub OAuth 登录（发布/收藏技能时需要）
savhub search <query> # 搜索技能
savhub explore        # 浏览注册表
savhub self-update    # 更新 CLI
```

## 文档

完整文档和 CLI 参考请访问 [savhub.ai/docs](https://savhub.ai/zh/docs)。

## 许可证

[Apache-2.0](LICENSE)
