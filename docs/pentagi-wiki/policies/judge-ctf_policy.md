---
name: judge-ctf_policy
description: PentAGI safety policy for Judge Ctf. CTF flag 评判检查清单。当需要判断 CTF 挑战是否完成（flag 是否已获取）、分析攻击失败原因、或为下一步攻击提供精确指导时使用。覆盖 flag 搜索验证、漏洞发现评估、漏洞利用评估、flag 位置推断、常见题型模式匹配（SQLi/LFI/RCE/IDOR/SSRF/反序列化）
when-to-use: Use when Judge CTF methodology is triggered and active testing may occur.
allowed-tools:
  - evidence_search
  - evidence_read
  - todo_write
---

# Judge CTF — PentAGI Safety Policy

> Policy compiled from `general/judge-ctf/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `general/judge-ctf/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous

## Blocked Operations

- 未经授权的自动化操作
- 覆盖已有报告/证据

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

Source: `general/judge-ctf/SKILL.md`
