---
name: uncover-search_policy
description: PentAGI safety policy for Uncover Search. 使用 uncover 聚合查询网络空间搜索引擎。当需要从 Shodan/Censys/FOFA/Hunter/Quake/ZoomEye 等引擎快速发现暴露资产时使用。uncover 统一了多个搜索引擎的查询接口，一条命令查询多个引擎。任何涉及资产发现、互联网测绘、暴露面排查的场景都应使用此技能
when-to-use: Use when Uncover Search methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Uncover Search — PentAGI Safety Policy

> Policy compiled from `tool/uncover-search/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/uncover-search/SKILL.md` for knowledge reference
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

Source: `tool/uncover-search/SKILL.md`
