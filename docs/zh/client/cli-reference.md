---
title: CLI 参考
description: Savhub CLI 完整命令参考
---

# CLI 参考

## 全局选项

所有命令均支持以下全局选项：

| 选项 | 说明 |
|------|------|
| `--profile <路径>` | 配置/数据目录（覆盖 `SAVHUB_CONFIG_DIR` 和默认的 `~/.savhub`） |
| `--workdir <路径>` | 项目目录（默认：当前目录） |
| `--dir <路径>` | 工作目录内的技能子目录 |
| `--site <URL>` | API 站点地址 |
| `--registry <URL>` | 注册表地址 |
| `--no-input` | 禁用交互式提示 |

## 身份验证

```bash
savhub login [--no-browser]       # 通过 GitHub OAuth 登录
savhub logout                      # 清除本地令牌
savhub whoami                      # 显示当前用户
```

## Apply（自动配置）

```bash
savhub apply [选项]
```

| 选项 | 说明 |
|------|------|
| `--dry-run` | 预览修改内容 |
| `--yes` | 跳过所有提示 |
| `--agents <列表>` | 仅同步到这些 AI 客户端 |
| `--skip-agents <列表>` | 跳过这些 AI 客户端 |
| `--skills <列表>` | 手动添加技能（持久化保存） |
| `--skip-skills <列表>` | 手动跳过技能（持久化保存） |
| `--flocks <列表>` | 手动添加技能集（持久化保存） |
| `--skip-flocks <列表>` | 手动跳过技能集（持久化保存） |

不带参数运行 `savhub` 等同于 `savhub apply`。

## 技能管理

```bash
savhub search <关键词...> [--limit N]           # 搜索注册表
savhub fetch <slug> [--version V] [--force]      # 获取技能
savhub update                                     # 从本地缓存更新项目技能
savhub prune <slug> [--yes]                      # 移除技能
savhub list                                       # 列出已获取技能
savhub explore [--limit N] [--sort S] [--json]   # 浏览技能
savhub inspect <slug> [选项]                     # 查看技能详情
```

### Inspect 选项

| 选项 | 说明 |
|------|------|
| `--version <V>` | 查看指定版本 |
| `--tag <TAG>` | 按标签筛选 |
| `--versions` | 显示版本历史 |
| `--files` | 列出文件 |
| `--file <路径>` | 显示文件内容 |
| `--json` | JSON 格式输出 |

## 启用 / 禁用

```bash
savhub enable <slug> --repo <路径> [选项]   # 在项目中启用仓库技能
savhub disable <slug> [--yes]               # 禁用项目技能
```

### Enable 选项

| 选项 | 说明 |
|------|------|
| `--repo <路径>` | 仓库名称 |
| `--selector <S>` | 关联选择器 |
| `--use-repo` | 覆盖已有技能 |
| `--keep-existing` | 冲突时保留已有技能 |

## 选择器

```bash
savhub selector list              # 列出所有选择器
savhub selector show <名称>       # 查看选择器详情
savhub selector test              # 对当前目录运行选择器
```

## 技能集

```bash
savhub flock list                     # 列出所有技能集
savhub flock show <slug>              # 查看技能集详情
savhub flock fetch <slug> [--yes]     # 获取技能集中的技能
```

## 技能缓存管理

```bash
savhub fetched                        # 列出全局已获取的技能
savhub fetched --update               # 更新所有已获取的仓库/技能到最新版本
savhub fetched --update --force       # 强制更新（即使已是最新）
savhub fetched --prune                # 移除未被任何项目使用的技能
```

## Pilot（内置技能）

```bash
savhub pilot install [--agents <列表>]     # 将内置技能安装到 AI 客户端
savhub pilot uninstall [--agents <列表>]   # 从 AI 客户端移除内置技能
savhub pilot status [--agents <列表>]      # 显示每个客户端的安装状态
savhub pilot notify                        # 触发配置变更信号
```

## 注册表

```bash
savhub registry search <关键词...> [--limit N]          # 搜索注册表
savhub registry list [--page N] [--page-size N] [--json] # 分页列出技能
```

## 自更新

```bash
savhub self-update       # 将 CLI 更新到最新版本
```

从 GitHub Releases 检查是否有新版本，下载当前平台对应的二进制文件，原地替换当前可执行文件。旧版本会备份为 `.old` 文件，下次启动时自动清理。

## 其他

```bash
savhub docs              # 在浏览器中打开文档
```
