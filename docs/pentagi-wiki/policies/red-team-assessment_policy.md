---
name: red-team-assessment_policy
description: PentAGI safety policy for Red Team Assessment. 红队评估全流程方法论。当开始一个完整的渗透测试项目/红队评估、需要从侦察到报告的完整流程编排时使用。适用于外网打点、安全评估项目、定期渗透测试。本技能负责任务调度——具体漏洞利用调用专项 skills，不要在本技能内做深度漏洞测试
when-to-use: Use when Red Team Assessment methodology is triggered and active testing may occur.
allowed-tools:
  - evidence_search
  - evidence_read
  - todo_write
---

# Red Team Assessment — PentAGI Safety Policy

> Policy compiled from `general/red-team-assessment/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `general/red-team-assessment/SKILL.md` for knowledge reference
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

Source: `general/red-team-assessment/SKILL.md`
