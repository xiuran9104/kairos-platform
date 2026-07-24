---
name: ctf-flag-hunting_policy
description: PentAGI safety policy for CTF Flag Hunting. CTF 挑战中的 Flag 搜索策略。当通过 RCE/LFI/SQLi/webshell 等方式获得目标访问权限后、使用常规 ls/cat 命令找不到 flag 时使用。覆盖文件系统、数据库、环境变量、源码、内存等所有 flag 可能的存储位置。按成功率排序的搜索优先级——先试标准路径，再搜索全盘
when-to-use: Use when CTF Flag Hunting methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - todo_write
---

# CTF Flag Hunting — PentAGI Safety Policy

> Policy compiled from `ctf/ctf-flag-hunting/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ctf/ctf-flag-hunting/SKILL.md` for knowledge reference
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

Source: `ctf/ctf-flag-hunting/SKILL.md`
