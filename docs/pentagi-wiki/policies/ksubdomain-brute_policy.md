---
name: ksubdomain-brute_policy
description: PentAGI safety policy for Ksubdomain Brute. 使用 ksubdomain 进行无状态子域名爆破。当需要极高速子域名爆破和验证时使用。ksubdomain 直接操作网卡原始套接字发包，速度比 dnsx 快 10 倍，支持验证和枚举两种模式。需要 root 权限。任何涉及高速子域名爆破、大规模子域名验证的场景都应使用此技能
when-to-use: Use when Ksubdomain Brute methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Ksubdomain Brute — PentAGI Safety Policy

> Policy compiled from `tool/ksubdomain-brute/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/ksubdomain-brute/SKILL.md` for knowledge reference
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

Source: `tool/ksubdomain-brute/SKILL.md`
