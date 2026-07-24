---
name: efficiency-rules_policy
description: PentAGI safety policy for Efficiency Rules. 渗透测试效率规则。防止盲目枚举、工具阻塞等低效行为，确保在有限时间内最大化漏洞发现数量。当开始渗透测试、发现扫描工具运行时间过长、陷入盲目爆破循环、或需要决策下一步攻击方向时使用。每轮渗透开始前建议阅读
when-to-use: Use when Efficiency Rules methodology is triggered and active testing may occur.
allowed-tools:
  - evidence_search
  - evidence_read
  - todo_write
---

# Efficiency Rules — PentAGI Safety Policy

> Policy compiled from `general/efficiency-rules/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `general/efficiency-rules/SKILL.md` for knowledge reference
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

Source: `general/efficiency-rules/SKILL.md`
