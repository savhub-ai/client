---
title: 快速开始
description: 安装和设置 Savhub CLI 与桌面应用
---

# 快速开始

Savhub Client 是一款用于发现、安装和管理 AI 编程技能的工具，支持 Claude Code、Codex、Cursor、Windsurf 等 AI 客户端。

## 安装

### CLI

从 [GitHub Releases](https://github.com/savhub-ai/savhub/releases) 下载最新版本，或从源码构建：

```bash
# 克隆并构建
git clone https://github.com/savhub-ai/savhub.git
cd savhub
cargo build --release

# 二进制文件位于 target/release/savhub
```

### 桌面应用

从相同的发布页面下载桌面安装包，或从源码构建：

```bash
cargo build --release -p savhub-desktop
```

## 身份验证

通过 GitHub 账户登录以发布和管理技能：

```bash
savhub login
```

这将打开浏览器进行 GitHub OAuth 授权，授权后令牌将保存在本地。

```bash
# 验证登录状态
savhub whoami

# 需要时登出
savhub logout
```

## 快速上手

### 1. 浏览可用技能

```bash
# 搜索技能
savhub search rust

# 浏览所有技能
savhub explore

# 列出可用的技能集（flock）
savhub flock list
```

### 2. 为项目应用技能

进入你的项目目录并运行：

```bash
cd /path/to/my-project
savhub apply
```

这将会：
1. 运行选择器检测项目类型（如 Rust、Python、Web 等）
2. 显示匹配的选择器和推荐的技能集
3. 让你交互式选择要安装的技能集
4. 安装技能并同步到你的 AI 客户端（Claude Code、Codex 等）

### 3. 验证

```bash
# 列出项目中已安装的技能
savhub list

# 仅查看选择器匹配结果，不做任何修改
savhub selector test
```

## 下一步

- [Apply 命令](https://savhub.ai/zh/docs/client/apply) - apply 工作流的详细用法
- [选择器](https://savhub.ai/zh/docs/client/selectors) - 创建自定义项目检测规则
