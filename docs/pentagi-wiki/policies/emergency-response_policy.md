---
name: emergency-response_policy
description: PentAGI safety policy for Emergency Response. 应急响应方法论。当目标主机疑似被入侵、需要排查后门/木马/异常进程/异常账户、或需要进行安全事件溯源分析时使用。覆盖 Linux 和 Windows 双平台的账户审计、进程排查、网络连接分析、启动项/计划任务检查、日志分析、Rootkit 检测
when-to-use: Use when Emergency Response methodology is triggered and active testing may occur.
allowed-tools:
  - evidence_search
  - evidence_read
  - todo_write
---

# Emergency Response — PentAGI Safety Policy

> Policy compiled from `dfir/emergency-response/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `dfir/emergency-response/SKILL.md` for knowledge reference
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

Source: `dfir/emergency-response/SKILL.md`
