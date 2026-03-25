---
title: 快速开始
description: 安装 Savhub CLI 并应用你的第一个 AI 技能
---

# 快速开始

Savhub 是一个基于项目特征的 AI Skill 管理器。它通过**选择器（Selectors）**分析你的项目特征 — 检测项目中的文件、框架和语言 — 自动安装匹配的 AI 技能到你的编程客户端（Claude Code、Codex、Cursor、Windsurf 等）。

## 安装

### 一键安装（推荐）

**Linux / macOS：**
```bash
curl -fsSL https://raw.githubusercontent.com/savhub-ai/savhub/main/scripts/install.sh | bash
```

**Windows（PowerShell）：**
```powershell
irm https://raw.githubusercontent.com/savhub-ai/savhub/main/scripts/install.ps1 | iex
```

安装脚本会自动下载最新版本、添加 `savhub` 到 PATH，并将内置技能安装到你的 AI 客户端。

### 手动下载

从 [GitHub Releases](https://github.com/savhub-ai/savhub/releases) 下载对应平台的二进制文件，放入 PATH 即可。

### 从源码构建

```bash
git clone https://github.com/savhub-ai/savhub.git
cd savhub
cargo build --release
# 二进制文件位于 target/release/savhub（Windows 下为 savhub.exe）
```

## 为项目应用技能

进入你的项目目录并运行：

```bash
cd /path/to/my-project
savhub apply
```

这将会：
1. 运行**选择器**分析项目特征（如 `Cargo.toml` → Rust、`package.json` 含 `react` → React）
2. 显示匹配的选择器和推荐的**技能集**（flock）
3. 让你交互式选择要安装的技能集
4. 安装技能并复制到你的 AI 客户端（Claude Code、Codex 等）

Savhub 预置了常见语言和框架的选择器。你也可以创建**自定义选择器**来匹配任意项目结构 — 详见[选择器](https://savhub.ai/zh/docs/client/selectors)。

## 浏览和搜索技能

```bash
# 按关键词搜索技能
savhub search rust

# 浏览注册表中的所有技能
savhub explore

# 列出可用的技能集
savhub flock list

# 查看特定技能集及其包含的技能
savhub flock show rust-dev
```

## 登录（可选）

通过 GitHub 登录以收藏技能和发布自己的技能：

```bash
savhub login       # 打开浏览器进行 GitHub OAuth 授权
savhub whoami      # 验证登录状态
savhub logout      # 清除本地令牌
```

## 验证

```bash
# 列出项目中已安装的技能
savhub list

# 仅查看选择器匹配结果，不做任何修改
savhub selector test

# 预览 apply 将执行的操作
savhub apply --dry-run
```

## 下一步

- [基础概念](https://savhub.ai/zh/docs/getting-started/concepts) - 核心术语和各部分如何协作
- [Apply 命令](https://savhub.ai/zh/docs/client/apply) - apply 工作流的详细用法
- [选择器](https://savhub.ai/zh/docs/client/selectors) - 创建自定义项目检测规则
- [CLI 参考](https://savhub.ai/zh/docs/client/cli-reference) - 完整命令列表
