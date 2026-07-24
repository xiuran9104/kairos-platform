---
name: mapcidr-cidr_policy
description: PentAGI safety policy for Mapcidr Cidr. 使用 mapcidr 进行 CIDR 网段处理。当需要展开 CIDR 为 IP 列表、聚合 IP 为 CIDR、切片网段、IP 数量统计、打乱 IP 顺序时使用。mapcidr 是 ProjectDiscovery 出品的 CIDR 处理工具，支持 IPv4/IPv6。任何涉及 CIDR 计算、IP 列表处理、网段切片的场景都应使用此技能
when-to-use: Use when Mapcidr Cidr methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Mapcidr Cidr — PentAGI Safety Policy

> Policy compiled from `tool/mapcidr-cidr/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/mapcidr-cidr/SKILL.md` for knowledge reference
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

Source: `tool/mapcidr-cidr/SKILL.md`
