---
name: nuclei-scan_policy
description: PentAGI safety policy for Nuclei Scan. Nuclei 漏洞扫描工具使用方法论。当需要对目标进行已知漏洞扫描、CVE 验证、批量 PoC 检测时使用。Nuclei 拥有社区维护的 9000+ 模板，覆盖 CVE、默认口令、配置错误、信息泄露等。任何涉及 nuclei 扫描、CVE 批量验证、PoC 检测、漏洞模板搜索的场景都应使用此技能。也适用于需要从 nuclei 模板中提取 payload 用于手动利用的场景
when-to-use: Use when Nuclei Scan methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Nuclei Scan — PentAGI Safety Policy

> Policy compiled from `tool/nuclei-scan/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/nuclei-scan/SKILL.md` for knowledge reference
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

Source: `tool/nuclei-scan/SKILL.md`
