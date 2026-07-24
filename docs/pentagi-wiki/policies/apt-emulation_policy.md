---
name: apt-emulation_policy
description: PentAGI safety policy for Apt Emulation. APT 模拟与情报驱动红队方法论。基于已知 APT 组织的 TTP（MITRE ATT&CK）设计红队行动计划。当需要模拟特定威胁组织、设计高仿真攻击演练、或根据威胁情报制定攻击策略时使用
when-to-use: Use when Apt Emulation methodology is triggered and active testing may occur.
allowed-tools:
  - evidence_search
  - evidence_read
  - todo_write
---

# Apt Emulation — PentAGI Safety Policy

> Policy compiled from `threat-intel/apt-emulation/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `threat-intel/apt-emulation/SKILL.md` for knowledge reference
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

Source: `threat-intel/apt-emulation/SKILL.md`
