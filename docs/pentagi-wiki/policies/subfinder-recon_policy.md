---
name: subfinder-recon_policy
description: PentAGI safety policy for Subfinder Recon. 使用 subfinder 进行被动子域名枚举。当需要发现目标域名的子域名、扩展攻击面时使用。subfinder 是 ProjectDiscovery 出品的被动子域名发现工具，聚合 Shodan、Censys、SecurityTrails、VirusTotal 等多数据源，快速且隐蔽。任何涉及子域名枚举、攻击面发现、被动信息收集的场景都应使用此技能
when-to-use: Use when Subfinder Recon methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Subfinder Recon — PentAGI Safety Policy

> Policy compiled from `tool/subfinder-recon/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/subfinder-recon/SKILL.md` for knowledge reference
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

Source: `tool/subfinder-recon/SKILL.md`
