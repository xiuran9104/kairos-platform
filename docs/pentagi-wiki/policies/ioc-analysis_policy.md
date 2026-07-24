---
name: ioc-analysis_policy
description: PentAGI safety policy for Ioc Analysis. IOC（失陷指标）分析与对抗方法论。蓝队视角：如何收集、富化、关联 IOC 进行威胁猎杀。红队视角：如何避免自身基础设施和工具产生可识别的 IOC，以及如何使 IOC 快速失效。当红队需要评估自身暴露面或规划 C2 基础设施时使用
when-to-use: Use when Ioc Analysis methodology is triggered and active testing may occur.
allowed-tools:
  - evidence_search
  - evidence_read
  - todo_write
---

# Ioc Analysis — PentAGI Safety Policy

> Policy compiled from `threat-intel/ioc-analysis/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `threat-intel/ioc-analysis/SKILL.md` for knowledge reference
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

Source: `threat-intel/ioc-analysis/SKILL.md`
