---
name: ctf-web-recon_policy
description: PentAGI safety policy for CTF Web Recon. CTF Web 挑战专用侦察方法。当面对 CTF 靶场目标需要快速发现攻击入口时使用。与真实渗透的 recon 不同——CTF 是单个应用、有意留线索、侦察应在 2-3 轮内完成。覆盖源码泄露、备份文件、隐藏路径、页面线索提取
when-to-use: Use when CTF Web Recon methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - todo_write
---

# CTF Web Recon — PentAGI Safety Policy

> Policy compiled from `ctf/ctf-web-recon/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ctf/ctf-web-recon/SKILL.md` for knowledge reference
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

Source: `ctf/ctf-web-recon/SKILL.md`
