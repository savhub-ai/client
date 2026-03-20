---
title: API 参考
description: REST API 接口列表
---

# API 参考

所有接口前缀为 `/api/v1/`。

## 公开接口

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查 |
| GET | `/search?q=...` | 全文搜索技能 |
| GET | `/skills` | 技能列表（支持 `sort`、`limit`、`cursor`、`q`） |
| GET | `/skills/{slug}` | 技能详情 |
| GET | `/flocks` | 所有 flock 列表 |
| GET | `/flocks/{id}` | Flock 详情（UUID） |
| GET | `/repos` | 仓库列表 |
| GET | `/repos/{domain}/{path_slug}` | 仓库详情 |
| GET | `/users` | 用户列表 |
| GET | `/users/{handle}` | 用户资料 |
| GET | `/resolve?slug=...&hash=...` | 按指纹解析技能 |
| GET | `/download?slug=...` | 下载技能 zip 包 |

## 需认证接口

请求头需携带 `Authorization: Bearer {token}`。

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/whoami` | 当前用户信息 |
| POST | `/index` | 提交索引任务 |
| GET | `/index/list` | 索引任务列表 |
| GET | `/index/{id}` | 索引任务状态 |
| POST | `/repos` | 创建仓库 |
| POST | `/skills/{slug}/comments` | 添加评论 |
| POST | `/skills/{slug}/star` | 收藏/取消收藏 |

## 管理员接口

需要管理员角色。

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/management/summary` | 管理面板概览 |
| GET/POST | `/management/site-admins` | 站点管理员管理 |
| GET/POST | `/management/index-rules` | 索引规则管理 |
| POST | `/management/users/{id}/role` | 设置用户角色 |
| POST | `/management/users/{id}/ban` | 封禁用户 |

## WebSocket

连接 `/api/v1/ws` 获取实时索引任务进度。

```json
{"action": "subscribe", "job_id": "019..."}
{"type": "index_progress", "job_id": "019...", "status": "running", "progress_pct": 50}
{"action": "unsubscribe", "job_id": "019..."}
```

## 认证流程

Savhub 使用 GitHub OAuth 登录。前端跳转到 `/auth/github/start`，GitHub 回调后后端创建会话 token 返回给前端。
