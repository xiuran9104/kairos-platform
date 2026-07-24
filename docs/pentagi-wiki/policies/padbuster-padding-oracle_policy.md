---
name: padbuster-padding-oracle_policy
description: PentAGI safety policy for Padbuster Padding Oracle. 使用 PadBuster 进行 Padding Oracle 攻击。当发现 Web 应用使用 CBC 模式加密且存在 Padding Oracle 漏洞时使用。PadBuster 可自动解密密文和伪造任意明文对应的合法密文，适用于加密 Cookie/Token/URL 参数。任何涉及 Padding Oracle 攻击、CBC 密文解密、Cookie 伪造的场景都应使用此技能
when-to-use: Use when Padbuster Padding Oracle methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Padbuster Padding Oracle — PentAGI Safety Policy

> Policy compiled from `tool/padbuster-padding-oracle/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/padbuster-padding-oracle/SKILL.md` for knowledge reference
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

Source: `tool/padbuster-padding-oracle/SKILL.md`
