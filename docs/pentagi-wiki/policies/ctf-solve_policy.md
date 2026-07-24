---
name: ctf-solve_policy
description: PentAGI safety policy for CTF Solve. CTF 综合解题编排器。当面对未知类型的 CTF 挑战、需要自动分析挑战类型并选择正确解题路径时使用。自动调度对应的专项 skill（pwn/crypto/web/reverse/forensics/osint/malware/misc），适合给定挑战文件或服务端点但不确定属于哪个类别的场景
when-to-use: Use when CTF Solve methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - todo_write
---

# CTF Solve — PentAGI Safety Policy

> Policy compiled from `ctf/ctf-solve/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ctf/ctf-solve/SKILL.md` for knowledge reference
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

Source: `ctf/ctf-solve/SKILL.md`
