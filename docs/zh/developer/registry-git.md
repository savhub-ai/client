---
title: Registry Git 访问
description: 配置 registry 仓库的推送权限
---

# Registry Git 访问

后端维护一个 registry git 仓库的本地 checkout。每次扫描完成后，会提交索引数据（JSON 文件）并推送到远程。选择以下**一种**认证方式。

## 方式 A: HTTPS Token（推荐）

最适合 GitHub 仓库。Token 会自动嵌入到远程 URL 中。

| 变量 | 说明 |
|------|------|
| `SAVHUB_REGISTRY_GIT_URL` | Registry 仓库 URL。默认：`https://github.com/savhub-ai/registry.git` |
| `SAVHUB_REGISTRY_GIT_TOKEN` | GitHub Personal Access Token |

### 生成 Token

1. 进入 GitHub -> Settings -> Developer settings -> Personal access tokens -> Fine-grained tokens
2. 选择 registry 仓库
3. 授权权限：**Contents**（Read and write）
4. 复制 token，设置为 `SAVHUB_REGISTRY_GIT_TOKEN`

Token 会被嵌入为 `https://x-access-token:{token}@github.com/...`。

## 方式 B: SSH 密钥

适合自建 git 服务器或偏好 SSH 的场景。

| 变量 | 说明 |
|------|------|
| `SAVHUB_REGISTRY_GIT_URL` | SSH 地址，如 `git@github.com:savhub-ai/registry.git` |
| `SAVHUB_REGISTRY_GIT_SSH_KEY` | Base64 编码的 SSH 私钥 |

### 编码密钥

```bash
# Linux / macOS
base64 -w0 < ~/.ssh/id_ed25519

# Windows PowerShell
[Convert]::ToBase64String([IO.File]::ReadAllBytes("$HOME\.ssh\id_ed25519"))
```

密钥在运行时解码，写入临时文件（Unix 上设置 `0600` 权限），通过 `GIT_SSH_COMMAND` 使用。

## 方式 C: 无凭证

如果既没有设置 token 也没有 SSH 密钥，后端依赖系统的 git 凭证助手。当机器已配置推送权限时可用（如通过 `gh auth` 或全局 credential store）。

## 工作原理

1. 启动时，后端 clone（或 pull）registry 仓库到 `{SAVHUB_SPACE_PATH}/registry/`
2. 每个索引任务完成后，变更的文件会被暂存并提交
3. 单次 `git push` 将变更推送到远程
4. 所有 registry 写入通过进程级锁串行化，防止冲突
5. Git 身份配置为 `savhub-bot <aston@sonc.ai>`
