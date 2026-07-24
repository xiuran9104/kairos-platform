---
name: browser-xterm-interaction_policy
description: PentAGI safety policy for Browser Xterm Interaction. Playwright Browser MCP 与 xterm.js 终端交互方法论。当需要通过浏览器操作网页内嵌终端、CTF 靶场伪终端、Cloud Shell、在线 IDE 中的终端时使用。覆盖终端内容读取（5 种方法）、命令执行、输出捕获、screenshot 降级策略。只要目标页面中有任何形式的 Web 终端（xterm.js/hterm/jQuery Terminal），就应使用此技能
when-to-use: Use when Browser Xterm Interaction methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Browser Xterm Interaction — PentAGI Safety Policy

> Policy compiled from `tool/browser-xterm-interaction/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/browser-xterm-interaction/SKILL.md` for knowledge reference
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

Source: `tool/browser-xterm-interaction/SKILL.md`
