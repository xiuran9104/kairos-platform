# Kairos Platform 技术方案

## 目标

Kairos Platform 对齐 yudao-cloud 的工程思想，但使用 Rust 技术栈实现：后端采用 `framework + modules + gateway` 的 workspace 结构，前端使用 Vben Admin 5.x，部署脚本和 SQL 脚本独立放置。

## 总体架构

```text
browser
  |
  v
apps/web                         # Vben Admin 5.x
  |
  v
services/gateway                 # 统一 HTTP 网关
  |
  +-- crates/modules/system-*     # 系统模块：用户、认证、权限、租户
  +-- crates/modules/infra-*      # 基础设施模块：配置、文件、任务、监控
  +-- crates/modules/ai-*         # 统一 AI 模型、聊天、知识库和媒体生成
  +-- crates/modules/media-*      # 媒体模块：素材、上传、转码、存储

crates/framework/*               # 公共 starter / framework 能力
```

## 目录结构

```text
apps/
  web/                            # Vben Admin 前端 monorepo
crates/
  framework/
    common/                       # 配置、健康检查、统一响应、启动器
    database/                     # SQLx 连接池、迁移、数据库健康探测
    web/                          # Web 中间件、错误映射、请求追踪
    security/                     # JWT、权限与当前用户上下文
    redis/                        # Redis 客户端、缓存与限流
    mq/                           # 事件、发布订阅与重试契约
    tenant/                       # 租户上下文与数据隔离
    telemetry/                    # tracing 与审计基础能力
  modules/
    system-api/
    system-server/
    ai-api/
    ai-server/
    infra-api/
    infra-server/
    media-api/
    media-server/
services/
  gateway/
sql/
  postgresql/
script/
  docker/
```

## 模块边界

每个业务模块拆成 `api` 和 `server`：

- `*-api`：模块对外契约，放 DTO、事件定义、客户端可复用类型。
- `*-server`：模块服务实现，放 HTTP 路由、应用服务、领域逻辑和基础设施适配。
- `gateway`：统一入口，只组合模块路由和横切中间件，不放业务规则。
- `framework`：类似 yudao-cloud 的 starter，沉淀通用能力。

Axum 网关的生产化演进、流量治理、审计队列和可观测性实施计划见
[Axum 网关优化方案](./Axum网关优化方案.md)。

模块内部后续继续按下列目录演进：

```text
src/
  domain/          # 实体、值对象、领域规则
  application/     # 用例、命令、查询、事务编排
  transport/       # HTTP/gRPC/MQ 入口
  infrastructure/  # DB、Redis、MQ、对象存储适配
```

依赖方向：

```text
gateway -> module-server -> module-api
module-server -> framework/*
framework/* 不依赖具体业务模块
```

## 前端方案

前端使用 Vben Admin 5.x，默认主应用为 `@vben/web-antd`。开发环境：

- `VITE_GLOB_API_URL=/api`
- Vite proxy：`/api -> http://localhost:8080`
- 后端统一响应格式：`{ "code": 0, "data": ..., "message": "ok" }`

后续页面建议按模块组织：

```text
views/
  system/
  infra/
  ai/
  media/
```

## 服务入口

网关默认端口 `8080`：

- `GET /health`
- `POST /system/auth/login`
- `GET /system/auth/me`（Bearer Token）
- `/ai/**`：模型、聊天、知识库、图片、音乐、工具与写作
- `/system/**`、`/infra/**`、`/media/**`：基础后台模块

## 本地运行

后端：

```bash
export DATABASE_URL=postgres://kairos_platform:kairos_platform@localhost:5432/kairos_platform
export JWT_SECRET='replace-with-at-least-32-random-bytes'
cargo run -p kairos-platform-gateway
```

前端：

```bash
cd apps/web
pnpm install
pnpm dev:antd
```

基础设施：

```bash
docker compose -f script/docker/docker-compose.yml up -d
```

## 已实现的 Web 基础能力

- 统一成功响应与错误响应：`{ code, data, message }`。
- 未匹配路由返回 JSON `404`，不再返回纯文本。
- 自动生成并透传 `x-request-id`，方便跨服务排查。
- HTTP 请求 tracing 与开发环境 CORS。

## 权限模型

权限框架采用 RBAC：用户关联角色，角色关联权限，角色同时定义数据范围。权限码使用
`模块:资源:动作` 格式，例如 `system:user:read`，并支持受控通配符
`system:user:*`。数据库使用 `roles`、`permissions`、`user_roles`、
`role_permissions` 四张关系表，运行时将多角色权限合并为 `PermissionSet` 后进行鉴权。

安全框架提供严格校验的 `SecurityConfig`、HS256 Access Token 签发与验证、
Bearer 认证中间件及 `CurrentUser` 请求提取器。JWT 强制校验签发方、受众、签名和
过期时间，密钥不得少于 32 字节，默认有效期为 15 分钟。公开接口与受保护接口通过
路由分层装配，不依赖容易遗漏的路径白名单。

数据库框架统一读取 `DATABASE_URL` 及连接池参数，启动时验证
`sql/postgresql/0001_initial.sql` 已完成初始化，并提供数据库 readiness 探测。数据库 URL
的调试输出会自动脱敏。密码使用 Argon2id
随机盐哈希，默认策略要求至少 12 位并包含大小写字母、数字和符号。
