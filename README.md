# 启元智能中台（Kairos Platform）

启元智能中台是面向智能业务研发与运营的一体化技术平台。平台以 Rust/Axum
作为统一服务底座，以 Vue 3、Vben 和 Ant Design Vue 提供管理控制面，并通过模块化
能力承载系统治理、AI 模型与 Agent、资产运营、自动化扫描、授权渗透测试和 Toon
内容生产等不同业务域。

## 功能组成

- System：认证、用户、角色、权限、菜单、租户及后台管理能力。
- Infra：配置、文件、任务、日志、数据源等基础设施能力。
- AI：统一模型管理、聊天/SSE、工具调用、知识库、图片、Midjourney、音乐、语音、Embedding 和写作。
- Pent：授权渗透任务、Agent 会话、工具路由、安全护栏、运行时编排与结果追踪。
- Scan：扫描器、扫描任务、资产发现及异构扫描结果归一化。
- Asset Ops：资产、机房、区域、端点、指纹与风险运营能力。
- Toonflow：项目、小说、剧本、事件、资产、分镜、音频、视频、Agent、提示词、Skill 和任务中心。
- Media：素材上传及媒体基础能力。

## 环境要求

- Rust stable（项目使用 Rust 2024 edition）
- PostgreSQL 18
- Node.js `22.18+` 或 `24.x`
- pnpm `11+`
- Docker 及 Docker Compose（推荐用于本地基础设施）

## 五分钟本地启动

### 1. 启动基础设施

```bash
docker compose -f script/docker/docker-compose.yml up -d
```

启动 PostgreSQL、Redis、NATS 和 MinIO。PostgreSQL 首次创建数据卷时会自动导入
`sql/postgresql/0001_initial.sql` 全量快照。

### 2. 启动 Rust 网关

```bash
export DATABASE_URL='postgres://kairos_platform:kairos_platform@127.0.0.1:5432/kairos_platform'
export REDIS_URL='redis://127.0.0.1:6379'
export JWT_SECRET='replace-with-at-least-32-random-bytes'
export BOOTSTRAP_ADMIN_USERNAME='admin'
export BOOTSTRAP_ADMIN_PASSWORD='Admin#123456'
cargo run -p kairos-platform-gateway
```

网关启动时只验证数据库已经初始化，不再自动执行 SQLx migration。全新环境必须先由
PostgreSQL 导入 `sql/postgresql/0001_initial.sql`；如果已有旧数据卷，Compose 不会再次导入，
需要备份后重建数据卷或按发布说明执行升级。

数据库发生结构或数据变化后，先更新维护中的参考数据库，再运行
`bash script/export-database-bootstrap.sh` 导出新的全量初始化快照。
开发期间可以临时创建 `0002_*.sql` 等增量脚本；确认其内容已经合并进新的
`0001_initial.sql` 后即可删除，发布的新环境只执行完整的 `0001_initial.sql`。

可选环境变量：
- `DATABASE_MIN_CONNECTIONS`（默认 1）
- `DATABASE_MAX_CONNECTIONS`（默认 20）
- `DATABASE_ACQUIRE_TIMEOUT_SECONDS`（默认 5）
- `GATEWAY_HOST`（默认 `0.0.0.0`）
- `GATEWAY_PORT`（默认 `8080`）
- `RUST_LOG`（推荐 `info`）

### 3. 启动 Vben 前端

```bash
cd apps/web
corepack enable
pnpm install
pnpm dev:antd
```

访问 `http://127.0.0.1:5666`，默认后端为 `http://127.0.0.1:8080`。

> 生产环境必须替换示例密码和 JWT 密钥。

## 验证

```bash
cargo test --workspace
bash script/test-database-migrations.sh
bash script/test-ai-e2e.sh
pnpm --dir apps/web --filter @vben/web-antd run typecheck
pnpm --dir apps/web --filter @vben/web-antd run build
```

## 文档

- [AI 启动交接指南](AGENTS.md)
- [技术架构](docs/technical-solution.md)
- [配置与模型接入](docs/configuration.md)
- [启动、部署与运维](docs/deployment.md)
- [功能范围与验收口径](docs/parity-roadmap.md)
