---
name: ctf-web-methodology_policy
description: PentAGI safety policy for CTF Web Methodology. CTF Web 挑战的总体方法论。当面对 CTF 靶场、xbow benchmark、Web 安全挑战赛时使用。包含挑战类型快速识别、攻击策略选择决策树、常见出题模式、何时切换方向的判断标准。与 exploit/ 下的具体漏洞技能配合使用
when-to-use: Use when CTF Web Methodology methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - todo_write
---

# CTF Web Methodology — PentAGI Safety Policy

> Policy compiled from `ctf/ctf-web-methodology/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ctf/ctf-web-methodology/SKILL.md` for knowledge reference
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

Source: `ctf/ctf-web-methodology/SKILL.md`
