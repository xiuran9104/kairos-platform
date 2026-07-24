---
name: report-generate_policy
description: PentAGI safety policy for Report Generate. 渗透测试报告生成。当完成安全评估需要输出正式报告时使用。覆盖报告结构模板、风险评级标准（CVSS）、漏洞描述写法、修复建议规范。适用于正式渗透测试报告和红队评估报告
when-to-use: Use when Report Generate methodology is triggered and active testing may occur.
allowed-tools:
  - evidence_search
  - evidence_read
  - todo_write
---

# Report Generate — PentAGI Safety Policy

> Policy compiled from `general/report-generate/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `general/report-generate/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous

## Blocked Operations

- 未经授权的自动化操作
- 覆盖已有报告/证据

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

Source: `general/report-generate/SKILL.md`
