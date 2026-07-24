---
name: disk-forensics-evasion_policy
description: PentAGI safety policy for Disk Forensics Evasion. 磁盘取证与反磁盘取证方法论。理解蓝队如何从磁盘恢复删除文件、提取时间线、分析文件系统 artifact，以及红队如何进行反取证（安全删除、时间戳篡改、痕迹清除）。当需要规避磁盘取证或进行磁盘分析时使用
when-to-use: Use when Disk Forensics Evasion methodology is triggered and active testing may occur.
allowed-tools:
  - evidence_search
  - evidence_read
  - todo_write
---

# Disk Forensics Evasion — PentAGI Safety Policy

> Policy compiled from `dfir/disk-forensics-evasion/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `dfir/disk-forensics-evasion/SKILL.md` for knowledge reference
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

Source: `dfir/disk-forensics-evasion/SKILL.md`
