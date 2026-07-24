---
name: nmap-scan_policy
description: PentAGI safety policy for Nmap Scan. 使用 nmap 进行端口扫描和服务识别。当需要对目标进行精细端口扫描、服务版本探测、操作系统指纹识别、NSE 脚本漏洞扫描时使用。nmap 是最经典的网络扫描器，支持 SYN/TCP/UDP/ACK 等多种扫描模式，内置 600+ NSE 脚本。任何涉及端口扫描、服务识别、漏洞脚本扫描、操作系统指纹的场景都应使用此技能。速度不如 naabu，但功能远超 naabu
when-to-use: Use when Nmap Scan methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# Nmap Scan — PentAGI Safety Policy

> Policy compiled from `tool/nmap-scan/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/nmap-scan/SKILL.md` for knowledge reference
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

Source: `tool/nmap-scan/SKILL.md`
