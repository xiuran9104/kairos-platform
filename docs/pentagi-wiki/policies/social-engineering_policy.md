---
name: social-engineering_policy
description: PentAGI safety policy for Social Engineering. 目标组织的人员情报收集与社工攻击准备。当需要收集目标邮箱格式、关键人员信息、组织架构，或准备钓鱼攻击/水坑攻击/凭据猜测时使用。适合渗透测试中的社会工程学攻击准备阶段
when-to-use: Use when Social Engineering methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - dns_lookup
  - whois_lookup
  - http_probe
  - evidence_search
  - todo_write
---

# Social Engineering — PentAGI Safety Policy

> Policy compiled from `recon/social-engineering/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `recon/social-engineering/SKILL.md` for knowledge reference
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

Source: `recon/social-engineering/SKILL.md`
