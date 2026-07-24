---
name: memory-forensics-evasion_policy
description: PentAGI safety policy for Memory Forensics Evasion. 内存取证与反内存取证方法论。从蓝队视角理解内存取证如何发现恶意行为（Volatility3 分析流程），从红队视角掌握如何规避内存检测（进程隐藏、内存加密、痕迹清除）。当需要分析内存 dump 或设计反取证策略时使用
when-to-use: Use when Memory Forensics Evasion methodology is triggered and active testing may occur.
allowed-tools:
  - evidence_search
  - evidence_read
  - todo_write
---

# Memory Forensics Evasion — PentAGI Safety Policy

> Policy compiled from `dfir/memory-forensics-evasion/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `dfir/memory-forensics-evasion/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous

## Blocked Operations

- 修改/删除原始取证数据
- 在生产系统上运行取证工具
- 泄露取证数据

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

Source: `dfir/memory-forensics-evasion/SKILL.md`
