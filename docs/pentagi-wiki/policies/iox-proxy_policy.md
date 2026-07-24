---
name: iox-proxy_policy
description: PentAGI safety policy for Iox Proxy. 使用 iox 进行端口转发和内网穿透。当需要把内网端口转发到外网、建立 SOCKS5 隧道、或做流量中转时使用。iox 是单二进制零依赖的端口转发/代理工具，支持正向代理、反向代理、端口映射、SOCKS5 代理，特别适合内网渗透中的流量隧道搭建。比 frp/nps 轻量，比 socat 功能更全。涉及端口转发、内网穿透、SOCKS5 代理、流量中转、隧道搭建的场景使用此技能
when-to-use: Use when Iox Proxy methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Iox Proxy — PentAGI Safety Policy

> Policy compiled from `tool/iox-proxy/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/iox-proxy/SKILL.md` for knowledge reference
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

Source: `tool/iox-proxy/SKILL.md`
