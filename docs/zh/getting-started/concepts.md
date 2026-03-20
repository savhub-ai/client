---
title: 基础概念
description: Savhub 是什么、核心术语，以及各部分如何协作
---

# 基础概念

## Savhub 是什么？

Savhub 是一个面向 AI 编程客户端的开放技能索引平台。它扫描公开 Git 仓库中的 `SKILL.md` 文件，将它们组织为可发现的集合，让你通过一条命令就能将技能安装到任何项目中。支持 Claude Code、Codex、Cursor、Windsurf 等 AI 编辑器。

平台分为两部分：

- **savhub.ai** — 在线注册中心，用于浏览、搜索、收藏和评论技能。
- **Savhub Client** — CLI（及桌面应用），将技能同步到本地项目和 AI 编辑器。

## 核心术语

### Skill（技能）

技能是一个 Markdown 文件（`SKILL.md`），教 AI 客户端如何完成某件事 — 代码规范、框架配方、部署清单等。每个技能存放在 Git 仓库中，与相关源码一起版本化。

### Flock（技能集）

Flock 是一组相关技能的集合。当仓库被索引时，Savhub 根据目录结构和内容自动将技能分组到一个或多个 flock 中。安装时以 flock 为单位。

### Repo / Realm（仓库）

仓库（也称为 *realm*）是一个已注册的 Git 仓库。Savhub 会克隆它、扫描 `SKILL.md` 文件，并从结果中创建 flock。一个仓库可以包含多个 flock。

### Selector（选择器）

选择器是一种检测项目类型的规则 — 例如"这是一个 Rust 项目"或"这个仓库使用了 React"。Savhub Client 在执行 `savhub apply` 时运行选择器，为项目推荐合适的 flock。

### Preset（预设组）

预设组是一组你希望一起应用的技能列表，独立于选择器。适合个人工作流或团队规范。

### Registry（注册表）

注册表是一个 Git 仓库，存储所有已索引技能和 flock 的元数据。Savhub Client 将注册表同步到本地，使搜索快速且可离线使用。

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
