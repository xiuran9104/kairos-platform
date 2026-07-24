---
name: recon-full_policy
description: PentAGI safety policy for Recon Full. 主动式全流程资产侦察。当需要对目标进行从零到漏洞发现的完整侦察、渗透测试的第一阶段、或需要全面了解目标攻击面时使用。覆盖子域名枚举→端口扫描→存活检测→指纹识别→POC 扫描的完整链条
when-to-use: Use when Recon Full methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - dns_lookup
  - whois_lookup
  - http_probe
  - evidence_search
  - todo_write
---

# Recon Full — PentAGI Safety Policy

> Policy compiled from `recon/recon-full/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `recon/recon-full/SKILL.md` for knowledge reference
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

Source: `recon/recon-full/SKILL.md`
