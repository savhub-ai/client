---
title: 索引规则
description: 控制仓库扫描和分组方式
---

# 索引规则

索引规则控制 Savhub 如何扫描仓库并将 skill 组织为 flock。规则通过管理面板的 **管理 -> 索引规则** 进行管理。

## 规则结构

每条规则包含：

| 字段 | 说明 |
|------|------|
| **Repo URL** | 归一化的 git URL（如 `https://github.com/org/repo.git`） |
| **Path Regex** | 仓库内的扫描路径（如 `skills`、`*`） |
| **Strategy** | 分组算法：`each_dir_as_flock` 或 `smart` |

## 策略

### `each_dir_as_flock`

每个匹配的目录成为一个独立的 flock。没有最小数量门槛。

**示例**：仓库结构如下：
```
skills/
  python/
    SKILL.md
  rust/
    SKILL.md
  go/
    SKILL.md
```

规则 `path_regex: skills` 会产生 3 个 flock：`python`、`rust`、`go`。

### `smart`（默认）

使用 LCA（最近公共祖先）算法自动检测分组结构。仅当存在至少 2 个分组且每个分组至少 2 个 skill 时才创建多个分组，否则回退为单个 flock。

## 路径匹配逻辑

用户提交索引任务时：

1. 系统查找匹配归一化仓库 URL 的规则
2. 如果用户从根目录扫描（`subdir = "."`），具体 `path_regex`（如 `skills`）会**覆盖扫描根目录**
3. 同一仓库存在多条规则时，最精确的匹配优先
4. 没有规则匹配时，使用默认的 `smart` 策略
