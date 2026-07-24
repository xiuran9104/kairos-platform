---
name: githacker-git-leak_policy
description: PentAGI safety policy for Githacker Git Leak. 使用 GitHacker 利用 .git 目录泄露漏洞恢复目标源码。GitHacker 是多线程 .git 泄露利用工具，相比 git-dumper/GitHack 能恢复更完整的内容——包括 stash、所有分支、标签、reflog。即使目标禁用了目录列表（403），也能通过暴力枚举恢复分支和标签。当目标 /.git/HEAD 或 /.git/config 返回 200、目录列表中发现 .git
when-to-use: Use when Githacker Git Leak methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Githacker Git Leak — PentAGI Safety Policy

> Policy compiled from `tool/githacker-git-leak/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/githacker-git-leak/SKILL.md` for knowledge reference
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

Source: `tool/githacker-git-leak/SKILL.md`
