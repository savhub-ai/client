---
title: 预设组
description: 管理技能分组
---

# 预设组

预设组（Preset）是技能和技能集的命名集合，可以一起启用到项目中，方便将相关技能组合在一起。

## CLI 命令

```bash
# 创建预设组
savhub preset create my-rust-tools --description "Rust 开发技能"

# 列出所有预设组
savhub preset list

# 查看预设组详情
savhub preset show my-rust-tools

# 向预设组添加技能
savhub preset add my-rust-tools rust-clippy rust-testing

# 从预设组移除技能
savhub preset remove my-rust-tools rust-testing

# 删除预设组
savhub preset delete my-rust-tools --yes

# 将预设组绑定到当前项目
savhub preset bind my-rust-tools

# 解除当前项目的预设组绑定
savhub preset unbind

# 查看预设组绑定状态
savhub preset status
```

`savhub preset` 命令也可以通过 `savhub profile`（别名）使用。

## 工作原理

### 创建预设组

预设组包含名称、可选描述、技能 slug 列表和技能集 slug 列表：

```bash
savhub preset create web-dev
savhub preset add web-dev react-patterns typescript-best-practices
```

### 绑定到项目

将预设组绑定到项目后，该预设组中的所有技能都会生效：

```bash
cd /path/to/my-project
savhub preset bind web-dev
```

这将写入 `savhub.toml`：

```toml
presets = ["web-dev"]
```

### 技能解析

技能按以下顺序从多个来源解析：

1. **显式预设组绑定** - 你手动绑定到项目的预设组
2. **选择器匹配的预设组** - 匹配的选择器贡献的预设组
3. **手动技能** - 通过 `savhub fetch` 单独添加的技能

### 包含技能集

预设组可以引用技能集。当预设组启用时，其包含的技能集中的所有技能也会一起解析。

## 存储

预设组全局存储在 `~/.config/savhub/profiles.json` 中，在所有项目间共享。

## 桌面应用

Savhub 桌面应用在**预设组**页面提供可视化的预设组管理，包括：
- 创建/编辑/删除预设组
- 搜索和添加技能
- 绑定/解除绑定预设组到项目
