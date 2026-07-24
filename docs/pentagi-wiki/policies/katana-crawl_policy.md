---
name: katana-crawl_policy
description: PentAGI safety policy for Katana Crawl. 使用 katana 进行 Web 爬虫和 URL 发现。当需要爬取目标网站的所有页面和端点、发现隐藏的 API 路径、JavaScript 中的端点、表单提取时使用。katana 是 ProjectDiscovery 出品的下一代爬虫框架，支持标准模式和 Headless 浏览器模式，能解析 JavaScript 发现动态端点。任何涉及 Web 爬虫、URL 发现、JS 端点提取、表单发现、站点地
when-to-use: Use when Katana Crawl methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Katana Crawl — PentAGI Safety Policy

> Policy compiled from `tool/katana-crawl/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/katana-crawl/SKILL.md` for knowledge reference
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

Source: `tool/katana-crawl/SKILL.md`
