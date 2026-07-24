---
name: ctf-forensics_policy
description: PentAGI safety policy for CTF Forensics. CTF 数字取证与信号分析技术。当挑战提供磁盘镜像(.dd/.E01)、内存 dump（原始内存镜像/.vmem）、网络抓包(.pcap/.pcapng)、隐写图片/音频、Windows 事件日志(.evtx)时使用。覆盖 Volatility 内存分析、Wireshark 流量还原、binwalk 隐写提取、文件系统恢复等取证全链路
when-to-use: Use when CTF Forensics methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - todo_write
---

# CTF Forensics — PentAGI Safety Policy

> Policy compiled from `ctf/ctf-forensics/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ctf/ctf-forensics/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous

## Blocked Operations

- 对非 CTF 目标使用
- 破坏 CTF 基础设施
- 泄露 flag 给未授权方

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

Source: `ctf/ctf-forensics/SKILL.md`
