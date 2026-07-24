---
name: firmware-analysis_policy
description: PentAGI safety policy for Firmware Analysis. 固件安全分析方法论。涵盖固件获取（从设备/下载/UART/JTAG）、固件解包与文件系统提取（binwalk/firmware-mod-kit）、静态分析（硬编码凭据/加密密钥/后门）、动态分析（QEMU 模拟）、漏洞挖掘、固件修改与重打包。当 Agent 需要分析嵌入式设备固件、提取固件中的敏感信息、或进行 IoT 设备安全评估时触发。
when-to-use: Use when Firmware Analysis methodology is triggered and active testing may occur.
allowed-tools:
  - evidence_search
  - evidence_read
  - todo_write
---

# Firmware Analysis — PentAGI Safety Policy

> Policy compiled from `hardware/firmware-analysis/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `hardware/firmware-analysis/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 物理接触目标设备
- 修改固件
- 旁路攻击（需独立授权）

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

Source: `hardware/firmware-analysis/SKILL.md`
