---
title: 扫描与分组
description: 技能如何被发现和组织为 flock
---

# 扫描与分组

用户提交 Git 仓库 URL 后，后端会 clone 仓库，发现所有 `SKILL.md` 文件，并自动将它们分组为 **flock**。

## 流水线概览

```
POST /api/v1/index { git_url, git_ref, git_subdir }
  |
  +- 1. Clone 仓库                (10%)
  +- 2. 解析索引规则              (20%)
  +- 3. 扫描 SKILL.md 文件        (30%)
  +- 4. 分组为 flock               (50%)  <- 本文档
  +- 5. AI 生成元数据              (多 skill flock)
  +- 6. 持久化 repo/flock/skill   (70%)
  +- 7. 安全扫描
  +- 8. 同步 registry checkout     (95%)
  +- 9. 完成                       (100%)
```

## 术语

| 术语 | 含义 |
|------|------|
| **Repo** | 顶层命名空间，对应一个 git 仓库 |
| **Flock** | repo 内的技能分组，对应目录子树 |
| **Skill** | 单个 `SKILL.md` 文件及其所在目录 |
| **`relative_dir`** | 从扫描根目录到技能目录的相对路径，`"."` 表示根目录 |
| **LCA** | 最近公共祖先——所有技能路径的共享前缀 |

## 策略选择

分组前，系统查询 `index_rules` 表寻找匹配规则：

| 策略 | 行为 |
|------|------|
| `each_dir_as_flock` | 每个匹配的目录成为一个 flock |
| `smart`（默认） | 下述 LCA 算法 |

## Smart 分组算法（基于 LCA）

### 第 1 步 — 解析路径段

```
"."                    -> []
"skills/lang/python"   -> ["skills", "lang", "python"]
```

### 第 2 步 — 求 LCA

计算所有候选路径共享的最长前缀。

### 第 3 步 — 分配分组键

去掉 LCA 后取第一个剩余段作为分组键。

### 第 4 步 — 质量检查

| 条件 | 结果 |
|------|------|
| 分组数 >= 2 且最大组 >= 2 个 skill | 多个 flock |
| 否则 | 单个 flock，以仓库名命名 |

## 示例

### 有分类的深层嵌套

```
repo: github.com/dave/toolbox
+-- skills/lang/python/SKILL.md
+-- skills/lang/rust/SKILL.md
+-- skills/devops/deploy/SKILL.md
```
LCA=`["skills"]`，分组：`lang`(2), `devops`(1) -> 多个 flock

### each_dir_as_flock 策略

```
repo: github.com/anthropics/skills（规则：path_regex="skills"）
+-- skills/python/SKILL.md
+-- skills/rust/SKILL.md
```
扫描根目录覆盖为 `skills/`，每个子目录 = 1 个 flock

## 实现参考

- LCA 分组：`backend/src/service/index_jobs.rs` -> `compute_flock_group_plans()`
- each_dir_as_flock：`backend/src/service/index_jobs.rs` -> `compute_each_dir_as_flock_plans()`
- 索引规则：`backend/src/service/index_rules.rs` -> `resolve_index_rule()`
- AI 元数据：`backend/src/service/ai.rs` -> `generate_flock_metadata()`
