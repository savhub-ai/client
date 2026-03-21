---
title: 部署配置
description: 环境变量和服务器部署
---

# 部署配置

所有配置通过环境变量设置。将 `.env.example` 复制为 `.env`（或 `.env.local`）并填写对应值。

## 必填变量

| 变量 | 说明 | 示例 |
|------|------|------|
| `DATABASE_URL` | PostgreSQL 连接字符串 | `postgres://postgres:postgres@127.0.0.1:45432/savhub_dev` |
| `SAVHUB_GITHUB_CLIENT_ID` | GitHub OAuth 应用 Client ID | `Ov23li...` |
| `SAVHUB_GITHUB_CLIENT_SECRET` | GitHub OAuth 应用 Secret | `70733a...` |
| `SAVHUB_GITHUB_REDIRECT_URL` | GitHub OAuth 回调地址 | `http://127.0.0.1:5006/api/v1/auth/github/callback` |

## 服务器设置

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `SAVHUB_BIND` | 后端监听地址 | `127.0.0.1:5006` |
| `SAVHUB_FRONTEND_ORIGIN` | 前端地址（CORS） | `http://127.0.0.1:5007` |
| `SAVHUB_API_BASE` | 公开 API 基础地址 | `http://{SAVHUB_BIND}/api/v1` |
| `SAVHUB_SPACE_PATH` | 数据目录（registry checkout 和 repo 缓存） | `./space` |

## 用户角色

| 变量 | 说明 |
|------|------|
| `SAVHUB_GITHUB_ADMIN_LOGINS` | 逗号分隔的 GitHub 用户名，首次登录时授予管理员角色 |
| `SAVHUB_GITHUB_MODERATOR_LOGINS` | 逗号分隔的 GitHub 用户名，首次登录时授予版主角色 |

## 后台任务

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `SAVHUB_SYNC_INTERVAL_SECS` | 后台 flock 同步间隔（秒） | `300` |
| `SAVHUB_SYNC_STALE_HOURS` | flock 视为过期需要重新同步的小时数 | `6` |
| `SAVHUB_AUTO_INDEX_MIN_INTERVAL_SECS` | 每个仓库自动索引检查的最小间隔（秒） | `3600` |

## GitHub OAuth 设置

创建 GitHub OAuth 应用，使用以下设置：

- **Homepage URL**: `http://127.0.0.1:5007`
- **Authorization callback URL**: `http://127.0.0.1:5006/api/v1/auth/github/callback`

## Docker Compose

```bash
# 设置必要的环境变量
export SAVHUB_REGISTRY_GIT_TOKEN=ghp_xxx

# 启动所有服务
docker compose up
```
