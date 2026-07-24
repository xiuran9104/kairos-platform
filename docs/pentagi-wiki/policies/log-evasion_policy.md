---
name: log-evasion_policy
description: PentAGI safety policy for Log Evasion. 日志分析与日志逃逸方法论。理解蓝队如何通过日志追踪攻击行为（SIEM/Event Log/Syslog），以及红队如何规避日志记录或精准清除痕迹。当需要设计无痕操作或分析日志监控覆盖范围时使用
when-to-use: Use when Log Evasion methodology is triggered and active testing may occur.
allowed-tools:
  - evidence_search
  - evidence_read
  - todo_write
---

# Log Evasion — PentAGI Safety Policy

> Policy compiled from `dfir/log-evasion/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `dfir/log-evasion/SKILL.md` for knowledge reference
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

Source: `dfir/log-evasion/SKILL.md`
