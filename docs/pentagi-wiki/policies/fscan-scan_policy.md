---
name: fscan-scan_policy
description: PentAGI safety policy for Fscan Scan. 使用 fscan 进行内网综合扫描与利用。当拿到内网立足点后需要做存活探测、端口扫描、弱口令爆破、漏洞检测时，fscan 是第一选择——单二进制零依赖，一条命令完成存活+端口+服务+弱口令+POC全流程。也支持 Redis 写公钥/计划任务、SSH 命令执行、MS17-010 利用、Pass-the-Hash、WMI 远程执行等后利用操作。任何涉及内网扫描、网段探测、弱口令批量检测、内网漏洞扫描、
when-to-use: Use when Fscan Scan methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Fscan Scan — PentAGI Safety Policy

> Policy compiled from `tool/fscan-scan/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/fscan-scan/SKILL.md` for knowledge reference
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

Source: `tool/fscan-scan/SKILL.md`
