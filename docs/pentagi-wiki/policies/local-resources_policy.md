---
name: local-resources_policy
description: PentAGI safety policy for Local Resources. 本地资源库导航——字典库(Dic)、Payload库、POC库的结构和使用方法。当需要使用 ffuf/spray 目录爆破、密码爆破、或构造 Fuzz payload 时必读。覆盖字典选择策略、payload 模板调用、POC 库搜索方法。字典库统一安装在 /pentest 目录下
when-to-use: Use when Local Resources methodology is triggered and active testing may occur.
allowed-tools:
  - evidence_search
  - evidence_read
  - todo_write
---

# Local Resources — PentAGI Safety Policy

> Policy compiled from `general/local-resources/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `general/local-resources/SKILL.md` for knowledge reference
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

Source: `general/local-resources/SKILL.md`
