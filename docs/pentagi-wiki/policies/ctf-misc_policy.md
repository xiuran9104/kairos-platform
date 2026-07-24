---
name: ctf-misc_policy
description: PentAGI safety policy for CTF Misc. CTF 杂项挑战技术。当遇到编码谜题、Python/Bash 沙箱逃逸、RF/SDR 信号、游戏/VM 逆向、K8s RBAC、浮点数技巧、Z3 约束求解、博弈论等不属于 pwn/crypto/web/reverse/forensics/osint 的 CTF 挑战时使用。先排除其他分类后再使用本技能
when-to-use: Use when CTF Misc methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - todo_write
---

# CTF Misc — PentAGI Safety Policy

> Policy compiled from `ctf/ctf-misc/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ctf/ctf-misc/SKILL.md` for knowledge reference
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

Source: `ctf/ctf-misc/SKILL.md`
