---
name: ctf-flag-verification_policy
description: PentAGI safety policy for CTF Flag Verification. CTF/靶场 Flag 强制验证流程。当通过**任何方式**发现疑似 flag 字符串（含 flag{、FLAG{、ctf{ 等格式）时**必须立即**使用此 skill 验证，不要直接提交。防止因字符截断、编码错误、HTML 实体、base64 不完整解码、hex 截断等原因导致提交错误 flag。即使 flag 看起来完整，也可能存在隐藏字符或编码问题。覆盖 SQL 注入分段提取、命令执行输出
when-to-use: Use when CTF Flag Verification methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - todo_write
---

# CTF Flag Verification — PentAGI Safety Policy

> Policy compiled from `ctf/ctf-flag-verification/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ctf/ctf-flag-verification/SKILL.md` for knowledge reference
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

Source: `ctf/ctf-flag-verification/SKILL.md`
