---
name: 401-403-bypass_policy
description: PentAGI safety policy for 401 403 Bypass. 401/403 访问拒绝绕过方法论。当遇到管理后台、API 端点返回 401/403 Forbidden 时使用。覆盖路径操纵、HTTP 方法篡改、Header 注入、协议降级、组合攻击
when-to-use: Use when 401 403 Bypass methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# 401 403 Bypass — PentAGI Safety Policy

> Policy compiled from `exploit/auth/401-403-bypass/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/auth/401-403-bypass/SKILL.md` for knowledge reference
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

Source: `exploit/auth/401-403-bypass/SKILL.md`
