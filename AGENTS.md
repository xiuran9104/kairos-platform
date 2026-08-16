# AI Startup Guide

This file is the handoff guide for AI coding agents working in this repository. Read it before starting services or changing deployment docs.

## Project Shape

- Backend: Rust workspace, gateway entrypoint at `services/gateway`.
- Frontend: Vben Admin app at `apps/web`, main app package `@vben/web-antd`.
- Database bootstrap: `sql/postgresql/0001_initial.sql` is the complete `pg_dump` snapshot and the sole initialization source for a new PostgreSQL data volume.
- Numbered files such as `0002_*.sql` may be created temporarily while developing or upgrading an existing database. The gateway does not execute them automatically.
- Local infrastructure: PostgreSQL, Redis, NATS, and MinIO via `script/docker/docker-compose.yml`.

Do not mount the `sql/postgresql` directory into PostgreSQL init scripts. Mount only
`sql/postgresql/0001_initial.sql` as `/docker-entrypoint-initdb.d/0001_initial.sql`;
the official PostgreSQL image imports it when creating a new data volume.

## Database Change Workflow

Whenever a database schema or baseline-data change is made:

1. Create a temporary numbered SQL file such as `0002_<change>.sql` when the
   change needs a reviewable or repeatable upgrade step.
2. Apply the SQL change to the maintained reference database.
3. Run `bash script/export-database-bootstrap.sh` to replace
   `sql/postgresql/0001_initial.sql` with a complete owner/ACL-independent dump.
4. Review the dump for secrets and transient data that must not be distributed.
5. After confirming the change is present in `0001_initial.sql`, delete the
   temporary `0002_*.sql`, `0003_*.sql`, and later files that have been consolidated.
6. Run `bash script/test-database-migrations.sh`; despite its historical name,
   this test now proves the snapshot initializes an empty PostgreSQL instance.
7. Update relevant bootstrap assertions in
   `crates/framework/database/tests/migrations.rs`.

Changes to an already initialized production database must be applied through an
explicit, reviewed upgrade procedure before exporting the next snapshot. Replacing
`0001_initial.sql` only affects newly created PostgreSQL data volumes.

Before a production database upgrade, create a timestamped backup that is separate
from the initialization snapshot:

```bash
DATABASE_URL='postgres://...' bash script/backup-database.sh
```

Backups default to `backups/postgresql/`, are ignored by Git, and must be copied to
durable encrypted storage according to the deployment's retention policy.

## Local Development Startup

From the repository root:

```bash
docker compose -f script/docker/docker-compose.yml up -d
```

Start the backend gateway:

```bash
export DATABASE_URL='postgres://kairos_platform:kairos_platform@127.0.0.1:5432/kairos_platform'
export REDIS_URL='redis://127.0.0.1:6379'
export JWT_SECRET='local-development-jwt-secret-change-me-32bytes'
export BOOTSTRAP_ADMIN_USERNAME='admin'
export BOOTSTRAP_ADMIN_PASSWORD='Admin#123456'
export RUST_LOG='info'
cargo run -p kairos-platform-gateway
```

In another terminal, start the execution worker. The gateway never executes
security tools:

```bash
export DATABASE_URL='postgres://kairos_platform:kairos_platform@127.0.0.1:5432/kairos_platform'
export PENTA_WORKER_ID='local-01'
cargo run -p kairos-platform-pent-agent
```

In a second terminal, start the frontend:

```bash
cd apps/web
corepack enable
pnpm install
pnpm dev:antd
```

Open:

- Frontend: `http://127.0.0.1:5666`
- Backend health: `http://127.0.0.1:8080/health`
- OpenAPI: `http://127.0.0.1:8080/openapi.json`
- MinIO console: `http://127.0.0.1:9001`

Default local bootstrap account:

- Username: `admin`
- Password: `Admin#123456`

`BOOTSTRAP_ADMIN_PASSWORD` is only used to create the initial admin when it does not exist. If the database already has admins, startup skips creating another one.

## Local Verification

Use these checks after startup:

```bash
curl -fsS http://127.0.0.1:8080/health
cargo test --workspace
bash script/test-database-migrations.sh
pnpm --dir apps/web --filter @vben/web-antd run typecheck
```

Frontend production build check:

```bash
pnpm --dir apps/web --filter @vben/web-antd run build
```

## New Server Startup

Install prerequisites:

- Rust stable with Rust 2024 edition support.
- Docker and Docker Compose.
- Bubblewrap (`bwrap`) on every host that runs `kairos-platform-pent-agent`.
- Node.js `22.18+` or `24.x`.
- pnpm `11+` through Corepack.
- Nginx or another reverse proxy for production frontend/API routing.

Clone and enter the repository:

```bash
git clone <repo-url> kairos-platform
cd kairos-platform
```

Start infrastructure. For a single-server deployment, the repository compose file is enough:

```bash
docker compose -f script/docker/docker-compose.yml up -d
```

For managed PostgreSQL or Redis, skip those compose services and set `DATABASE_URL` / `REDIS_URL` to the managed endpoints.

Create a backend environment file outside git, for example `/etc/kairos-platform/gateway.env`:

```bash
DATABASE_URL=postgres://kairos_platform:kairos_platform@127.0.0.1:5432/kairos_platform
REDIS_URL=redis://127.0.0.1:6379
JWT_SECRET=replace-with-a-strong-random-secret-at-least-32-bytes
GATEWAY_HOST=0.0.0.0
GATEWAY_PORT=8080
RUST_LOG=info
BOOTSTRAP_ADMIN_USERNAME=admin
BOOTSTRAP_ADMIN_PASSWORD=replace-with-a-strong-initial-password
```

Build the backend:

```bash
cargo build --release -p kairos-platform-gateway
cargo build --release -p kairos-platform-pent-agent
```

Run once manually to verify the imported database and initial admin creation:

```bash
set -a
. /etc/kairos-platform/gateway.env
set +a
./target/release/kairos-platform-gateway
```

After the first successful login, remove `BOOTSTRAP_ADMIN_PASSWORD` from `/etc/kairos-platform/gateway.env` and restart the service.

## systemd Service

Use systemd or another process manager in production. Example `/etc/systemd/system/kairos-platform-gateway.service`:

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

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now kairos-platform-gateway
sudo systemctl status kairos-platform-gateway
curl -fsS http://127.0.0.1:8080/health
```

## Frontend Production

Build the frontend:

```bash
corepack enable
pnpm --dir apps/web install --frozen-lockfile
pnpm --dir apps/web --filter @vben/web-antd run build
```

Static output:

```text
apps/web/apps/web-antd/dist
```

Serve that directory through Nginx or a CDN. Route SPA paths to `index.html`, and reverse proxy API traffic to the gateway.

Minimal Nginx location rules:

```nginx
location / {
    try_files $uri $uri/ /index.html;
}

location /api/ {
    proxy_pass http://127.0.0.1:8080/;
    proxy_http_version 1.1;
    proxy_buffering off;
    proxy_read_timeout 600s;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
}
```

## Common Problems

- `DATABASE_URL is required`: export it or add it to the systemd environment file.
- JWT startup error: `JWT_SECRET` must be at least 32 bytes.
- No admin account: set `BOOTSTRAP_ADMIN_PASSWORD` for the first startup, then remove it after the account exists.
- Frontend API 404: check `VITE_BASE_URL`, `VITE_GLOB_API_URL`, and Nginx `/api/` proxy prefix handling.
- SSE responses arrive all at once: disable proxy buffering and increase read timeout.
- Port already in use: check `ss -ltnp | rg ':(8080|5666|5432|6379)'`.
