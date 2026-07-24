---
name: ctf-reverse_policy
description: PentAGI safety policy for CTF Reverse. CTF 逆向工程技术。当挑战提供未知二进制文件需要分析算法逻辑、游戏客户端需要破解验证、混淆代码需要还原、自定义 VM 需要解释执行时使用。覆盖 Ghidra/IDA 静态分析、GDB/Frida 动态调试、反调试绕过、WASM/.NET/APK/Python 字节码/Go/Rust 多平台逆向
when-to-use: Use when CTF Reverse methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - todo_write
---

# CTF Reverse — PentAGI Safety Policy

> Policy compiled from `ctf/ctf-reverse/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ctf/ctf-reverse/SKILL.md` for knowledge reference
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

Source: `ctf/ctf-reverse/SKILL.md`
