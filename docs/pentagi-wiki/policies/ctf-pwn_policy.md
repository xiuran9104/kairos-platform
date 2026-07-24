---
name: ctf-pwn_policy
description: PentAGI safety policy for CTF Pwn. CTF 二进制漏洞利用(Pwn)技术。当挑战提供 ELF/PE 可执行文件并开放 nc 端口、存在栈溢出/堆溢出/格式化字符串/UAF 漏洞时使用。覆盖 ROP 链构造、堆利用(tcache/House of Orange/Spirit)、内核利用、seccomp 沙箱逃逸、pwntools 自动化利用脚本编写
when-to-use: Use when CTF Pwn methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - todo_write
---

# CTF Pwn — PentAGI Safety Policy

> Policy compiled from `ctf/ctf-pwn/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ctf/ctf-pwn/SKILL.md` for knowledge reference
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

Source: `ctf/ctf-pwn/SKILL.md`
