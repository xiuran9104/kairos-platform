---
name: js-api-extract_policy
description: PentAGI safety policy for Js Api Extract. JavaScript 静态分析提取隐藏 API 端点。当目标是 SPA/前后端分离架构、页面使用 Vue/React/Angular、或常规目录扫描收效甚微时使用。通过分析 JS bundle 提取 API 路径、密钥、内部域名、WebSocket 端点。是 API 渗透的信息收集阶段。发现端点后 → api-fuzz 进行语义化 fuzz
when-to-use: Use when Js Api Extract methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - dns_lookup
  - whois_lookup
  - http_probe
  - evidence_search
  - todo_write
---

# Js Api Extract — PentAGI Safety Policy

> Policy compiled from `recon/js-api-extract/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `recon/js-api-extract/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 端口扫描（需 nmap_scan policy 授权）
- 主动漏洞利用
- 社会工程学信息收集

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

Source: `recon/js-api-extract/SKILL.md`
