---
name: fingerprintx-probe_policy
description: PentAGI safety policy for Fingerprintx Probe. 使用 fingerprintx 进行应用层服务指纹识别。当需要识别开放端口运行的具体服务和协议时使用。fingerprintx 支持 51 种协议识别（SSH/RDP/MySQL/Redis/HTTP/SMB/FTP 等），JSON 输出含服务元数据。任何涉及服务识别、协议探测、端口指纹的场景都应使用此技能
when-to-use: Use when Fingerprintx Probe methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Fingerprintx Probe — PentAGI Safety Policy

> Policy compiled from `tool/fingerprintx-probe/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/fingerprintx-probe/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 未经 command_check 验证直接执行工具
- 对非授权目标使用工具
- 高危参数（--os-shell/--os-cmd/破坏性操作）

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

Source: `tool/fingerprintx-probe/SKILL.md`
