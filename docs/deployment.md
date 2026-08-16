# 启动、部署与运维

AI 编码助手接手项目时应先阅读仓库根目录的 [AI 启动交接指南](../AGENTS.md)。该文件包含本地启动、新服务器启动、systemd、Nginx 和常见故障的可执行命令。

## 本地开发方案

1. 执行 `docker compose -f script/docker/docker-compose.yml up -d`。
2. 按 [配置文档](configuration.md) 导出数据库、JWT 和管理员环境变量。
3. 执行 `cargo run -p kairos-platform-gateway`（PostgreSQL 首次创建数据卷时已导入全量快照）。
4. 在 `apps/web` 执行 `pnpm install && pnpm dev:antd`。
5. 检查 `GET http://127.0.0.1:8080/health`，然后访问 `http://127.0.0.1:5666`。

数据库由 PostgreSQL 官方镜像在首次创建数据卷时导入
`sql/postgresql/0001_initial.sql`。Gateway 只验证关键表存在，不执行 migration。

## 后端生产构建

```bash
cargo build --release -p kairos-platform-gateway
```

产物为 `target/release/kairos-platform-gateway`。生产服务至少需要：

- `DATABASE_URL`
- 强随机 `JWT_SECRET`
- `GATEWAY_HOST` 和 `GATEWAY_PORT`
- 可选 `REDIS_URL`

推荐由 systemd、Docker、Kubernetes 或其他进程管理器注入环境变量并负责重启。多个网关实例可以共享 PostgreSQL 和 Redis；异步任务轮询使用持久化状态，重复轮询应由供应商任务查询接口保持幂等。

新服务器首次部署建议流程：

1. 安装 Rust stable、Docker Compose、Node.js `22.18+` 或 `24.x`，并通过 Corepack 使用 pnpm `11+`。
   如需使用 Pent 主动检测，另需在 pent-agent 宿主机安装 `bubblewrap`、`nmap`、`nuclei`、`sqlmap`、`ffuf`、
   `gobuster`、`whatweb`、`nikto`、`subfinder`、`amass`、`wfuzz`、`dirsearch`、
   `feroxbuster`、`dalfox` 等本地工具。
2. 克隆代码到固定目录，例如 `/opt/kairos-platform`。
3. 启动基础设施：`docker compose -f script/docker/docker-compose.yml up -d`。使用托管 PostgreSQL/Redis 时，改为在环境变量中指向托管地址。
4. 创建 `/etc/kairos-platform/gateway.env`，写入 `DATABASE_URL`、`REDIS_URL`、强随机 `JWT_SECRET`、`GATEWAY_HOST`、`GATEWAY_PORT`、`RUST_LOG` 和首次管理员变量。
5. 执行 `cargo build --release -p kairos-platform-gateway`。
6. 手动加载环境变量运行一次 `target/release/kairos-platform-gateway`，确认数据库验证成功和管理员可登录。
7. 管理员创建后，从环境文件移除 `BOOTSTRAP_ADMIN_PASSWORD`。
8. 使用 systemd、Docker 或 Kubernetes 托管网关进程。

systemd 示例：

```ini
[Unit]
Description=Kairos Platform Gateway
After=network-online.target docker.service
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=/opt/kairos-platform
EnvironmentFile=/etc/kairos-platform/gateway.env
ExecStart=/opt/kairos-platform/target/release/kairos-platform-gateway
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

启用服务：

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now kairos-platform-gateway
sudo systemctl status kairos-platform-gateway
curl -fsS http://127.0.0.1:8080/health
```

## 前端生产构建

```bash
corepack enable
pnpm --dir apps/web install --frozen-lockfile
pnpm --dir apps/web --filter @vben/web-antd run build
```

静态产物位于 `apps/web/apps/web-antd/dist`，可由 Nginx 或对象存储/CDN 托管。SPA 部署需要把未知前端路径回退至 `index.html`，并将 `/api/` 反向代理到 Rust 网关。

Nginx 核心配置示例：

```nginx
location / {
    try_files $uri $uri/ /index.html;
}

location /api/ {
    proxy_pass http://kairos-platform-gateway:8080/;
    proxy_http_version 1.1;
    proxy_buffering off; # SSE 必须关闭代理缓冲
    proxy_read_timeout 600s;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
}
```

## 数据库初始化与升级

`sql/postgresql/0001_initial.sql` 是新环境的唯一初始化基线，包含当前完整 schema 和数据。

- Compose 将快照只读挂载到
  `/docker-entrypoint-initdb.d/0001_initial.sql`。
- PostgreSQL 只在数据目录为空时执行初始化脚本；已有数据卷不会重复导入。
- Gateway 连接数据库后验证关键表，数据库未初始化时立即报错退出。
- 开发或存量数据库升级期间可以临时创建 `0002_*.sql`、`0003_*.sql` 等增量
  脚本；应用到参考数据库并重新导出 `0001_initial.sql` 后，确认变更已经合并，
  再删除这些临时增量脚本。最终发布的新环境仍只使用 `0001_initial.sql` 初始化。
- 数据库变化先应用到维护中的参考数据库，再运行
  `bash script/export-database-bootstrap.sh` 生成最新快照。
