---
name: httpx-probe_policy
description: PentAGI safety policy for Httpx Probe. 使用 httpx 进行 HTTP 探活、指纹识别和信息收集。当需要批量检测 HTTP/HTTPS 服务存活、识别 Web 技术栈、提取页面标题/状态码/响应头、CDN 检测、截图时使用。httpx 是 ProjectDiscovery 出品的多功能 HTTP 探针工具，是渗透测试管道中连接端口扫描和漏洞扫描的关键桥梁。任何涉及 HTTP 存活检测、Web 指纹识别、技术栈识别、批量 URL 探测的
when-to-use: Use when Httpx Probe methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Httpx Probe — PentAGI Safety Policy

> Policy compiled from `tool/httpx-probe/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/httpx-probe/SKILL.md` for knowledge reference
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

Source: `tool/httpx-probe/SKILL.md`
