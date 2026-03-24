---
title: CLI 参考
description: Savhub CLI 完整命令参考
---

# CLI 参考

## 全局选项

所有命令均支持以下全局选项：

| 选项 | 说明 |
|------|------|
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
savhub auth login|logout|whoami    # 身份验证子命令
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
| `--presets <列表>` | 手动添加预设组 |
| `--skip-presets <列表>` | 手动跳过预设组 |
| `--skills <列表>` | 手动添加技能 |
| `--skip-skills <列表>` | 手动跳过技能 |
| `--flocks <列表>` | 手动添加技能集 |
| `--skip-flocks <列表>` | 手动跳过技能集 |

别名：`savhub auto`

## 技能管理

```bash
savhub search <关键词...> [--limit N]           # 搜索注册表
savhub fetch <slug> [--version V] [--force]      # 获取技能
savhub update [slug] [--all] [--global] [--force] # 更新技能
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
| `--preset <P>` | 关联预设组 |
| `--selector <S>` | 关联选择器 |
| `--use-repo` | 覆盖已有技能 |
| `--keep-existing` | 冲突时保留已有技能 |

## 选择器

```bash
savhub selector list              # 列出所有选择器
savhub selector show <名称>       # 查看选择器详情
savhub selector test              # 对当前目录运行选择器
```

别名：`savhub detector`

## 预设组

```bash
savhub preset create <名称> [--description D]   # 创建预设组
savhub preset list                                # 列出预设组
savhub preset show <名称>                         # 查看预设组
savhub preset delete <名称> [--yes]               # 删除预设组
savhub preset add <预设组> <技能...>              # 添加技能
savhub preset remove <预设组> <技能...>           # 移除技能
savhub preset bind <名称>                         # 绑定到项目
savhub preset unbind                              # 解除绑定
savhub preset status                              # 查看绑定状态
```

别名：`savhub profile`

## 技能集

```bash
savhub flock list                     # 列出所有技能集
savhub flock show <slug>              # 查看技能集详情
savhub flock fetch <slug> [--yes]     # 获取技能集中的技能
```

## 注册表

```bash
savhub registry search <关键词...> [--limit N]          # 搜索注册表
savhub registry list [--page N] [--page-size N] [--json] # 分页列出技能
```

## 社交功能

```bash
savhub star <slug>       # 收藏技能
savhub unstar <slug>     # 取消收藏
```

## 自更新

```bash
savhub self-update       # 将 CLI 更新到最新版本
```

从 GitHub Releases 检查是否有新版本，下载当前平台对应的二进制文件，原地替换当前可执行文件。旧版本会备份为 `.old` 文件，下次启动时自动清理。

## 其他

```bash
savhub delete <slug>     # 删除技能（仅管理员）
savhub docs              # 在浏览器中打开文档
```
