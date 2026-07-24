---
name: subdomain-deep_policy
description: PentAGI safety policy for Subdomain Deep. 深度子域名挖掘，多源联合枚举。当需要最大化子域名发现覆盖率、常规 DNS 枚举结果不足、或目标使用了 CDN/通配符 DNS 时使用。联合 DNS 爆破、OSINT 引擎、爬虫三种方式交叉发现
when-to-use: Use when Subdomain Deep methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - dns_lookup
  - whois_lookup
  - http_probe
  - evidence_search
  - todo_write
---

# Subdomain Deep — PentAGI Safety Policy

> Policy compiled from `recon/subdomain-deep/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `recon/subdomain-deep/SKILL.md` for knowledge reference
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

Source: `recon/subdomain-deep/SKILL.md`
