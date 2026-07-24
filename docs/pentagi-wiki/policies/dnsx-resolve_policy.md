---
name: dnsx-resolve_policy
description: PentAGI safety policy for Dnsx Resolve. 使用 dnsx 进行批量 DNS 记录查询和解析。当需要批量 DNS 解析、验证子域名存活、查询 A/AAAA/CNAME/NS/MX/TXT 记录、通配符过滤时使用。dnsx 是 ProjectDiscovery 出品的高速 DNS 工具包，支持多记录类型查询和通配符自动过滤。任何涉及 DNS 解析、子域名验证、DNS 记录收集的场景都应使用此技能
when-to-use: Use when Dnsx Resolve methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Dnsx Resolve — PentAGI Safety Policy

> Policy compiled from `tool/dnsx-resolve/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/dnsx-resolve/SKILL.md` for knowledge reference
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

Source: `tool/dnsx-resolve/SKILL.md`
