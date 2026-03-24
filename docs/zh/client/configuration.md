---
title: 配置文件
description: Savhub 客户端的配置文件和设置项
---

# 配置文件

## 全局配置文件

所有全局配置存储在 `~/.config/savhub/`（或平台特定的配置目录）中。

| 文件 | 说明 |
|------|------|
| `config.json` | 认证令牌、注册表 URL、语言偏好 |
| `selectors.json` | 选择器定义（项目类型检测） |
| `projects.json` | 已注册的项目目录 |
| `fetched_skills.json` | 已获取技能的跟踪数据 |

### 用户配置覆盖

可选的 `~/.savhub/config.toml` 可以覆盖 REST API 基础地址：

```toml
[rest_api]
base_url = "https://custom-registry.example.com"
```

## 项目配置

Savhub 在项目根目录使用两个文件：`savhub.toml`（用户意图）和 `savhub.lock`（实际安装状态）。

### savhub.toml — 项目配置

配置文件包含三个顶层节点：`[selectors]`、`[flocks]`、`[skills]`。每个节点均支持：

- **`matched`** — 由 `savhub apply` 自动管理，每次运行时替换。
- **`manual_added`** — 用户手动添加的条目，`savhub apply` 永远不会修改。
- **`manual_skipped`** — 用户手动排除的条目，`savhub apply` 永远不会修改。

每个节点的最终生效结果为：`matched + manual_added - manual_skipped`。

```toml
version = 1

# ── 选择器 ─────────────────────────────────────────────────
[selectors]

# 由 `savhub apply` 自动管理：
[[selectors.matched]]
selector = "Rust Project"
flocks = ["rust-dev"]

[[selectors.matched]]
selector = "Salvo Web Framework"
flocks = ["salvo-skills"]

# 用户自定义（可选）：
# manual_added = ["my-custom-selector"]
# manual_skipped = ["unwanted-selector"]

# ── 技能集 ─────────────────────────────────────────────────
[flocks]

# 自动管理：由匹配的选择器贡献的技能集。
matched = ["rust-dev", "salvo-skills"]

# 用户自定义（可选）：
# manual_added = ["my-private-flock"]
# manual_skipped = ["salvo-skills"]

# ── 技能 ───────────────────────────────────────────────────
[skills]
# layout = "flat"    # 或 "flock"

# 用户手动添加的技能（通过 `savhub fetch` 等）。
# `savhub apply` 永远不会修改。
# [[skills.manual_added]]
# sign = "github.com/anthropics/skills/skills/claude-api"
# slug = "claude-api"
# version = "1.0.0"

# 永远不自动安装的技能。支持 slug 和签名。
# manual_skipped = [
#     "some-unwanted-skill",
#     "github.com/owner/repo/skills/another-skill",
# ]
```

#### 各节点说明

**`[selectors]`** — 每个 `[[selectors.matched]]` 记录匹配的选择器及其贡献的技能集。使用 `manual_skipped` 可以屏蔽某个选择器。

**`[flocks]`** — `matched` 列出选择器贡献的技能集。使用 `manual_added` 始终安装某个技能集。使用 `manual_skipped` 排除某个技能集。

**`[skills]`** — `manual_added` 列出用户手动安装的技能。`manual_skipped` 列出永远不被 `savhub apply` 自动安装的技能。

#### 技能签名（Sign）

技能"签名"通过注册表路径唯一标识一个技能：

```
github.com/owner/repo/path/to/skill-slug
```

签名可用于 `manual_added`（作为 `sign` 字段）和 `manual_skipped` 条目。同时支持简单 slug 和完整签名。

#### 技能布局

- **flat**（默认） — 技能位于 `skills/{slug}/`
- **flock** — 技能按技能集分组，位于 `skills/{flock-slug}/{skill-slug}/`

### savhub.lock — 安装状态

锁定文件记录了 `savhub apply` 安装的所有技能及其精确版本和 git commit。它是当前实际安装状态的唯一来源。

当 `savhub apply` 运行时：
- **有选择器匹配** — 新技能追加到锁定文件中。
- **无选择器匹配** — 读取锁定文件确定要移除的技能，然后删除锁定文件。

```toml
version = 1

[[skills]]
path = "rust-clippy"
version = "1.2.0"
git_sha = "abc123def456"

[[skills]]
path = "rust-testing"
version = "0.8.1"
git_sha = "789def012345"
```

> **提示：** 你可以将 `savhub.lock` 提交到版本控制中，这样团队成员可以获得相同的技能版本。

## AI 客户端集成

`savhub apply` 将技能直接复制到 AI 客户端的项目级目录：

| AI 客户端 | 技能目录 |
|-----------|----------|
| Claude Code | `.claude/skills/` |
| Codex | `.agents/skills/` |
| Cursor | 不支持（使用 `.mdc` 规则格式） |
| Windsurf | 不支持（使用不同格式） |

## 仓库缓存

技能源代码仓库克隆到 `~/.savhub/repos/`。`savhub apply` 命令使用稀疏检出（sparse checkout）以减少磁盘占用，仅克隆所需的技能目录。同一仓库的多个技能共享同一个 clone。

## 向后兼容

以下旧格式会自动迁移：

| 旧格式 | 当前格式 |
|--------|----------|
| `detectors.json` | `selectors.json` |
| `savhub.toml` 中的 `[detectors]` | `[selectors]`（serde 别名） |
| `.savhub/lock.json` | `savhub.toml` 的 skills 部分 |
