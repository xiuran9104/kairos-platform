---
name: redis-attack_policy
description: PentAGI safety policy for Redis Attack. Redis 未授权访问与利用。当发现目标开放 6379 端口、Redis 服务无认证或弱密码、需要从 Redis 获取敏感数据或实现远程命令执行时使用。覆盖未授权访问、数据窃取、crontab 写入 RCE、主从复制 RCE、模块加载 RCE、webshell 写入、SSH 公钥注入、Lua 脚本注入、Pub/Sub 窃听、持久化文件利用
when-to-use: Use when Redis Attack methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Redis Attack — PentAGI Safety Policy

> Policy compiled from `exploit/network-service/redis-attack/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/network-service/redis-attack/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 无 confirm_authorized 的主动利用
- 破坏性 payload（DROP/DELETE/rm -rf）
- 直接执行 OS 命令
- 写入 webshell 或后门文件

## Escalation Path

If the methodology requires blocked operations:
1. Stop automated execution
2. Document the specific need and target
3. Request explicit human authorization with scope boundaries
4. Wait for approval before proceeding

## Evidence Requirements

- Record every probe attempt with request/response evidence
- Link findings to the source methodology path
- Mark unverified observations as `needs_review`

---

Source: `exploit/network-service/redis-attack/SKILL.md`
