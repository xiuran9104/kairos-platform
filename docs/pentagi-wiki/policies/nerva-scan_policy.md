---
name: nerva-scan_policy
description: PentAGI safety policy for Nerva Scan. 使用 nerva 进行高性能服务指纹识别。当需要识别开放端口的具体服务时使用。nerva 支持 120+ 协议（数据库、远程访问、消息队列、工控、Web 等），是 fingerprintx 的升级版。任何涉及服务识别、协议探测、端口指纹的场景都应使用此技能
when-to-use: Use when Nerva Scan methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Nerva Scan — PentAGI Safety Policy

> Policy compiled from `tool/nerva-scan/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/nerva-scan/SKILL.md` for knowledge reference
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

Source: `tool/nerva-scan/SKILL.md`
