---
name: tlsx-probe_policy
description: PentAGI safety policy for Tlsx Probe. 使用 tlsx 进行 TLS 证书和配置分析。当需要提取 SSL/TLS 证书信息（SAN/CN）、检测 TLS 版本、加密套件、JARM/JA3 指纹、证书错误配置时使用。tlsx 是 ProjectDiscovery 出品的 TLS 探针工具。任何涉及 SSL/TLS 分析、证书信息收集、JARM 指纹的场景都应使用此技能
when-to-use: Use when Tlsx Probe methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Tlsx Probe — PentAGI Safety Policy

> Policy compiled from `tool/tlsx-probe/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/tlsx-probe/SKILL.md` for knowledge reference
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

Source: `tool/tlsx-probe/SKILL.md`
