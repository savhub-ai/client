---
title: 基础概念
description: Savhub 是什么、核心术语，以及各部分如何协作
---

# 基础概念

## Savhub 是什么？

Savhub 是一个基于项目特征的 AI Skill 管理器。它的核心理念：**通过选择器（Selectors）分析项目特征（文件、框架、语言），自动为项目安装匹配的 AI 技能。**

平台分为两部分：

- **savhub.ai** — 在线注册中心，用于浏览、搜索、收藏和评论技能。
- **Savhub Client** — CLI（及桌面应用），通过预置和自定义选择器检测项目类型，将匹配的技能同步到你的 AI 编辑器（Claude Code、Codex、Cursor、Windsurf 等）。

## 核心术语

### Skill（技能）

技能是一个 Markdown 文件（`SKILL.md`），教 AI 客户端如何完成某件事 — 代码规范、框架配方、部署清单等。每个技能存放在 Git 仓库中，与相关源码一起版本化。

### Flock（技能集）

Flock 是一组相关技能的集合。当仓库被索引时，Savhub 根据目录结构和内容自动将技能分组到一个或多个 flock 中。安装时以 flock 为单位。

### Repo（仓库）

仓库是一个已注册的 Git 仓库。Savhub 会克隆它、扫描 `SKILL.md` 文件，并从结果中创建 flock。一个仓库可以包含多个 flock。

### Selector（选择器）

选择器是分析项目特征的规则，通过检查文件、文件夹、glob 模式或文件内容来判断项目类型。例如 "存在 `Cargo.toml`" → Rust 项目 → 推荐 `rust-dev` 技能集。Savhub 预置了常见语言和框架的选择器，你也可以创建自定义选择器匹配任意项目结构。客户端在执行 `savhub apply` 时运行选择器，为项目推荐合适的 flock。

### Registry（注册表）

注册表存储所有已索引技能和 flock 的元数据。Savhub Client 通过 API 查询注册表，搜索速度很快。

## 工作流程

```
Git 仓库 ──索引──▶ Savhub 服务器 ──发布──▶ Registry
                                              │
                    savhub.ai ◀── 浏览 ───────┘
                                              │
                 你的项目 ◀── savhub apply ────┘
```

1. 仓库所有者（或 Savhub 爬虫）提交 Git URL 进行索引。
2. 服务器克隆仓库、发现 `SKILL.md` 文件、将它们分组为 flock，并存入数据库。
3. 元数据推送到公开的 registry 仓库。
4. 用户在 savhub.ai 上浏览技能，或通过 `savhub search` 在本地搜索。
5. 在项目中运行 `savhub apply`，自动检测项目类型、推荐匹配的 flock 并安装技能。

## 下一步

- [快速开始](https://savhub.ai/zh/docs/getting-started/quick-start) — 安装客户端并应用你的第一个技能
- [Apply 命令](https://savhub.ai/zh/docs/client/apply) — 详细的 apply 工作流
- [CLI 参考](https://savhub.ai/zh/docs/client/cli-reference) — 完整命令列表
