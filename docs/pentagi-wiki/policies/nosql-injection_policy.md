---
name: nosql-injection_policy
description: PentAGI safety policy for Nosql Injection. NoSQL 注入检测与利用。当目标使用 MongoDB/CouchDB 等 NoSQL 数据库、登录表单使用 JSON body 提交、或参数中出现 $gt/$ne/$regex 等操作符时使用。覆盖操作符注入认证绕过、$regex 盲注数据提取、$where JS 注入、CouchDB 攻击、WAF 绕过
when-to-use: Use when Nosql Injection methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# NoSQL Injection — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/nosql-injection/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/nosql-injection/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/nosql-injection/SKILL.md`
