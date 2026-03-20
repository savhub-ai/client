---
title: Apply 命令
description: 自动检测项目类型并将技能应用到 AI 客户端
---

# Apply 命令

`savhub apply` 是为项目配置技能的主要方式。它运行选择器检测项目类型，解析匹配的技能集和技能，并将它们直接安装到 AI 客户端的技能目录中。

## 基本用法

```bash
cd /path/to/my-project
savhub apply
```

## 工作流程

### 选择器匹配时

1. **选择器匹配** — 所有已配置的选择器对当前目录运行，每个选择器通过检查规则（文件是否存在、glob 模式等）来检测项目类型。

2. **技能集收集** — 匹配的选择器贡献技能集（flock）和预设组（preset），预设组也可能引用技能集。

3. **交互式选择** — 通过多选对话框选择要安装的技能集（使用 `--yes` 可跳过）。

4. **跳过过滤** — `savhub.toml` 中 `[skills] skipped` 列出的技能会被排除。条目可以是 slug 或签名（sign），详见[技能签名](#技能签名)。

5. **与锁定文件比对** — 已记录在 `savhub.lock` 中的技能会被跳过，仅安装新技能。

6. **批量安装** — 所有新技能以单次批量操作安装，按 git 仓库分组以减少 clone/pull 操作次数。

7. **直接复制** — 技能从仓库检出目录直接复制到各 AI 客户端的项目级技能目录：
   - Claude Code：`.claude/skills/`
   - Codex：`.agents/skills/`

8. **文件更新：**
   - `savhub.toml` — `[selectors]`、`[presets]`、`[flocks]` 中的 `matched` 字段会被当前选择器结果**替换**。所有 `manual_*` 字段**永远不会**被 apply 修改。
   - `savhub.lock` — 已安装的技能会被**追加**，记录版本和 git commit。

### 没有选择器匹配时

如果没有选择器匹配，所有之前由 savhub 安装的技能将被移除：

1. 读取 `savhub.lock` 确定已安装的技能。
2. 列出要移除的技能并请求确认（除非使用了 `--yes`）。
3. 从 AI 客户端目录中删除技能文件夹（`.claude/skills/`、`.agents/skills/`）。
4. 清除 `savhub.toml` 中的 `selectors.matched`、`presets.matched` 和 `flocks.matched`（所有 `manual_*` 字段保持不变）。
5. 删除 `savhub.lock`。

## 选项

| 选项 | 说明 |
|------|------|
| `--dry-run` | 预览将要执行的操作，不做实际修改 |
| `--yes`, `-y` | 跳过所有确认提示（接受所有技能集） |
| `--agents <列表>` | 仅同步到指定的 AI 客户端 |
| `--skip-agents <列表>` | 跳过指定的 AI 客户端 |
| `--presets <列表>` | 手动添加预设组（保存到 `presets.manual_added`） |
| `--skip-presets <列表>` | 手动跳过预设组（保存到 `presets.manual_skipped`） |
| `--skills <列表>` | 手动添加技能（保存到 `skills.manual_added`） |
| `--skip-skills <列表>` | 手动跳过技能（保存到 `skills.manual_skipped`） |
| `--flocks <列表>` | 手动添加技能集（保存到 `flocks.manual_added`） |
| `--skip-flocks <列表>` | 手动跳过技能集（保存到 `flocks.manual_skipped`） |

所有 `--presets`、`--skills`、`--flocks` 及其 `--skip-*` 对应项都是**持久化的**——它们保存到 `savhub.toml` 中，在后续每次运行时生效。

## 技能签名

技能"签名"（sign）是唯一标识一个技能的完整注册表路径：

```
github.com/owner/repo/path/to/skill
```

签名可用于 `savhub.toml` 的 `[skills] manual_skipped` 中排除特定技能的自动安装。同时支持简单 slug 和签名：

```toml
[skills]
manual_skipped = [
    "some-skill",                                          # 按 slug
    "github.com/anthropics/skills/skills/claude-api",      # 按完整签名
]
```

## 示例

```bash
# 预览修改内容
savhub apply --dry-run

# 无交互式确认（适用于 CI/自动化）
savhub apply -y

# 仅为 Claude Code 安装
savhub apply --agents claude-code

# 安装到除 Cursor 外的所有客户端
savhub apply --skip-agents cursor

# 多个客户端（逗号分隔）
savhub apply --agents claude-code,codex

# 手动添加预设组（持久化到 savhub.toml）
savhub apply --presets my-custom-preset

# 手动添加技能集
savhub apply --flocks rust-dev

# 手动添加技能
savhub apply --skills my-skill

# 跳过某个技能的自动安装
savhub apply --skip-skills unwanted-skill

# 组合使用：添加技能集但跳过其中某个技能
savhub apply --flocks web-dev --skip-skills legacy-tool
```

## 客户端名称

`--agents` 和 `--skip-agents` 可使用以下名称：

| 名称 | AI 客户端 |
|------|-----------|
| `claude-code` | Claude Code |
| `codex` | Codex (OpenAI) |
| `cursor` | Cursor |
| `windsurf` | Windsurf |
| `continue` | Continue |
| `vscode` | VS Code |

## 输出示例

```
Matched selectors (by priority):
  [+] Rust Project (priority 10) — Detects Rust/Cargo projects

Flocks to fetch:
  [+] rust-dev (5 skills)

Presets from selectors:
  [+] rust-core

Skills to fetch:
  [+] rust-clippy
  [+] rust-testing

  ✓ rust-clippy -> .claude/skills/rust-clippy
  ✓ rust-testing -> .claude/skills/rust-testing
  ✓ rust-clippy -> .agents/skills/rust-clippy
  ✓ rust-testing -> .agents/skills/rust-testing

Done. 2 skill(s) fetched from 1 flock(s), 1 selector(s) matched.
```

## 向后兼容

此命令也可通过 `savhub auto`（别名）使用。
