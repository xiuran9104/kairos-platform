---
name: threat-hunting-evasion_policy
description: PentAGI safety policy for Threat Hunting Evasion. 威胁猎杀原理与规避方法论。理解蓝队如何主动猎杀（Hypothesis-driven / IOC-driven / Analytics-driven），红队如何设计行为使自己不被猎杀到。当需要评估自身操作是否可被威胁猎杀发现时使用
when-to-use: Use when Threat Hunting Evasion methodology is triggered and active testing may occur.
allowed-tools:
  - evidence_search
  - evidence_read
  - todo_write
---

# Threat Hunting Evasion — PentAGI Safety Policy

> Policy compiled from `threat-intel/threat-hunting-evasion/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `threat-intel/threat-hunting-evasion/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous

## Blocked Operations

- 主动接触威胁 Actor
- 下载/执行恶意样本
- 在非隔离环境分析 IOC

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

Source: `threat-intel/threat-hunting-evasion/SKILL.md`
