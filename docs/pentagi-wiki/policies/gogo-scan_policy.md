---
name: gogo-scan_policy
description: PentAGI safety policy for Gogo Scan. 使用 gogo 进行端口扫描和指纹识别。gogo 是 chainreactors 出品的高速端口扫描器，支持主动/被动指纹识别、智能分组输出、自动 TLS 握手提取证书信息。和 fscan 的区别：gogo 专注于扫描精度和指纹覆盖（2000+ 指纹规则），而 fscan 兼顾弱口令和 POC。当需要精确识别目标服务和中间件版本（而不只是端口开放）时优先使用 gogo。涉及端口扫描、服务识别、指纹
when-to-use: Use when Gogo Scan methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Gogo Scan — PentAGI Safety Policy

> Policy compiled from `tool/gogo-scan/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/gogo-scan/SKILL.md` for knowledge reference
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

Source: `tool/gogo-scan/SKILL.md`
