---
name: dirsearch-fuzz_policy
description: PentAGI safety policy for Dirsearch Fuzz. 使用 dirsearch 进行 Web 目录和文件暴力枚举。当需要发现隐藏的目录、文件、后台路径、备份文件时使用。dirsearch 内置高质量字典，支持多扩展名、递归扫描、状态码过滤。任何涉及目录枚举、路径发现、后台查找、敏感文件发现的场景都应使用此技能
when-to-use: Use when Dirsearch Fuzz methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Dirsearch Fuzz — PentAGI Safety Policy

> Policy compiled from `tool/dirsearch-fuzz/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/dirsearch-fuzz/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 未经 command_check 验证直接执行工具
- 对非授权目标使用工具
- 高危参数（--os-shell/--os-cmd/破坏性操作）

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

Source: `tool/dirsearch-fuzz/SKILL.md`
