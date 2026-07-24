---
name: mongodb-attack_policy
description: PentAGI safety policy for Mongodb Attack. MongoDB 未授权访问与 NoSQL 注入利用。当发现目标开放 27017 端口、MongoDB 服务无认证、Web 应用使用 MongoDB 后端存在 NoSQL 注入、或需要从 MongoDB 提取数据时使用。覆盖未授权访问、数据库枚举与导出、NoSQL 注入（$ne/$regex/$where/盲注）、JavaScript 执行（mapReduce/eval）、权限提升、GridFS 文
when-to-use: Use when Mongodb Attack methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Mongodb Attack — PentAGI Safety Policy

> Policy compiled from `exploit/network-service/mongodb-attack/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/network-service/mongodb-attack/SKILL.md` for knowledge reference
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

Source: `exploit/network-service/mongodb-attack/SKILL.md`
