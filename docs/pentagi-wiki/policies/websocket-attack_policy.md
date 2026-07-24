---
name: websocket-attack_policy
description: PentAGI safety policy for Websocket Attack. WebSocket 安全测试。当目标使用 ws:// 或 wss:// 协议、页面 JS 中有 new WebSocket() 调用、或发现 101 Switching Protocols 响应时使用。覆盖 WS 劫持(CSWSH)、消息注入、认证绕过、信息泄露
when-to-use: Use when Websocket Attack methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Websocket Attack — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/websocket-attack/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/websocket-attack/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 无 confirm_authorized 的主动利用
- 破坏性 payload（DROP/DELETE/rm -rf）
- 直接执行 OS 命令
- 写入 webshell 或后门文件

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

Source: `exploit/web-method/websocket-attack/SKILL.md`
