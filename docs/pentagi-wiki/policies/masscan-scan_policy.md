---
name: masscan-scan_policy
description: PentAGI safety policy for Masscan Scan. 使用 masscan 进行超大规模高速端口扫描。当需要扫描大范围 CIDR 网段、全互联网规模扫描时使用。masscan 是异步无状态扫描器，速度可达每秒百万包级别，适合大规模资产发现。需要 root 权限。任何涉及大规模网段扫描、高速端口发现、互联网测绘的场景都应使用此技能
when-to-use: Use when Masscan Scan methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Masscan Scan — PentAGI Safety Policy

> Policy compiled from `tool/masscan-scan/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/masscan-scan/SKILL.md` for knowledge reference
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

Source: `tool/masscan-scan/SKILL.md`
