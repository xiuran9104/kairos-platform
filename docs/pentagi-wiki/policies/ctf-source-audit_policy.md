---
name: ctf-source-audit_policy
description: PentAGI safety policy for CTF Source Audit. CTF 挑战中的源码审计方法。当发现 .git 目录、.bak/.zip 备份、/proc/self/environ 泄露源码时使用。与真实代码审计不同——CTF 源码中的漏洞是故意设置的，通常只有 1-2 个关键点。先找危险函数（sink），再追溯输入（source）到该函数的路径。覆盖 PHP/Python/Node.js/Java 四种语言的危险函数和漏洞模式
when-to-use: Use when CTF Source Audit methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - todo_write
---

# CTF Source Audit — PentAGI Safety Policy

> Policy compiled from `ctf/ctf-source-audit/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ctf/ctf-source-audit/SKILL.md` for knowledge reference
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

Source: `ctf/ctf-source-audit/SKILL.md`
