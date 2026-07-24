---
name: naabu-portscan_policy
description: PentAGI safety policy for Naabu Portscan. 使用 naabu 进行高速端口扫描。当需要对目标主机/网段进行端口发现、存活检测时使用。naabu 是 ProjectDiscovery 出品的快速端口扫描器，支持 SYN/CONNECT/UDP 扫描，性能远超 nmap，支持批量目标、CDN 排除、nmap 集成。任何涉及端口扫描、端口发现、主机存活检测、网段探测的场景都应使用此技能。如果 naabu 结果不足再降级用 nmap
when-to-use: Use when Naabu Portscan methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Naabu Portscan — PentAGI Safety Policy

> Policy compiled from `tool/naabu-portscan/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/naabu-portscan/SKILL.md` for knowledge reference
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

Source: `tool/naabu-portscan/SKILL.md`
