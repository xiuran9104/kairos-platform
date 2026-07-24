---
name: ot-ics-attack_policy
description: PentAGI safety policy for Ot Ics Attack. OT/ICS 工控系统攻击方法论。当目标涉及 SCADA/DCS/PLC 系统、发现 Modbus/DNP3/OPC UA/EtherNet-IP 等工控协议、或需要评估关键基础设施安全时使用。覆盖工控协议利用、PLC 攻击、HMI 渗透、IT/OT 边界突破
when-to-use: Use when Ot Ics Attack methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Ot Ics Attack — PentAGI Safety Policy

> Policy compiled from `exploit/advanced/ot-ics-attack/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/advanced/ot-ics-attack/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 无 confirm_authorized 的主动利用
- 破坏性 payload（DROP/DELETE/rm -rf）
- 直接执行 OS 命令
- 写入 webshell 或后门文件

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

Source: `exploit/advanced/ot-ics-attack/SKILL.md`
