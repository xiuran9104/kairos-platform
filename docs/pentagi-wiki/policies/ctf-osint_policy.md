---
name: ctf-osint_policy
description: PentAGI safety policy for CTF Osint. CTF 开源情报(OSINT)技术。当挑战要求从公开信息中找线索——如给定用户名/邮箱追踪身份、给定照片进行地理定位、从历史网页快照中恢复数据时使用。覆盖社交媒体调查、Google Dorking、反向图片搜索、Wayback Machine、DNS 侦察、Tor 中继查询、元数据提取
when-to-use: Use when CTF Osint methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - todo_write
---

# CTF Osint — PentAGI Safety Policy

> Policy compiled from `ctf/ctf-osint/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ctf/ctf-osint/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous

## Blocked Operations

- 对非 CTF 目标使用
- 破坏 CTF 基础设施
- 泄露 flag 给未授权方

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

Source: `ctf/ctf-osint/SKILL.md`
