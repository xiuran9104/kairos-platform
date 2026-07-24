---
name: nikto-scan_policy
description: PentAGI safety policy for Nikto Scan. 使用 Nikto 进行 Web 服务器漏洞扫描。当需要检测 Web 服务器的已知漏洞、过时软件版本、危险文件/CGI、配置错误时使用。Nikto 内置 7000+ 检查项，覆盖 OWASP 常见问题。任何涉及 Web 漏洞扫描、服务器安全检查、配置审计的场景都应使用此技能
when-to-use: Use when Nikto Scan methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Nikto Scan — PentAGI Safety Policy

> Policy compiled from `tool/nikto-scan/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/nikto-scan/SKILL.md` for knowledge reference
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

Source: `tool/nikto-scan/SKILL.md`
