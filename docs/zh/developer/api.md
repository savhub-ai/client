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
| GET | `/skills/{slug}/file?path=...` | 获取技能版本中的文件 |
| GET | `/flocks` | 所有 flock 列表 |
| GET | `/flocks/{id}` | Flock 详情（UUID） |
| GET | `/repos` | 仓库列表 |
| GET | `/repos/{domain}/{path_slug}` | 仓库详情（含 flock 和技能） |
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
| POST | `/skills/{slug}/comments` | 添加技能评论 |
| DELETE | `/skills/{slug}/comments/{id}` | 删除技能评论 |
| POST | `/skills/{slug}/star` | 收藏/取消收藏技能 |
| POST | `/repos/{d}/{p}/flocks/{s}/comments` | 添加 flock 评论 |
| POST | `/repos/{d}/{p}/flocks/{s}/rate` | 评价 flock |
| POST | `/repos/{d}/{p}/flocks/{s}/star` | 收藏/取消收藏 flock |
| POST | `/repos/{d}/{p}/flocks/{s}/block` | 屏蔽 flock |
| DELETE | `/repos/{d}/{p}/flocks/{s}/block` | 取消屏蔽 flock |
| GET | `/blocks/flocks` | 已屏蔽的 flock 列表 |
| POST | `/history` | 记录浏览历史 |
| GET | `/history` | 获取浏览历史 |
| POST | `/reports` | 创建举报 |
| GET | `/reports` | 举报列表（管理员） |
| POST | `/reports/{id}/review` | 审核举报（管理员） |

## 管理员接口

需要管理员角色。

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/management/summary` | 管理面板概览和审计日志 |
| GET | `/management/site-admins` | 站点管理员列表 |
| POST | `/management/site-admins` | 添加站点管理员 |
| DELETE | `/management/site-admins/{id}` | 移除站点管理员 |
| GET | `/management/index-rules` | 索引规则列表 |
| POST | `/management/index-rules` | 创建索引规则 |
| POST | `/management/index-rules/{id}` | 更新索引规则 |
| DELETE | `/management/index-rules/{id}` | 删除索引规则 |
| POST | `/management/users/{id}/role` | 设置用户角色 |
| POST | `/management/users/{id}/ban` | 封禁用户 |
| DELETE | `/skills/{slug}` | 软删除技能 |
| POST | `/skills/{slug}/restore` | 恢复已删除技能 |
| POST | `/skills/{slug}/moderation` | 更新审核状态 |

## WebSocket

连接 `/api/v1/ws` 获取实时索引任务进度。

```json
// 订阅任务
{"action": "subscribe", "job_id": "019..."}

// 接收进度事件
{"type": "index_progress", "job_id": "019...", "status": "running",
 "progress_pct": 50, "progress_message": "Scanning for skills..."}

// 取消订阅
{"action": "unsubscribe", "job_id": "019..."}
```

## 认证流程

Savhub 使用 GitHub OAuth 登录：

1. 前端跳转到 `GET /auth/github/start`
2. GitHub 回调到 `GET /auth/github/callback`
3. 后端创建会话 token 并带 token 重定向到前端
4. 前端将 token 存入 localStorage，后续请求通过 `Authorization: Bearer {token}` 发送
