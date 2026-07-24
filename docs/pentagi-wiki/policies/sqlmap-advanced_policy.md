---
name: sqlmap-advanced_policy
description: PentAGI safety policy for Sqlmap Advanced. sqlmap 高级用法完整参考。当确认存在 SQL 注入需要用 sqlmap 自动化利用时使用。覆盖 POST/Cookie/Header 注入、tamper 脚本选择、--technique 精确控制、二次注入、OS shell/文件读写、数据库提取优化
when-to-use: Use when Sqlmap Advanced methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Sqlmap Advanced — PentAGI Safety Policy

> Policy compiled from `tool/sqlmap-advanced/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/sqlmap-advanced/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 未经 command_check 验证直接执行工具
- 对非授权目标使用工具
- 高危参数（--os-shell/--os-cmd/破坏性操作）

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

Source: `tool/sqlmap-advanced/SKILL.md`
