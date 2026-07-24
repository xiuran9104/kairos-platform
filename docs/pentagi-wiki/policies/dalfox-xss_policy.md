---
name: dalfox-xss_policy
description: PentAGI safety policy for Dalfox Xss. 使用 DalFox 进行 XSS 漏洞扫描。当需要检测反射型/存储型/DOM XSS、分析参数注入点、绕过 WAF 时使用。DalFox 支持自动参数分析、DOM 挖掘、Blind XSS 回调、WAF 绕过、自动生成 PoC。任何涉及 XSS 漏洞检测、参数测试、WAF 绕过的场景都应使用此技能
when-to-use: Use when Dalfox Xss methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Dalfox Xss — PentAGI Safety Policy

> Policy compiled from `tool/dalfox-xss/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/dalfox-xss/SKILL.md` for knowledge reference
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

Source: `tool/dalfox-xss/SKILL.md`
