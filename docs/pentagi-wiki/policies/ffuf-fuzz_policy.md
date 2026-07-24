---
name: ffuf-fuzz_policy
description: PentAGI safety policy for Ffuf Fuzz. ffuf 模糊测试工具完整参考。用于目录/文件发现、参数 Fuzz、虚拟主机枚举、POST 数据 Fuzz、多位置 Fuzz。当需要对 Web 应用进行路径爆破、参数发现、子域名枚举时使用。比 gobuster/dirsearch 更灵活——支持多 FUZZ 位置和自定义过滤。任何需要 Web 路径或参数暴力枚举的场景都应使用此 skill
when-to-use: Use when Ffuf Fuzz methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Ffuf Fuzz — PentAGI Safety Policy

> Policy compiled from `tool/ffuf-fuzz/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/ffuf-fuzz/SKILL.md` for knowledge reference
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

Source: `tool/ffuf-fuzz/SKILL.md`