- 发布快照前必须检查凭据、Token、日志和其他不应分发的运行数据。
- 发布前运行 `bash script/test-database-migrations.sh`，验证全量快照可以初始化空的
  PostgreSQL 18。脚本名称暂时保留以兼容现有 CI。
- 替换快照不会升级已有数据卷；生产升级必须先备份，并执行该版本单独提供的升级步骤。

全新本地数据库需要显式删除旧数据卷后重新创建。删除数据卷会清除本地数据库，
必须先执行 `bash script/export-database-bootstrap.sh` 或另行备份。

### 生产数据库备份

`0001_initial.sql` 是可分发的初始化基线，不代替生产备份。升级、重建数据卷或执行
高风险数据操作前运行：

```bash
DATABASE_URL='postgres://user:password@host:5432/kairos_platform' \
  bash script/backup-database.sh
```

脚本默认在 `backups/postgresql/` 生成类似
`kairos_platform_20260724_103000.dump` 的 PostgreSQL custom-format 文件，
文件权限为 `0600`，且不会覆盖 `0001_initial.sql`。可以通过
`DATABASE_BACKUP_DIR=/secure/backup/path` 指定外部备份目录。

恢复到一个已经创建的空数据库：

```bash
pg_restore \
  --dbname='postgres://user:password@host:5432/kairos_platform_restore' \
  --clean --if-exists \
  /secure/backup/path/kairos_platform_20260724_103000.dump
```

本地 `backups/` 已加入 Git 忽略规则；生产备份仍需复制到有访问控制、加密和保留
策略的持久存储。

## 首次管理员

仅首次部署设置：

```bash
export BOOTSTRAP_ADMIN_USERNAME=admin
export BOOTSTRAP_ADMIN_PASSWORD='替换为高强度密码'
```

管理员存在后可以移除 `BOOTSTRAP_ADMIN_PASSWORD`，网关会跳过初始化。不要把生产密码提交到仓库或镜像。

## 健康检查与日志

- 存活检查：`GET /health`
- OpenAPI 文档：`GET /openapi.json`
- 请求自动生成或透传 `x-request-id`
- 使用 `RUST_LOG=info` 或模块级过滤规则控制 tracing 输出

Pent 相关运行检查：

```bash
curl -fsS http://127.0.0.1:8080/health
curl -fsS http://127.0.0.1:9222/json/version
curl -fsS -X PUT 'http://127.0.0.1:9222/json/new?http%3A%2F%2F127.0.0.1%3A5666%2F'
for tool in nmap nuclei sqlmap ffuf gobuster whatweb nikto subfinder amass wfuzz dirsearch feroxbuster dalfox; do
  command -v "$tool" >/dev/null && echo "ok $tool" || echo "missing $tool"
done
/usr/bin/nmap -sT -Pn --reason -- 127.0.0.1
```

Pent 主动检测工具由 pent-agent 宿主机上的 bubblewrap 沙箱执行。沙箱使用本地工具和文件系统，
不创建 OCI/runc 容器或工具镜像；`/workspace` 与 `/artifacts` 默认持久化在
`PENTA_SANDBOX_ROOT`（默认 `/tmp/kairos-pent-sandbox`）下。沙箱隔离 mount、PID、user、IPC、UTS
和 cgroup namespace，清空 capabilities 并加载 seccomp-bpf；按当前部署决策复用宿主机网络栈，
不提供 network namespace 或内核级内网阻断。Compose 中的
Docker 服务只承担数据库、缓存、消息队列、对象存储、图数据库、headless Chrome 和代理等
基础设施职责。

## 测试和发布检查

```bash
cargo fmt --all -- --check
cargo test --workspace
bash script/test-database-migrations.sh
bash script/test-ai-e2e.sh
curl -fsS http://127.0.0.1:9222/json/version
/usr/bin/nmap -sT -Pn --reason -- 127.0.0.1
pnpm --dir apps/web --filter @vben/web-antd run typecheck
pnpm --dir apps/web --filter @vben/web-antd run build
```

`test-ai-e2e.sh` 会启动临时 PostgreSQL，并用本地模拟模型验证 JWT、普通聊天、SSE、消息落库及 Midjourney Imagine/Action 状态机，不调用外部付费模型。

## 常见问题

- 网关提示 `DATABASE_URL is required`：未配置数据库连接串。
- 网关提示 JWT 密钥过短：`JWT_SECRET` 必须至少 32 字节。
- 没有初始管理员：确认首次启动时设置了 `BOOTSTRAP_ADMIN_PASSWORD`。
- SSE 到前端后一次性出现：关闭 Nginx/Ingress 的响应缓冲并增加读取超时。
- AI 任务一直处理中：检查模型配置的任务查询路径、鉴权头和供应商任务 ID；可调用 `/poll` 接口立即同步。
- 前端请求 404：确认 Vben API 基址或 Nginx `/api/` 转发是否去掉了正确的前缀。
- Pent 工具调用提示命令不存在：在 pent-agent 宿主机安装对应二进制，并确认 `command -v <tool>` 可找到。
- `mcp_navigate_page` 提示 MCP 不可用：先检查 `http://127.0.0.1:9222/json/version`。
  Chrome DevTools 可用时，网关会使用 HTTP fallback 打开页面。
