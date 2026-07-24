---
name: xray-scan_policy
description: PentAGI safety policy for Xray Scan. 使用 xray 进行 Web 漏洞自动化扫描。当需要对 Web 应用进行全面漏洞扫描（XSS/SQLi/命令注入/SSRF/XXE/路径穿越/文件上传/弱口令等）时使用。xray 是长亭科技出品的综合性 Web 安全评估工具，支持主动扫描、被动代理扫描、基础爬虫扫描三种模式，内置丰富的检测插件和社区 POC。任何涉及 xray 漏洞扫描、Web 安全评估、被动代理扫描的场景都应使用此技能
when-to-use: Use when Xray Scan methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Xray Scan — PentAGI Safety Policy

> Policy compiled from `tool/xray-scan/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/xray-scan/SKILL.md` for knowledge reference
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

Source: `tool/xray-scan/SKILL.md`
