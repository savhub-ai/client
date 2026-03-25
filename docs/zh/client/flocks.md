---
title: 技能集
description: 浏览和获取注册表中的技能集合
---

# 技能集

技能集（Flock）是 Savhub 注册表中的精选技能集合。一个技能集将相关的技能组合在一起（例如，将所有 Rust 开发工具放在一个 "rust-dev" 技能集中）。

## CLI 命令

```bash
# 列出所有可用的技能集
savhub flock list

# 查看技能集详情和包含的技能
savhub flock show rust-dev

# 获取技能集中的所有技能
savhub flock fetch rust-dev

# 跳过确认直接获取
savhub flock fetch rust-dev --yes
```

## 工作原理

### 注册表定义

技能集由仓库维护者在 Savhub 注册表中定义。每个技能集包含：
- **Slug** - 唯一标识符（如 `rust-dev`）
- **名称** - 显示名称（如"Rust 开发"）
- **描述** - 技能集涵盖的内容
- **技能列表** - 此技能集中的技能 slug 列表

### 浏览技能集

```bash
$ savhub flock list
  rust-dev                   5 skill(s)  Rust Development
    Rust 项目开发工具和最佳实践
  web-frontend               8 skill(s)  Web Frontend
    现代 Web 前端开发技能
  python-ml                  6 skill(s)  Python ML
    Python 机器学习技能

3 flock(s)
```

### 获取技能集

获取技能集时，每个技能从其 git 源克隆，并在 `savhub.toml` 中记录：

```bash
$ savhub flock fetch rust-dev
Flock: Rust Development (rust-dev)
Skills to fetch:
  [+] rust-clippy
  [+] rust-testing
  [+] rust-error-handling
Fetch 3 skill(s) from flock "rust-dev"? [Y/n]
  Added: rust-clippy
  Added: rust-testing
  Added: rust-error-handling

Done. 3 skill(s) added from flock "rust-dev".
```

## 技能集在生态系统中的角色

技能集与其他 Savhub 功能集成：

- **选择器**可以引用技能集 — 当选择器匹配时，其技能集会被推荐
- **Apply 命令** — `savhub apply` 从匹配的选择器收集技能集，然后让你选择要获取的

## 桌面应用

Savhub 桌面应用在**技能**页面（分组视图）提供技能集浏览功能，以及详细的技能集页面，展示所有包含的技能及获取/移除操作。
