---
name: sql-injection-methodology_policy
description: PentAGI safety policy for Sql Injection Methodology. SQL注入检测、利用、绕过的完整方法论。当目标有表单提交、登录页面、搜索功能、数据查询接口时使用。包含POST参数完整性检查、EXTRACTVALUE截断陷阱、UNION/报错/盲注/sqlmap全流程
when-to-use: Use when Sql Injection Methodology methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# Sql Injection Methodology — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/sql-injection-methodology/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/sql-injection-methodology/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/sql-injection-methodology/SKILL.md`
